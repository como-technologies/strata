//! Every rule of the catalog that names a test runs it: the check fails on the fault fixture and
//! passes on the clean one. A rule whose `when` does not hold on its fixture fails the test too,
//! so a guard cannot hide a check that never runs. Each fixture file under rules/fixtures/ belongs
//! to a rule, and each fixture a rule names exists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use strata_preflight::catalog;
use strata_preflight::fixture;
use strata_preflight::jq::Jq;
use strata_preflight::probe::{self, Call, FixtureBackend, LiveBackend, Outcome, Verdict};
use strata_preflight::report;
use strata_preflight::rule::{
    Catalog, Check, Evidence, EvidenceKind, ExecCheck, Owner, PaletteCheck, Rule, Severity,
};
use strata_preflight::world;

#[path = "../../../tests/common/walk.rs"]
mod walk;

use walk::walk;

// The lock of the tests of strata: a process that a test starts takes it.
#[path = "../../../tests/common/process.rs"]
#[allow(dead_code)]
mod process;
use process::CommandExt;

/// The request of the instance type route for the fixtures of the AWS, GCP, and Azure rules, as the
/// rules send it. Its query lifts the default filter of Palette, which drops each type with fewer
/// than 2 vCPUs or less than 4 GiB of memory (#393).
const AWS_TYPES: &str = "GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-aws-1&cpuGtEq=0&memoryGtEq=0";
const GCP_TYPES: &str = "GET /v1/clouds/gcp/regions/us-east1/instancetypes?cloudAccountUid=acc-gcp-1&cpuGtEq=0&memoryGtEq=0";
const AZURE_TYPES: &str = "GET /v1/clouds/azure/regions/eastus/instancetypes?cloudAccountUid=acc-azure-1&subscriptionId=11111111-2222-3333-4444-555555555555&cpuGtEq=0&memoryGtEq=0";

/// What Palette answers to a key that cannot read a route. A live run reads a 403 as an `ERROR`,
/// and the fixture backend does too.
fn forbidden() -> serde_json::Value {
    serde_json::json!({"ok": false, "status": 403, "body": {"code": "AccessDenied", "message": "no"}})
}

/// Run the check of `rule` on the fixture `doc`. The test fails when `when` does not hold on the
/// fixture: a guard that hides the check passes every test.
fn outcome_of(rule: &Rule, doc: serde_json::Value, jq: &Jq) -> Outcome {
    let (backend, world) = FixtureBackend::split(doc);
    if let Some(when) = &rule.when {
        let holds = jq
            .first(&rule.id, when, &world, None)
            .unwrap_or_else(|e| panic!("{}: when: {e}", rule.id));
        assert_eq!(
            holds,
            serde_json::Value::Bool(true),
            "{}: `when` ({when}) does not hold on the fixture, so the check would not run",
            rule.id
        );
    }
    match rule.check {
        Some(Check::Watch { .. }) => probe::judge_watch(rule, &world, jq),
        _ => probe::run_check(rule, &world, &backend, jq),
    }
}

fn verdict(rule: &Rule, fixture_path: &Path, jq: &Jq) -> (Verdict, Vec<String>) {
    let doc = fixture::load(fixture_path)
        .unwrap_or_else(|e| panic!("{}: fixture {}: {e}", rule.id, fixture_path.display()));
    let outcome = outcome_of(rule, doc, jq);
    (outcome.verdict, outcome.messages)
}

#[test]
fn every_rule_with_tests_fails_its_fault_and_passes_its_clean_fixture() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let mut tested = 0;
    let mut failures = Vec::new();
    for rule in catalog.rules() {
        for t in &rule.tests {
            if let Some(fault) = &t.fault {
                let (v, m) = verdict(rule, fault, &jq);
                if v != Verdict::Failed {
                    failures.push(format!(
                        "{}: the fault fixture {} gave {v:?} {m:?}, not FAIL",
                        rule.id,
                        fault.display()
                    ));
                }
                tested += 1;
            }
            if let Some(clean) = &t.clean {
                let (v, m) = verdict(rule, clean, &jq);
                if v != Verdict::Passed {
                    failures.push(format!(
                        "{}: the clean fixture {} gave {v:?} {m:?}, not PASS",
                        rule.id,
                        clean.display()
                    ));
                }
                tested += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(tested > 0);
}

/// The rules of the CAPA stack (#365) name what AWS refuses, and do not name what it allows.
#[test]
fn the_rules_of_the_capa_stack_name_what_aws_refuses() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let fault = |id: &str, i: usize| {
        let rule = catalog.rules().find(|r| r.id == id).unwrap();
        verdict(rule, rule.tests[i].fault.as_ref().unwrap(), &jq)
    };
    let (v, m) = fault("capa-stack-update-denied", 0);
    assert_eq!(v, Verdict::Failed);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(
        m[0].starts_with("iam:UpdateAssumeRolePolicy is not allowed on each of the 6 CAPA roles"),
        "{m:?}"
    );
    assert!(
        m[1].starts_with("iam:DetachRolePolicy is not allowed on each of the 6 CAPA roles"),
        "{m:?}"
    );
    let (_, m) = fault("capa-stack-release-behind", 0);
    assert_eq!(
        m,
        [
            "Palette 4.10.4 applied the CAPA stack last, and the tenant runs 4.10.12: the next create in the account updates the stack (capa-stack-update-denied tells if the role can apply the update)"
        ]
    );
    let (_, m) = fault("capa-bootstrap-stack", 2);
    assert!(
        m[0].contains("is UPDATE_ROLLBACK_FAILED in us-east-2: AWS refuses each update"),
        "{m:?}"
    );
    // With the stack managed by hand, Palette makes no stack, so neither new rule applies.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("rules/fixtures/eks");
    let world = |file: &str| FixtureBackend::split(fixture::load(&dir.join(file)).unwrap()).1;
    let (manual, managed) = (
        world("manual-stack.json"),
        world("capa-release-behind.json"),
    );
    for id in ["capa-stack-update-denied", "capa-stack-release-behind"] {
        let when = catalog
            .rules()
            .find(|r| r.id == id)
            .unwrap()
            .when
            .as_ref()
            .unwrap();
        assert_eq!(jq.first(id, when, &manual, None).unwrap(), false, "{id}");
        assert_eq!(jq.first(id, when, &managed, None).unwrap(), true, "{id}");
    }
}

/// Rule 7 of the book: a rule states what Palette does, and names no file or code of Palette.
/// Each name is a whole word: `hapi` is not a hit in "shaping".
const PALETTE_CODE: [&str; 6] = [
    "hubble",
    "hapi",
    "palette-sdk-go",
    "terraform-provider-spectrocloud",
    "optic src/",
    "pkg/",
];

/// True when `line` holds `name` as a whole word. A side of `name` that is a letter or a digit
/// must not touch a letter or a digit of `line`.
fn names_a_word(line: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric();
    line.match_indices(name).any(|(at, _)| {
        let before = line[..at].chars().next_back();
        let after = line[at + name.len()..].chars().next();
        let (first, last) = (name.chars().next(), name.chars().next_back());
        !(first.is_some_and(word) && before.is_some_and(word))
            && !(last.is_some_and(word) && after.is_some_and(word))
    })
}

/// True when `line` names a Go source file: `.go` with no letter or digit after it.
fn names_a_go_file(line: &str) -> bool {
    line.match_indices(".go").any(|(at, _)| {
        line[at + 3..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric())
    })
}

#[test]
fn no_rule_names_a_file_of_the_palette_code() {
    let mut found = Vec::new();
    for (name, text) in catalog::SECTIONS {
        for (n, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if PALETTE_CODE.iter().any(|m| names_a_word(&lower, m)) || names_a_go_file(&lower) {
                found.push(format!("{name}.yaml:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "rules that name the Palette code:\n{}",
        found.join("\n")
    );
}

/// Dotted names that a cloud defines and that are not Go: an Azure resource provider, and an AWS
/// error code with its reason.
const CLOUD_NAMES: [&str; 2] = ["Microsoft.", "InvalidAMIID.NotFound"];

/// True when `text` has a Go selector or call such as `pool.Validate(`: a name, a dot, and a name
/// that starts with a capital letter. A jq path starts with a dot, so `.stdout.Images` is not one.
/// A name in `CLOUD_NAMES` is not Go.
fn has_go_selector(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .filter(|token| !token.starts_with('.'))
        .filter(|token| !CLOUD_NAMES.iter().any(|n| token.starts_with(n)))
        .any(|token| {
            let mut parts = token.split('.').filter(|p| !p.is_empty());
            parts.next();
            parts.any(|p| p.starts_with(|c: char| c.is_ascii_uppercase()))
        })
}

/// True when `text` has a line of Go in a comment: `//` and a letter with no space between. A
/// URL has a colon before `//`, and the jq operator `//` has a space on each side.
fn has_commented_go(text: &str) -> bool {
    text.match_indices("//").any(|(at, _)| {
        !text[..at].ends_with([':', '/'])
            && text[at + 2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
    })
}

/// True when `text` has a commit id: 7 to 40 lower-case hex digits, with a digit and a letter among
/// them. A quota code such as `L-1216C47A` has capital letters, so it is not one.
fn has_commit_id(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        (7..=40).contains(&w.len())
            && w.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
            && w.chars().any(|c| c.is_ascii_digit())
            && w.chars().any(|c| c.is_ascii_lowercase())
    })
}

/// True when `text` has a call of a Go function with no selector, such as `CheckPool(`. The call
/// is a name that starts with a capital letter, and `(` right after it.
fn has_go_call(text: &str) -> bool {
    text.match_indices('(').any(|(at, _)| {
        let reversed: String = text[..at]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        reversed
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_uppercase())
    })
}

/// The form of Go code that a text of a rule holds, if it holds one. The texts are `what`, the
/// `text` of `fix`, and each `note`. The `check` field is jq and is not read here: jq has `!=` and
/// `.field`.
fn go_code_in(text: &str) -> Option<&'static str> {
    if has_go_selector(text) || has_go_call(text) {
        Some("a Go selector or call")
    } else if text.contains(":=") {
        Some("a Go assignment")
    } else if has_commented_go(text) {
        Some("a commented-out Go line")
    } else if has_commit_id(text) {
        Some("a commit id")
    } else {
        None
    }
}

/// One line for each text of `rule` that holds Go code: the `what`, the `text` of the fix, and
/// each `note`.
fn quoted_go_code(rule: &Rule) -> Vec<String> {
    let fix = rule.fix.iter().map(|f| ("fix", f.text.as_str()));
    let notes = rule.evidence.iter().map(|e| ("note", e.note.as_str()));
    std::iter::once(("what", rule.what.as_str()))
        .chain(fix)
        .chain(notes)
        .filter_map(|(field, text)| {
            go_code_in(text).map(|form| format!("{}: {field} has {form}: {text}", rule.id))
        })
        .collect()
}

#[test]
fn no_what_fix_or_note_text_quotes_go_code() {
    let catalog = catalog::builtin().unwrap();
    let found: Vec<String> = catalog.rules().flat_map(quoted_go_code).collect();
    assert!(
        found.is_empty(),
        "texts that quote Go code:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_go_code_test_refuses_a_bad_note_of_each_form() {
    let catalog = catalog::builtin().unwrap();
    let clean = catalog.rules().next().unwrap();
    assert!(quoted_go_code(clean).is_empty(), "{}", clean.id);
    for (form, bad) in [
        (
            "a Go selector or call",
            "`pool.Resolve(spec)` runs on each pool",
        ),
        (
            "a Go selector or call",
            "reads cfg.Spec.Region when it is empty",
        ),
        (
            "a Go selector or call",
            "Run CheckPool in the validate route; CheckPool(spec) fails",
        ),
        ("a Go assignment", "`found := lookup(id)`, then it returns"),
        ("a commented-out Go line", "//if limit > 3 is commented out"),
        (
            "a commit id",
            "the line is there since 1a2b3c4d5, 2020-01-01",
        ),
    ] {
        let mut rule = clean.clone();
        rule.evidence = vec![Evidence {
            kind: EvidenceKind::Source,
            reference: String::new(),
            note: bad.to_string(),
        }];
        let found = quoted_go_code(&rule);
        assert_eq!(found.len(), 1, "{bad}: {found:?}");
        assert!(found[0].contains(form), "{bad}: {found:?}");
        rule.evidence.clear();
        rule.what = bad.to_string();
        assert_eq!(quoted_go_code(&rule).len(), 1, "{bad}");
    }
}

#[test]
fn the_go_code_test_passes_jq_and_prose_and_the_names_of_a_cloud() {
    for good in [
        "`.exec.stdout.Images` is empty, and `.before != null` holds",
        "reads `.a // \"x\"` and `pack.podCIDR` at https://docs.example.com/page",
        "Palette lists no pack. The sync reads kubernetes.io and s3.amazonaws.com",
        "the image 20260929T041905Z and the version v1.27.0 stay",
        "the key type ED25519 and the quota L-1216C47A for Microsoft.Compute",
        "EC2 answers InvalidAMIID.NotFound for the image",
    ] {
        assert_eq!(go_code_in(good), None, "{good}");
    }
    assert!(names_a_word("the hapi server", "hapi"));
    assert!(!names_a_word("the shaping of a pool", "hapi"));
    assert!(names_a_word("see pkg/util", "pkg/"));
}

#[test]
fn each_source_entry_states_what_palette_does() {
    let catalog = catalog::builtin().unwrap();
    let silent: Vec<&str> = catalog
        .rules()
        .filter(|r| {
            r.evidence
                .iter()
                .any(|e| e.kind == EvidenceKind::Source && e.note.is_empty())
        })
        .map(|r| r.id.as_str())
        .collect();
    assert!(
        silent.is_empty(),
        "source entries with no note: {}",
        silent.join(", ")
    );
}

#[test]
fn every_rule_with_a_check_has_a_test() {
    let catalog = catalog::builtin().unwrap();
    let missing: Vec<&str> = catalog
        .rules()
        .filter(|r| r.check.is_some() && r.tests.is_empty())
        // a watch rule runs during a build; its evidence is a live run, and a fixture of the
        // events is welcome but not required
        .filter(|r| !matches!(r.check, Some(Check::Watch { .. })))
        .map(|r| r.id.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "rules with a check and no test: {}",
        missing.join(", ")
    );
}

#[test]
fn every_fixture_belongs_to_a_rule_and_every_named_fixture_exists() {
    let catalog = catalog::builtin().unwrap();
    let dir = catalog::rules_dir().join("fixtures");
    let mut named: BTreeSet<PathBuf> = BTreeSet::new();
    for rule in catalog.rules() {
        for t in &rule.tests {
            for p in [&t.fault, &t.clean].into_iter().flatten() {
                assert!(p.is_file(), "{}: no fixture {}", rule.id, p.display());
                named.insert(p.canonicalize().unwrap());
                // a base of a fixture is used through it
                let mut doc: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
                let mut at = p.clone();
                while let Some(base) = doc
                    .get("_base")
                    .and_then(|b| b.as_str())
                    .map(str::to_string)
                {
                    at = at.parent().unwrap().join(base);
                    named.insert(
                        at.canonicalize().unwrap_or_else(|_| {
                            panic!("{}: no base {}", p.display(), at.display())
                        }),
                    );
                    doc = serde_json::from_str(&std::fs::read_to_string(&at).unwrap()).unwrap();
                }
            }
        }
    }
    let mut orphans = Vec::new();
    for entry in walk(&dir) {
        if entry.extension().is_some_and(|e| e == "json")
            && !named.contains(&entry.canonicalize().unwrap())
        {
            orphans.push(entry.display().to_string());
        }
    }
    assert!(
        orphans.is_empty(),
        "fixtures no rule names: {}",
        orphans.join(", ")
    );
}

#[test]
fn the_ids_of_the_catalog_of_the_issue_are_all_here_or_listed_as_pending() {
    // the 149 rows of #91, by id. A row that is not a rule yet is in PENDING below. The count
    // there falls as the catalog grows. It never rises.
    let catalog = catalog::builtin().unwrap();
    let have: BTreeSet<&str> = catalog.rules().map(|r| r.id.as_str()).collect();
    let issue: BTreeSet<&str> = ISSUE_IDS.iter().copied().collect();
    let pending: BTreeSet<&str> = PENDING.iter().copied().collect();
    let missing: Vec<&&str> = issue
        .iter()
        .filter(|id| !have.contains(**id) && !pending.contains(**id))
        .collect();
    assert!(
        missing.is_empty(),
        "rows of #91 with no rule and not listed as pending: {missing:?}"
    );
    let done: Vec<&&str> = pending.iter().filter(|id| have.contains(**id)).collect();
    assert!(
        done.is_empty(),
        "rules that exist and are still listed as pending: {done:?}"
    );
}

/// The rows of the catalog of #91, in the order of its three comments. A row that
/// `ANSWERED_BY_THE_ROUTE` names is not here.
const ISSUE_IDS: &[&str] = &[
    // identity and permissions
    "eks-role-no-eks-policy",
    "role-partial-permissions",
    "sts-account-not-assumable",
    "iam-role-new",
    "eks-core-policies",
    "capa-bootstrap-stack",
    "capa-manual-stack",
    "global-policy-attach-silent",
    "iam-role-policy-quota",
    "ebs-csi-node-policy",
    "eks-irsa-roles",
    "eks-pod-identity-mapusers",
    "iaas-pod-identity",
    "gcp-account-roles",
    "gcp-default-sa-no-role",
    "gcp-default-sa-missing",
    "gcp-org-policies",
    "system-macros-hidden",
    // cloud services and APIs
    "gke-api-off",
    "gcp-apis-off",
    "aws-region-not-enabled",
    "aws-s3-irsa-bucket",
    "eks-fargate-govcloud",
    // quotas and limits
    "aws-quotas",
    "eks-quotas",
    "aws-sg-rules",
    "gcp-quotas",
    "worker-cpu-floor",
    "vcluster-min",
    // networking
    "eks-one-zone",
    "eks-unsupported-zone",
    "aws-dynamic-cidr-overlap",
    "aws-local-zone-letter",
    "eks-cidr-overlap",
    "gcp-dynamic-cidr-overlap",
    "gke-secondary-ranges",
    "subnet-ips",
    "private-egress",
    "static-subnets",
    "eks-public-subnet-mapping",
    "lb-subnet-tags",
    "sg-name-collision",
    "api-endpoint-reach",
    "eks-public-cidrs",
    "agent-egress",
    "proxy-no-proxy",
    "cilium-on-eks",
    "gcp-name-length",
    "registry-https",
    "igw-detach",
    // compute and pools
    "instance-offering",
    "spot-capacity",
    "eks-worker-needed",
    "eks-launch-template",
    "eks-fargate-private",
    "aws-ami",
    "gcp-image-not-shared",
    "root-volume",
    "dedicated-hosts",
    "gke-no-gpu",
    "gke-autopilot",
    "update-strategy-eks",
    "bastion-cost",
    // kubernetes version and engine
    "gke-no-release-channel",
    "gke-version-not-offered",
    "eks-version-not-offered",
    "eks-addon-version",
    "ccm-version",
    "pack-cloud-support",
    "engine-repave-rules",
    "eks-repave-answers",
    "gke-repave-answers",
    "skip-k8s-upgrade",
    // storage and CSI
    "csi-crash-after-running",
    "eks-default-storageclass",
    "eks-csi-iam",
    "kms",
    "eks-encryption-fixed",
    "gke-csi-builtin",
    "storageclass-immutable",
    "pvc-lb-leftover",
    // images and registries
    "helm-digest-frozen",
    "no-digest-kinds",
    "private-pack-registry-empty",
    "profile-follows-push",
    "floating-tag",
    "pack-state",
    "pack-dependency",
    "ecr-oci-images",
    "node-egress-images",
    "image-tag-not-digest",
    "zarf-password-log",
    "zarf-cosign",
    "helm-oci-endpoint",
    "registry-sync-delay",
    "registry-new-key",
    "ambiguous-pack-ref",
    // profiles and packs
    "profile-order",
    "position-zero",
    "edit-in-place",
    "template-lock-route",
    "template-outside",
    "variables-in-use",
    "version-string",
    "macro-flag-scope",
    // lifecycle
    "eks-type-change-orphan",
    "eks-delete-waits",
    "gke-type-change",
    "gke-pool-roll",
    "sweep-one-slot",
    "pool-replace-fields",
    "fixed-fields",
    "delete-pending-repave",
    "macro-repave",
    "macro-silent",
    "delete-while-provisioning",
    "pause-agent-upgrades",
    "delete-blockers",
    "override-silent",
    "push-without-resolve",
    // the Palette platform
    "status-not-cloud",
    "pivot-webhook-timeout",
    "stale-worker-condition",
    "validate-routes-unused",
    "conditions-not-cloud",
    "consumers-empty-on-error",
    "api-quirks",
    "transient-errors",
    "auto-cleanup-never",
    // client tools
    "provider-timeout-success",
    "provider-eks-azs",
    "provider-delete-timeout",
    "provider-skip-destroy",
    "provider-version",
    "ecr-token",
    "crane-empty-password",
    "cloud-cli-auth",
    "tools-present",
    "strata-binary-stale",
    // teardown and leftovers
    "force-delete-leaves-cloud",
    "teardown-order",
    "inventory-managed",
    "leftovers-before-build",
    "macro-undeletable",
    "versions-outside-state",
    "name-prefix",
];

/// The rows of #91 that are not rules yet. Each one becomes a rule, and leaves this list. The list
/// is empty: every row of #91 is a rule.
const PENDING: &[&str] = &[];

/// The rows of #91 that left the catalog, because the validate route of the cluster answers the
/// check: `gke-pool-multiple-3` (#243), `profile-cloud-mismatch` (#415), and `eks-ami-type`
/// (#409). A call of the route replaces each one: see "A check that a validate route answers" in
/// the rule format of the book.
const ANSWERED_BY_THE_ROUTE: &[&str] = &[
    "gke-pool-multiple-3",
    "profile-cloud-mismatch",
    "eks-ami-type",
];

#[test]
fn a_check_that_the_validate_route_answers_is_not_a_rule_of_the_catalog() {
    let catalog = catalog::builtin().unwrap();
    let back: Vec<&&str> = ANSWERED_BY_THE_ROUTE
        .iter()
        .filter(|id| catalog.rule(id).is_some())
        .collect();
    assert!(back.is_empty(), "a copy of the route is back: {back:?}");
}

/// A rule that no validate route answers stays in the catalog as a gap that preflight fills. Its
/// note names the route that takes the fault (#415).
#[test]
fn a_rule_that_the_cluster_validate_route_does_not_answer_says_so_in_its_note() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("pack-cloud-support").unwrap();
    assert!(rule.what.contains("no route answers"), "{}", rule.what);
    assert!(rule.what.contains("palette-validate"), "{}", rule.what);
}

/// The fault is in the second item, not the first (#144). The cases are a profile that comes
/// second and a host of the second pool. The other cases are a second pool with a host resource
/// group and a second compute cluster. Each check reads every item, and each message names the
/// item that holds the fault and no other.
#[test]
fn a_rule_with_each_finds_the_fault_of_a_later_item_and_names_only_that_item() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    for (id, fixture, named, other) in [
        (
            "template-outside",
            "template-linked-second.json",
            "prof-addon-1",
            "prof-eks-1",
        ),
        (
            "edge-host-in-use",
            "host-in-use-second-pool.json",
            "edge-host-w-2",
            "edge-host-cp-1",
        ),
        (
            "edge-host-health",
            "host-unhealthy-second-pool.json",
            "edge-host-w-2",
            "edge-host-w-1",
        ),
        (
            "edge-host-arch",
            "host-arm64-second-pool.json",
            "edge-host-w-2",
            "edge-host-w-1",
        ),
        (
            "edge-host-resources",
            "host-small-second-pool.json",
            "edge-host-w-2",
            "edge-host-w-1",
        ),
        (
            "edge-host-nic",
            "nic-unknown-second-pool.json",
            "edge-host-w-2",
            "edge-host-w-1",
        ),
        (
            "edge-agent-version",
            "agent-old-second-pool.json",
            "edge-host-w-2",
            "edge-host-w-1",
        ),
        (
            "dedicated-hosts",
            "hrg-second-pool-no-capacity.json",
            "pool worker-pool",
            "pool cp-pool",
        ),
        (
            "vsphere-compute-resources",
            "datastore-missing-second-pool.json",
            "pool worker-pool",
            "pool cp-pool",
        ),
        (
            "vsphere-compute-resources",
            "datastore-missing-second-cluster.json",
            "compute cluster Cluster02",
            "compute cluster Cluster01",
        ),
    ] {
        let rule = catalog.rule(id).unwrap();
        let fault = rule
            .tests
            .iter()
            .filter_map(|t| t.fault.as_ref())
            .find(|p| p.ends_with(fixture))
            .unwrap_or_else(|| panic!("{id} names no test with {fixture}"));
        let (v, messages) = verdict(rule, fault, &jq);
        assert_eq!(v, Verdict::Failed, "{id}: {messages:?}");
        assert!(
            messages
                .iter()
                .all(|m| m.contains(named) && !m.contains(other)),
            "{id}: every message names {named} and not {other}: {messages:?}"
        );
    }
}

#[test]
fn a_rule_with_each_asks_for_every_host_of_every_pool() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("edge-host-in-use").unwrap();
    let clean = rule.tests[0].clean.as_ref().unwrap();
    let out = outcome_of(rule, fixture::load(clean).unwrap(), &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
    assert_eq!(
        out.ran.requests().iter().map(Call::ran).collect::<Vec<_>>(),
        [
            "GET /v1/edgehosts/edge-host-cp-1",
            "GET /v1/edgehosts/edge-host-w-1",
            "GET /v1/edgehosts/edge-host-w-2",
        ]
    );
}

/// The messages of `instance-offering` name the pool, the type, and the zone (#143), also when the
/// region does not list the type. The rule makes one request, and it answers every pool.
#[test]
fn instance_offering_names_the_pool_the_type_and_the_zone_for_aws_and_gcp() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    for (fixture, ran, expected) in [
        (
            "fixtures/aws/instance-zone-not-offered.json",
            AWS_TYPES,
            vec![
                "pool cp-pool: the type t3.xlarge is not offered in the zone us-east-2a",
                "pool worker-pool: the type t3.xlarge is not offered in the zone us-east-2a",
            ],
        ),
        (
            "fixtures/aws/instance-type-not-listed.json",
            AWS_TYPES,
            vec![
                "pool cp-pool: the type t3.xlarge is not in the list of region us-east-2",
                "pool worker-pool: the type t3.xlarge is not in the list of region us-east-2",
            ],
        ),
        (
            "fixtures/gcp/instance-zone-not-offered.json",
            GCP_TYPES,
            vec![
                "pool cp-pool: the type e2-standard-4 is not offered in the zone us-east1-b",
                "pool worker-pool: the type e2-standard-4 is not offered in the zone us-east1-b",
            ],
        ),
    ] {
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        assert_eq!(
            out.verdict,
            Verdict::Failed,
            "{fixture}: {:?}",
            out.messages
        );
        assert_eq!(out.messages, expected, "{fixture}");
        assert_eq!(
            out.ran.text().as_deref(),
            Some(ran),
            "{fixture}: one request answers every pool"
        );
    }
}

/// `instance-offering` reads the type of an entry from `type`, as the instance type route of
/// Palette gives it (#404). An entry has no `name`: a list with `name` alone names no type.
#[test]
fn instance_offering_reads_the_type_of_an_entry_from_type() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/instance-zone-offered.json");
    let key = AWS_TYPES;
    let live = serde_json::json!({"category": "GPU instance", "cpu": 32, "memory": 488,
        "price": 7.2, "type": "p2.8xlarge",
        "cost": {"price": [{"onDemand": 7.2, "os": "linux"}]},
        "nonSupportedZones": ["us-east-2a", "us-east-2b", "us-east-2c"],
        "supportedArchitectures": ["x86_64"]});
    let named = serde_json::json!({"name": "p2.8xlarge", "nonSupportedZones": []});
    for (entry, zone, expected) in [
        (&live, "us-east-2d", vec![]),
        (
            &live,
            "us-east-2a",
            vec![
                "pool cp-pool: the type p2.8xlarge is not offered in the zone us-east-2a",
                "pool worker-pool: the type p2.8xlarge is not offered in the zone us-east-2a",
            ],
        ),
        (
            &named,
            "us-east-2d",
            vec![
                "pool cp-pool: the type p2.8xlarge is not in the list of region us-east-2",
                "pool worker-pool: the type p2.8xlarge is not in the list of region us-east-2",
            ],
        ),
    ] {
        let mut doc = fixture::load(&path).unwrap();
        doc["_responses"][key]["body"] = serde_json::json!({ "instanceTypes": [entry] });
        for pool in doc["entity"]["spec"]["machinepoolconfig"]
            .as_array_mut()
            .unwrap()
        {
            pool["cloudConfig"]["instanceType"] = "p2.8xlarge".into();
            pool["cloudConfig"]["azs"] = serde_json::json!([zone]);
        }
        let out = outcome_of(rule, doc, &Jq::new());
        assert_eq!(out.messages, expected, "{entry}");
    }
}

/// Palette gives `null` in `nonSupportedZones` of a type that each zone offers, and most types are
/// so (#411). `instance-offering` reads `null` as no zone: a pool of that type passes in each zone.
#[test]
fn instance_offering_passes_a_pool_whose_type_has_null_in_non_supported_zones_in_each_zone() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    for (fixture, key, kind, zones) in [
        (
            "fixtures/aws/instance-zone-null.json",
            AWS_TYPES,
            "t3.xlarge",
            ["us-east-2a", "us-east-2b", "us-east-2c"],
        ),
        (
            "fixtures/gcp/instance-zone-null.json",
            GCP_TYPES,
            "e2-standard-4",
            ["us-east1-b", "us-east1-c", "us-east1-d"],
        ),
    ] {
        let path = catalog::rules_dir().join(fixture);
        for zone in zones {
            let mut doc = fixture::load(&path).unwrap();
            let listed = doc["_responses"][key]["body"]["instanceTypes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["type"] == kind)
                .unwrap();
            assert!(listed["nonSupportedZones"].is_null(), "{fixture}: {listed}");
            for pool in doc["entity"]["spec"]["machinepoolconfig"]
                .as_array_mut()
                .unwrap()
            {
                assert_eq!(pool["cloudConfig"]["instanceType"], kind, "{fixture}");
                pool["cloudConfig"]["azs"] = serde_json::json!([zone]);
            }
            let out = outcome_of(rule, doc, &Jq::new());
            assert_eq!(
                out.verdict,
                Verdict::Passed,
                "{fixture} in {zone}: {:?}",
                out.messages
            );
        }
    }
}

/// A pool with no zone, as a GKE pool of a region, is checked against the list of the region alone.
#[test]
fn instance_offering_checks_a_pool_with_no_zone_against_the_list_of_the_region() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    let path = catalog::rules_dir().join("fixtures/gcp/instance-zone-not-offered.json");
    let mut doc = fixture::load(&path).unwrap();
    for pool in doc["entity"]["spec"]["machinepoolconfig"]
        .as_array_mut()
        .unwrap()
    {
        pool["cloudConfig"]["azs"] = serde_json::Value::Null;
    }
    let out = outcome_of(rule, doc, &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
}

/// The check of `instance-offering` fails, and says so, when Palette gives no list, or refuses the
/// request with a 4xx that is an answer. A wrong answer is never a pass. A 403 is the key, so it
/// is an `ERROR` on a live run, and the fixture backend gives the same verdict.
#[test]
fn instance_offering_fails_when_palette_refuses_or_gives_no_list_of_types() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/instance-zone-offered.json");
    let key = AWS_TYPES;
    for (answer, expected) in [
        (
            serde_json::json!({"ok": false, "status": 404, "body": {"code": "ResourceNotFound", "message": "no"}}),
            "Palette cannot list the instance types of region us-east-2: ResourceNotFound: no",
        ),
        (
            serde_json::json!({"ok": true, "status": 200, "body": {}}),
            "Palette gave no list of instance types for region us-east-2",
        ),
    ] {
        let mut doc = fixture::load(&path).unwrap();
        doc["_responses"][key] = answer;
        let out = outcome_of(rule, doc, &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
        assert_eq!(out.messages, [expected]);
    }
    let mut doc = fixture::load(&path).unwrap();
    doc["_responses"][key] = forbidden();
    let out = outcome_of(rule, doc, &Jq::new());
    assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
    assert!(out.messages[0].contains("HTTP 403"), "{:?}", out.messages);
}

/// The three quota rules count the vCPUs of a pool from the field `cpu` of the instance type route
/// (#393). They do not count from the name of the type. Each sends one request beside its
/// command. The quota still comes from the tool of the cloud.
#[test]
fn the_quota_rules_count_the_vcpus_of_a_pool_from_the_cpu_that_palette_lists() {
    let catalog = catalog::builtin().unwrap();
    for (id, fixture, ran, expected) in [
        (
            "aws-quotas",
            "fixtures/aws/vcpu-quota-low.json",
            AWS_TYPES,
            "the on-demand vCPU quota L-1216C47A in us-east-2 is 8.0, and the pools ask for 20 vCPUs: 1 x t3.xlarge, 2 x m5.2xlarge",
        ),
        (
            "gcp-quotas",
            "fixtures/gke/cpu-quota-low.json",
            GCP_TYPES,
            "CPUS in us-east1: limit 24.0, usage 20.0, free 4.0; the pools ask for 12 CPUs: 3 x e2-standard-4",
        ),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-low.json",
            AZURE_TYPES,
            "the pools ask for 12 vCPUs in eastus, and the regional quota has 2 free of 10 (8 in use)",
        ),
    ] {
        let rule = catalog.rule(id).unwrap();
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{id}: {:?}", out.messages);
        assert!(
            out.messages[0].starts_with(expected),
            "{id}: {:?}",
            out.messages
        );
        assert_eq!(out.ran.requests().len(), 1, "{id}: one request of Palette");
        assert_eq!(out.ran.requests()[0].ran(), ran, "{id}");
    }
}

/// Palette drops each type with fewer than 2 vCPUs or less than 4 GiB of memory from the instance
/// type route. It does so when the query says nothing. The three quota rules send
/// `cpuGtEq=0&memoryGtEq=0`, so a pool of a small type adds its vCPUs. The pools of the fixtures
/// ask for more than the quota gives, and the rule says so. With no query, the route lists no
/// small type, and the rule counts 0 vCPUs for that pool.
#[test]
fn the_quota_rules_ask_for_the_small_types_that_palette_drops_by_default_and_count_them() {
    let catalog = catalog::builtin().unwrap();
    for (id, fixture, ran, expected) in [
        (
            "aws-quotas",
            "fixtures/aws/vcpu-quota-small-type.json",
            AWS_TYPES,
            "the on-demand vCPU quota L-1216C47A in us-east-2 is 8.0, and the pools ask for 44 vCPUs: 1 x t3.xlarge, 20 x t3.small",
        ),
        (
            "gcp-quotas",
            "fixtures/gke/cpu-quota-small-type.json",
            GCP_TYPES,
            "CPUS in us-east1: limit 24.0, usage 20.0, free 4.0; the pools ask for 8 CPUs: 4 x e2-small",
        ),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-small-size.json",
            AZURE_TYPES,
            "the pools ask for 8 vCPUs in eastus, and the regional quota has 2 free of 10 (8 in use). Azure refuses the machines with OperationNotAllowed. The family quota of each type can be lower.",
        ),
    ] {
        let rule = catalog.rule(id).unwrap();
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{id}: {:?}", out.messages);
        assert_eq!(out.messages, [expected], "{id}");
        assert_eq!(out.ran.requests()[0].ran(), ran, "{id}");
        assert!(
            ran.ends_with("&cpuGtEq=0&memoryGtEq=0"),
            "{id}: the query lifts both filters of Palette"
        );
    }
    // the same pools, with a quota that covers them, pass
    let rule = catalog.rule("aws-quotas").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/vcpu-quota-small-type-clean.json");
    let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
}

/// A type that the route still does not list, such as an old family, does not add 0 vCPUs. The
/// rule fails closed for that pool, and names the pool and the type. A listed entry with no number
/// in `cpu` is the same. The message of a short quota counts the listed pools alone.
#[test]
fn a_quota_rule_fails_for_a_pool_whose_type_the_route_does_not_list() {
    let catalog = catalog::builtin().unwrap();
    for (id, fixture, expected) in [
        (
            "aws-quotas",
            "fixtures/aws/vcpu-quota-type-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the type m3.large in region us-east-2. The check cannot count the vCPUs of the pool.",
        ),
        (
            "gcp-quotas",
            "fixtures/gke/cpu-quota-type-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the type n1-standard-1 in region us-east1. The check cannot count the CPUs of the pool.",
        ),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-size-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the size Standard_A1 in region eastus. The check cannot count the vCPUs of the pool.",
        ),
    ] {
        let rule = catalog.rule(id).unwrap();
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{id}: {:?}", out.messages);
        assert_eq!(out.messages, [expected], "{id}: the quota covers the rest");
    }
    let rule = catalog.rule("aws-quotas").unwrap();
    let exec = "aws service-quotas get-service-quota --service-code ec2 --quota-code L-1216C47A --region us-east-2 --output json";
    let path = catalog::rules_dir().join("fixtures/aws/vcpu-quota-type-not-listed.json");
    let mut short = fixture::load(&path).unwrap();
    short["_exec"][exec]["stdout"]["Quota"]["Value"] = serde_json::json!(2.0);
    let out = outcome_of(rule, short, &Jq::new());
    assert_eq!(
        out.messages,
        [
            "the on-demand vCPU quota L-1216C47A in us-east-2 is 2.0, and the pools ask for 4 vCPUs: 1 x t3.xlarge",
            "pool worker-pool: the instance type route of Palette lists no cpu for the type m3.large in region us-east-2. The check cannot count the vCPUs of the pool.",
        ]
    );
    let mut no_number = fixture::load(&path).unwrap();
    no_number["entity"]["spec"]["machinepoolconfig"][1]["cloudConfig"]["instanceType"] =
        serde_json::json!("t3.xlarge");
    no_number["_responses"][AWS_TYPES]["body"]["instanceTypes"][0]["cpu"] = serde_json::json!("4");
    let out = outcome_of(rule, no_number, &Jq::new());
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert!(
        out.messages
            .iter()
            .any(|m| m.contains("lists no cpu for the type t3.xlarge")),
        "{:?}",
        out.messages
    );
}

/// A list that Palette refuses with a 4xx that is an answer, or gives no `instanceTypes`, is a
/// failure that says so. A quota rule cannot count the vCPUs of any pool then. A 403 is the key,
/// so it is an `ERROR`, as on a live run.
#[test]
fn a_quota_rule_fails_when_palette_refuses_the_list_of_instance_types() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("aws-quotas").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/vcpu-quota-clean.json");
    let doc = fixture::load(&path).unwrap();
    for (answer, expected) in [
        (
            serde_json::json!({"ok": false, "status": 404, "body": {"code": "ResourceNotFound", "message": "no"}}),
            "Palette cannot list the instance types of region us-east-2: ResourceNotFound: no",
        ),
        (
            serde_json::json!({"ok": true, "status": 200, "body": {}}),
            "Palette gave no list of instance types for region us-east-2",
        ),
    ] {
        let mut refused = doc.clone();
        refused["_responses"][AWS_TYPES] = answer;
        let out = outcome_of(rule, refused, &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
        assert_eq!(out.messages, [expected]);
    }
    let mut forbidden_list = doc;
    forbidden_list["_responses"][AWS_TYPES] = forbidden();
    let out = outcome_of(rule, forbidden_list, &Jq::new());
    assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
    assert!(out.messages[0].contains("HTTP 403"), "{:?}", out.messages);
}

/// The message of `ssh-key-pair` names the key pair and the region, for EKS with a dynamic VPC
/// (#143).
#[test]
fn ssh_key_pair_names_the_key_pair_and_the_region_for_eks() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("ssh-key-pair").unwrap();
    let fixture = "fixtures/eks/ssh-key-missing.json";
    let doc = fixture::load(&catalog::rules_dir().join(fixture)).unwrap();
    assert_eq!(
        doc["entity"]["spec"]["cloudConfig"]["vpcId"], "",
        "a dynamic VPC"
    );
    let out = outcome_of(rule, doc, &Jq::new());
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(
        out.messages,
        [
            "sshKeyName iris-on-palette is not a key pair of the account in region us-east-2; Palette lists 1 key pair"
        ]
    );
}

/// The property route of AWS refuses a key pair that the region lacks, and `iaas-pod-identity`
/// sends the key pair to it. So `ssh-key-pair` names EKS alone, and `iaas-pod-identity` names the
/// properties, not only the account (#430).
#[test]
fn ssh_key_pair_leaves_aws_to_the_properties_route_that_iaas_pod_identity_calls() {
    let catalog = catalog::builtin().unwrap();
    let key_pair = catalog.rule("ssh-key-pair").unwrap();
    assert!(key_pair.applies_to("eks"));
    // each engine but EKS, so that a new engine gets the check too
    let others: Vec<&str> = world::clouds().filter(|engine| *engine != "eks").collect();
    assert!(others.contains(&"aws"), "{others:?}");
    for engine in others {
        assert!(!key_pair.applies_to(engine), "{engine}");
    }
    let pod = catalog.rule("iaas-pod-identity").unwrap();
    assert!(pod.applies_to("aws"));
    let Some(Check::Palette(PaletteCheck { body, .. })) = &pod.check else {
        panic!("iaas-pod-identity calls the property route");
    };
    assert!(body.as_deref().unwrap().contains("sshKeyName"));
    let path = catalog::rules_dir().join("fixtures/aws/key-pair-not-found.json");
    let out = outcome_of(pod, fixture::load(&path).unwrap(), &Jq::new());
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(
        out.messages,
        [
            "Palette refuses the properties of the AWS cluster, the cloud account or the key pair: AwsKeyPairNotFound: Aws KeyPair 'strata-no-such-key' not found for region us-east-2; caused by: InvalidKeyPair.NotFound: The key pair 'strata-no-such-key' does not exist"
        ]
    );
}

/// The property route of AWS answers HTTP 500 for a key pair that the region lacks. The live
/// backend reads a 5xx as an error unless the check names its code. A run on AWS found that
/// `iaas-pod-identity` was an `error` and never `failed` (#546). The rule names `AwsKeyPairNotFound`
/// in `refusal_codes`, so the live backend and the fixture both read the 500 as the answer.
#[test]
fn iaas_pod_identity_fails_on_the_http_500_that_the_property_route_gives_for_a_key_pair() {
    const ROUTE: &str = "POST /v1/clouds/aws/properties/validate";
    const REFUSAL: &str = r#"{"code":"AwsKeyPairNotFound","message":"Aws KeyPair 'strata-no-such-key' not found for region us-east-2"}"#;
    let catalog = catalog::builtin().unwrap();
    let pod = catalog.rule("iaas-pod-identity").unwrap();
    let Some(Check::Palette(PaletteCheck { refusal_codes, .. })) = &pod.check else {
        panic!("iaas-pod-identity calls the property route");
    };
    assert_eq!(
        refusal_codes,
        &["AwsKeyPairNotFound", "FeatureNotSupported"]
    );
    let path = catalog::rules_dir().join("fixtures/aws/key-pair-not-found.json");
    let (_, world) = FixtureBackend::split(fixture::load(&path).unwrap());
    let jq = Jq::new();
    let live = |rule: &Rule, status: u16, body: &str| {
        let api = strata::api::MemoryApi::new().failing(ROUTE, status, body);
        probe::run_check(rule, &world, &LiveBackend::new(Some(&api), false), &jq)
    };
    // the live backend: a 500 with the code is the answer, and the rule fails
    let out = live(pod, 500, REFUSAL);
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(
        out.messages,
        [
            "Palette refuses the properties of the AWS cluster, the cloud account or the key pair: AwsKeyPairNotFound: Aws KeyPair 'strata-no-such-key' not found for region us-east-2"
        ]
    );
    // a 500 with another code is an error of the check, and not a catch
    let out = live(pod, 500, r#"{"code":"InternalError","message":"boom"}"#);
    assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
    // the rule without the key reads the 500 as an error: the fault that the run found
    let mut bare = pod.clone();
    let Some(Check::Palette(check)) = &mut bare.check else {
        unreachable!("checked above");
    };
    check.refusal_codes.clear();
    let out = live(&bare, 500, REFUSAL);
    assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
    assert!(out.messages[0].contains("HTTP 500"), "{:?}", out.messages);
    // the fixture keeps the same rule, so it cannot show a catch that a live run does not give
    let out = outcome_of(&bare, fixture::load(&path).unwrap(), &jq);
    assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
}

/// The property route of AWS checks the account before the key pair, and refuses a Pod Identity
/// account with `FeatureNotSupported`. Palette gives that refusal the type of a server fault, so
/// the answer is HTTP 500. No live run shows it yet. The fixture states the 500, so it cannot show
/// a catch that a live run does not give (#553).
#[test]
fn iaas_pod_identity_fails_on_the_http_500_that_the_property_route_gives_for_a_pod_identity_account()
 {
    const ROUTE: &str = "POST /v1/clouds/aws/properties/validate";
    const REFUSAL: &str = r#"{"code":"FeatureNotSupported","message":"Pod Identity for AWS infrastructure clusters is not supported"}"#;
    const MESSAGE: &str = "Palette refuses the properties of the AWS cluster, the cloud account or the key pair: FeatureNotSupported: Pod Identity for AWS infrastructure clusters is not supported";
    let catalog = catalog::builtin().unwrap();
    let pod = catalog.rule("iaas-pod-identity").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/pod-identity-iaas.json");
    let doc = fixture::load(&path).unwrap();
    assert_eq!(doc["_responses"][ROUTE]["status"], 500);
    let (_, world) = FixtureBackend::split(doc.clone());
    let jq = Jq::new();
    let live = |rule: &Rule, status: u16| {
        let api = strata::api::MemoryApi::new().failing(ROUTE, status, REFUSAL);
        probe::run_check(rule, &world, &LiveBackend::new(Some(&api), false), &jq)
    };
    // the live backend and the fixture: the 500 with the code is the answer, and the rule fails
    let out = live(pod, 500);
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(out.messages, [MESSAGE]);
    let out = outcome_of(pod, doc.clone(), &jq);
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(out.messages, [MESSAGE]);
    // the rule that names the key pair alone read this 500 as an error (#553)
    let mut key_pair_only = pod.clone();
    let Some(Check::Palette(check)) = &mut key_pair_only.check else {
        unreachable!("checked above");
    };
    check.refusal_codes = vec!["AwsKeyPairNotFound".into()];
    for out in [
        live(&key_pair_only, 500),
        outcome_of(&key_pair_only, doc, &jq),
    ] {
        assert_eq!(out.verdict, Verdict::Error, "{:?}", out.messages);
        assert!(out.messages[0].contains("HTTP 500"), "{:?}", out.messages);
    }
}

/// The check reads a refusal at a 5xx one time. The library sends a request again after a 5xx, and
/// the codes of a refusal are the exception. Preflight connects through `Retry`. A code that the
/// rule names and the library does not list gets four requests and a wait of seven seconds (#553).
/// The two codes of the property route of AWS get one request.
#[test]
fn the_refusals_of_the_property_route_of_aws_are_read_after_one_request_and_not_after_the_retries()
{
    const ROUTE: &str = "POST /v1/clouds/aws/properties/validate";
    let catalog = catalog::builtin().unwrap();
    let pod = catalog.rule("iaas-pod-identity").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/key-pair-not-found.json");
    let (_, world) = FixtureBackend::split(fixture::load(&path).unwrap());
    let jq = Jq::new();
    for code in ["AwsKeyPairNotFound", "FeatureNotSupported"] {
        let body = format!(r#"{{"code":"{code}","message":"refused"}}"#);
        let api = strata::api::MemoryApi::new().failing(ROUTE, 500, body);
        let slept = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let record = slept.clone();
        let retry = strata::api::Retry::with_sleep(
            &api,
            strata::api::RetryPolicy::new(3),
            strata::term::progress::Progress::silent(),
            move |wait| record.borrow_mut().push(wait),
        );
        let out = probe::run_check(pod, &world, &LiveBackend::new(Some(&retry), false), &jq);
        assert_eq!(out.verdict, Verdict::Failed, "{code}: {:?}", out.messages);
        assert_eq!(api.calls(ROUTE), 1, "{code}: one request and no retry");
        assert!(slept.borrow().is_empty(), "{code}: {:?}", slept.borrow());
    }
}

/// `ssh-key-pair` applies where `eks-public-subnet-mapping` does not. The property route of EKS
/// checks the key pair only when the body has a `vpcId`. `eks-public-subnet-mapping` sends the
/// route only with a `vpcId` and a subnet id (#430). On each body, one of the two rules applies.
#[test]
fn ssh_key_pair_applies_where_eks_public_subnet_mapping_does_not() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let path = catalog::rules_dir().join("fixtures/eks/ssh-key-listed.json");
    let clean = fixture::load(&path).unwrap();
    let applies = |id: &str, world: &serde_json::Value| {
        let when = catalog.rule(id).unwrap().when.as_deref().unwrap();
        jq.first(id, when, world, None).unwrap() == serde_json::Value::Bool(true)
    };
    let subnets = serde_json::json!([{"id": "subnet-1", "az": "us-east-2a"}]);
    for (vpc, with_subnets, key_pair, mapping) in [
        ("", false, true, false),
        ("", true, true, false),
        ("vpc-1", false, true, false),
        ("vpc-1", true, false, true),
    ] {
        let mut world = clean.clone();
        world["entity"]["spec"]["cloudConfig"]["vpcId"] = vpc.into();
        world["entity"]["spec"]["machinepoolconfig"][0]["cloudConfig"]["subnets"] = if with_subnets
        {
            subnets.clone()
        } else {
            serde_json::json!([])
        };
        let case = format!("vpcId {vpc:?}, subnets {with_subnets}");
        assert_eq!(applies("ssh-key-pair", &world), key_pair, "{case}");
        assert_eq!(
            applies("eks-public-subnet-mapping", &world),
            mapping,
            "{case}"
        );
    }
}

/// `pack-cloud-support` reads the cloud type of Palette, so an edge cluster takes a pack that lists
/// `edge-native` (#430). A pack with an empty list supports each cloud. A pack that lists another
/// cloud fails, and the message names the cloud type of the route.
#[test]
fn pack_cloud_support_reads_the_cloud_type_of_an_edge_pack_and_an_empty_list() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("pack-cloud-support").unwrap();
    let jq = Jq::new();
    for fixture in [
        "fixtures/edge/clean.json",
        "fixtures/gcp/pack-no-cloud-types.json",
    ] {
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &jq);
        assert_eq!(
            out.verdict,
            Verdict::Passed,
            "{fixture}: {:?}",
            out.messages
        );
    }
    for (fixture, message) in [
        (
            "fixtures/edge/pack-no-edge.json",
            "pack edge-native-byoi@1.0.0 of profile edge-stack supports aws, not edge-native",
        ),
        (
            "fixtures/gcp/pack-no-gcp.json",
            "pack cni-cilium-oss@1.19.6 of profile gcp-stack supports aws, azure, vsphere, not gcp",
        ),
    ] {
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &jq);
        assert_eq!(out.verdict, Verdict::Failed, "{fixture}");
        assert_eq!(out.messages, [message], "{fixture}");
    }
}

/// The `kms` rule fails a key that is in the list of the account and is not `Enabled` (#195). The
/// route of Palette reads the list alone, so a disabled key passes `palette-validate`.
#[test]
fn kms_fails_a_disabled_key_that_the_list_of_keys_holds() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("kms").unwrap();
    let key = "1234abcd-12ab-34cd-56ef-1234567890ab";
    for (fixture, expected) in [
        ("fixtures/eks/kms-key-disabled.json", Some("Disabled")),
        ("fixtures/eks/kms-clean.json", None),
    ] {
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        match expected {
            Some(state) => {
                assert_eq!(out.verdict, Verdict::Failed, "{fixture}");
                assert_eq!(
                    out.messages,
                    [format!("KMS key {key} is {state}, not Enabled")],
                    "{fixture}"
                );
            }
            None => assert_eq!(
                out.verdict,
                Verdict::Passed,
                "{fixture}: {:?}",
                out.messages
            ),
        }
    }
}

/// The world gives `accountKind` and `routeCloud`, so no rule maps the engine to a cloud itself
/// (#430). A path or a program that writes `edge-native`, or `then "aws"`, is a second copy of
/// `account_kind` or `route_cloud`.
#[test]
fn no_check_maps_the_engine_to_a_cloud_itself() {
    let catalog = catalog::builtin().unwrap();
    let mut found = Vec::new();
    for rule in catalog.rules() {
        let texts: Vec<&str> = match &rule.check {
            Some(Check::Static { expr }) | Some(Check::Watch { expr }) => vec![expr],
            Some(Check::Palette(PaletteCheck {
                path,
                body,
                each,
                expr,
                ..
            })) => [Some(path), body.as_ref(), each.as_ref(), Some(expr)]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect(),
            Some(Check::Exec(ExecCheck {
                argv,
                palette,
                expr,
                ..
            })) => argv
                .iter()
                .map(String::as_str)
                .chain(palette.iter().map(|read| read.path.as_str()))
                .chain([expr.as_str()])
                .collect(),
            None => Vec::new(),
        };
        let when = rule.when.as_deref().into_iter();
        for text in texts.into_iter().chain(when) {
            for copy in [
                "edge-native",
                "then \"aws\"",
                "then \"gcp\"",
                "then \"azure\"",
            ] {
                if text.contains(copy) {
                    found.push(format!("{}: {copy}", rule.id));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "read `.accountKind` or `.routeCloud` of the world:\n{}",
        found.join("\n")
    );
}

/// The messages of the rule `iam-role-policy-quota` on the fixture `name` of the EKS fixtures.
fn policy_quota_messages(name: &str) -> (Verdict, Vec<String>) {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("iam-role-policy-quota").unwrap();
    let path = catalog::rules_dir().join("fixtures/eks").join(name);
    verdict(rule, &path, &Jq::new())
}

/// AWS gives 20 managed policies for a role by default, and raises the quota of an account on
/// request. The rule reads the quota of the account, so a role with 11 to 20 policies passes. A
/// role over the quota of the account fails with the number of the account (#540).
#[test]
fn the_rule_of_the_managed_policies_of_a_role_reads_the_quota_of_the_account() {
    let (v, m) = policy_quota_messages("nodes-policies-within-quota.json");
    assert_eq!((v, m), (Verdict::Passed, Vec::<String>::new()));
    let (v, m) = policy_quota_messages("nodes-policies-over-quota.json");
    assert_eq!(v, Verdict::Failed);
    assert_eq!(
        m,
        [
            "the nodes role has 20 managed policies, and the cloud account adds 1 more: 21 is over the IAM quota of 20 for a role"
        ]
    );
    // 26 policies are over the default 20, and the quota of the account is 30: they pass
    let (v, m) = policy_quota_messages("nodes-policies-within-raised-quota.json");
    assert_eq!((v, m), (Verdict::Passed, Vec::<String>::new()));
    let (v, m) = policy_quota_messages("nodes-policies-over-raised-quota.json");
    assert_eq!(v, Verdict::Failed);
    assert_eq!(
        m,
        [
            "the nodes role has 30 managed policies, and the cloud account adds 1 more: 31 is over the IAM quota of 30 for a role"
        ]
    );
}

/// When the call that reads the quota fails, the rule uses the default 20, and the message says
/// that it did (#540).
#[test]
fn the_rule_of_the_managed_policies_of_a_role_uses_the_default_when_it_cannot_read_the_quota() {
    let (v, m) = policy_quota_messages("nodes-policies-quota-unread-within.json");
    assert_eq!((v, m), (Verdict::Passed, Vec::<String>::new()));
    let (v, m) = policy_quota_messages("nodes-policies-quota-unread.json");
    assert_eq!(v, Verdict::Failed);
    assert_eq!(
        m,
        [
            "the nodes role has 20 managed policies, and the cloud account adds 1 more: 21 is over the IAM quota of 20 for a role. The check could not read the quota of the account (An error occurred (AccessDeniedException) when calling the GetServiceQuota operation: User is not authorized to perform: servicequotas:GetServiceQuota), so it used the default quota"
        ]
    );
}

/// The second command exits 254 in the fixture. The answer of the check holds the exit code of
/// each command, so a failed read of the quota does not show as `exit 0`.
#[test]
fn the_answer_of_the_rule_of_the_managed_policies_of_a_role_shows_the_exit_code_of_the_quota_read()
{
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("iam-role-policy-quota").unwrap();
    let path = catalog::rules_dir().join("fixtures/eks/nodes-policies-quota-unread.json");
    let outcome = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
    assert_eq!(outcome.ran.answer().as_deref(), Some("exit 0; exit 254"));
    let path = catalog::rules_dir().join("fixtures/eks/nodes-policies-over-quota.json");
    let outcome = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
    assert_eq!(outcome.ran.answer().as_deref(), Some("exit 0; exit 0"));
}

/// A run of the ledger that names an id the catalog does not hold counts for no rule, and the
/// loader says nothing. When a rule changes its id, rename its runs. When a rule leaves the
/// catalog, drop its runs. The ledger holds the ids of the custom probes of the lab too, so the
/// catalog of the test holds them.
#[test]
fn each_run_of_the_ledger_of_the_binary_names_a_rule_of_the_catalog_or_of_the_lab_probes() {
    let lab =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/live/preflight/lab-probes.yaml");
    let catalog = catalog::with_probes(&[lab]).unwrap();
    let ledger = catalog::builtin_ledger().unwrap();
    let unknown: BTreeSet<&str> = ledger
        .runs
        .iter()
        .flat_map(|run| &run.results)
        .map(|result| result.rule.as_str())
        .filter(|id| catalog.rule(id).is_none())
        .collect();
    assert!(
        unknown.is_empty(),
        "the ledger names rules that no catalog holds, so their runs count for no rule: {unknown:?}"
    );
}

/// The rules that read the instance type route. Each writes the lookup itself: the two messages
/// of a refusal, and the read of `cpu` for a pool. A rule file has no shared jq definition, so the
/// lookup is in four places. The fifth rule that reads the route moves the lookup to one section of
/// jq definitions in the catalog. Then this list is not needed (#575). The check of the facts in
/// `xtask` keeps the query of each rule equal to the query of its fact.
const INSTANCE_TYPE_READERS: [&str; 4] = [
    "aws-quotas",
    "azure-vcpu-quota",
    "gcp-quotas",
    "instance-offering",
];

#[test]
fn four_rules_read_the_instance_type_route_and_a_fifth_moves_the_lookup_to_shared_definitions() {
    let catalog = catalog::builtin().unwrap();
    let reads = |rule: &Rule| match &rule.check {
        Some(Check::Palette(check)) => check.path.contains("/instancetypes?"),
        Some(Check::Exec(ExecCheck {
            palette: Some(read),
            ..
        })) => read.path.contains("/instancetypes?"),
        _ => false,
    };
    let readers: BTreeSet<&str> = catalog
        .rules()
        .filter(|rule| reads(rule))
        .map(|rule| rule.id.as_str())
        .collect();
    assert_eq!(
        readers,
        BTreeSet::from(INSTANCE_TYPE_READERS),
        "A rule reads the instance type route that INSTANCE_TYPE_READERS lacks, or one of the four \
         does not. With a fifth copy, move the two refusal messages and the read of `cpu` to one \
         section of jq definitions in the catalog"
    );
}

/// The 13 clean results of the rule `iam-ten-policies` hold for the rule `iam-role-policy-quota`. A
/// clean build that passes the quota of 10 passes the quota of 20. 9 are on `aws`, and 4 are on
/// `eks`. A later run adds to these counts, so the test names a floor (#575).
#[test]
fn the_clean_runs_of_the_old_id_of_the_managed_policy_rule_count_for_the_new_id() {
    let ledger = catalog::builtin_ledger().unwrap();
    let id = "iam-role-policy-quota";
    let passed = |engine: &str| ledger.counts_for(id, engine, None).passed;
    assert!(passed("aws") >= 9, "the runs on aws");
    assert!(passed("eks") >= 4, "the runs on eks");
}

/// The rule keeps no figure of the quota: the old id is not in the catalog, and the new id has no
/// number (#540).
#[test]
fn the_rule_of_the_managed_policies_of_a_role_has_an_id_with_no_number() {
    let catalog = catalog::builtin().unwrap();
    assert!(catalog.rule("iam-ten-policies").is_none());
    let rule = catalog.rule("iam-role-policy-quota").unwrap();
    assert!(!rule.id.chars().any(|c| c.is_ascii_digit()), "{}", rule.id);
    assert!(
        !rule.title.chars().any(|c| c.is_ascii_digit()),
        "{}",
        rule.title
    );
}

/// `gcloud` omits the key `secondaryIpRanges` for a subnet that has no secondary range. The
/// fixture shows the answer as the tool prints it, and the rule still fails on it (#550).
#[test]
fn a_subnet_with_no_secondary_range_has_no_key_in_the_fixture_and_the_rule_fails() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("gke-secondary-ranges").unwrap();
    let path = catalog::rules_dir().join("fixtures/gke/no-secondary-ranges.json");
    let doc = fixture::load(&path).unwrap();
    let subnets: Vec<&serde_json::Value> = doc["_exec"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(command, _)| command.starts_with("gcloud compute networks subnets list"))
        .flat_map(|(_, answer)| answer["stdout"].as_array().unwrap())
        .collect();
    assert_eq!(subnets.len(), 1);
    assert!(
        subnets[0].get("secondaryIpRanges").is_none(),
        "the fixture holds the key: {}",
        subnets[0]
    );
    let out = outcome_of(rule, doc, &Jq::new());
    assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
    assert_eq!(out.messages.len(), 1, "{:?}", out.messages);
    assert!(
        out.messages[0]
            .starts_with("subnet shared-net-us-east1 (10.10.0.0/20) has no secondary range"),
        "{:?}",
        out.messages
    );
}

/// The path of a fixture of the catalog: `folder` is the engine, such as `eks`.
fn fixture_path(folder: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("rules/fixtures")
        .join(folder)
        .join(file)
}

/// A rule that reads a version reads it as `strata` reads it, so a pre-release, a `v`, and build
/// data are versions. The program of `capa-stack-release-behind` stopped on `4.11.0-rc.1`, a
/// release that `--release` takes.
#[test]
fn the_rules_that_read_a_version_read_a_pre_release_a_v_and_build_data_as_strata_does() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let rule = |id: &str| catalog.rules().find(|r| r.id == id).unwrap().clone();
    let run = |id: &str, doc: serde_json::Value| {
        let outcome = outcome_of(&rule(id), doc, &jq);
        (outcome.verdict, outcome.messages)
    };

    // the tag of the role is 4.10.4 in this fixture
    let capa = |release: &str| {
        let mut doc = fixture::load(&fixture_path("eks", "capa-release-behind.json")).unwrap();
        doc["release"] = serde_json::json!(release);
        run("capa-stack-release-behind", doc)
    };
    let (verdict, messages) = capa("4.11.0-rc.1");
    assert_eq!(verdict, Verdict::Failed, "{messages:?}");
    assert!(
        messages[0].contains("the tenant runs 4.11.0-rc.1"),
        "{messages:?}"
    );
    for same in ["4.10", "v4.10", "4.10.4-rc.1", "4.10.4+build.5"] {
        assert_eq!(capa(same).0, Verdict::Passed, "{same}");
    }
    let (verdict, messages) = capa("latest");
    assert_eq!(verdict, Verdict::Error, "{messages:?}");
    assert!(
        messages[0].contains("--release latest is no version"),
        "{messages:?}"
    );

    // the pack is 1.26.15 in this fixture, under the floor 1.27 of AWS
    let ccm = |version: &str| {
        let mut doc = fixture::load(&fixture_path("aws", "k8s-1-26.json")).unwrap();
        doc["profiles"][0]["packs"][1]["version"] = serde_json::json!(version);
        run("ccm-version", doc).0
    };
    assert_eq!(ccm("1.26.15-rc.1"), Verdict::Failed);
    assert_eq!(ccm("v1.26.15"), Verdict::Failed);
    for at_the_floor in ["1.27.0-rc.1", "v1.27.3+rke2r1", "1.28"] {
        assert_eq!(ccm(at_the_floor), Verdict::Passed, "{at_the_floor}");
    }

    // the release is 4.10 in this fixture, and the agent of the host cp-1 is 4.7.16
    let agent = |version: &str| {
        let mut doc = fixture::load(&fixture_path("edge", "agent-old.json")).unwrap();
        doc["_responses"]["GET /v1/edgehosts/edge-host-cp-1"]["body"]["status"]["health"]["agentVersion"] =
            serde_json::json!(version);
        run("edge-agent-version", doc).0
    };
    assert_eq!(agent("v4.7.0-rc.1"), Verdict::Failed);
    for current in ["v4.10.0-rc.1", "4.10.16+build.2", "4.11"] {
        assert_eq!(agent(current), Verdict::Passed, "{current}");
    }
}

/// A text that is no version reads as it did in release 0.27.0: the rule keeps the first two
/// parts between the dots. The verdicts below are those of release 0.27.0. Only a `v` and build
/// data read another way.
#[test]
fn a_version_that_does_not_parse_reads_in_ccm_version_and_edge_agent_version_as_in_release_0_27_0()
{
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let verdict = |id: &str, doc: serde_json::Value| {
        let rule = catalog.rules().find(|r| r.id == id).unwrap();
        outcome_of(rule, doc, &jq).verdict
    };

    // the floor of AWS is 1.27
    let ccm = |version: &str| {
        let mut doc = fixture::load(&fixture_path("aws", "k8s-1-26.json")).unwrap();
        doc["profiles"][0]["packs"][1]["version"] = serde_json::json!(version);
        verdict("ccm-version", doc)
    };
    for (version, expected) in [
        ("1.26.x", Verdict::Failed),
        ("1.26.15.1", Verdict::Failed),
        ("1.x", Verdict::Failed),
        ("LTS__1.26", Verdict::Failed),
        ("v1.26.15", Verdict::Failed),
        ("1.27.x", Verdict::Passed),
        ("1.28.x", Verdict::Passed),
    ] {
        assert_eq!(ccm(version), expected, "ccm-version, {version}");
    }

    // the release is 4.10 in this fixture, and the agent of the host cp-1 is 4.7.16
    let agent = |version: &str| {
        let mut doc = fixture::load(&fixture_path("edge", "agent-old.json")).unwrap();
        doc["_responses"]["GET /v1/edgehosts/edge-host-cp-1"]["body"]["status"]["health"]["agentVersion"] =
            serde_json::json!(version);
        verdict("edge-agent-version", doc)
    };
    for (version, expected) in [
        ("4.10.x", Verdict::Passed),
        ("4.10.4.1", Verdict::Passed),
        ("4.9.x", Verdict::Failed),
        ("4.7.4.1", Verdict::Failed),
        // a `v` is part of the version now, and the numbers read as the version writes them
        ("v4.10.4", Verdict::Passed),
    ] {
        assert_eq!(agent(version), expected, "edge-agent-version, {version}");
    }
}

/// Two of the seven rules read a text that is no version in another way than release 0.27.0, and
/// `compatibility.md` says so (#608). `maas-os-image` reads the first two parts as numbers, so
/// `22.4.x` and `22.4.1.1` pass. `capa-stack-release-behind` stops with an error for a tag of four
/// numbers, which it compared before. A tag such as `1.26.x` was an error before too.
#[test]
fn maas_os_image_reads_the_first_two_parts_as_numbers_and_capa_stops_on_a_tag_of_four_numbers() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let maas = catalog.rule("maas-os-image").unwrap();
    for version in ["22.4.x", "22.4.1.1"] {
        let mut doc = fixture::load(&fixture_path("maas", "clean.json")).unwrap();
        doc["profiles"][0]["packs"][0]["version"] = serde_json::json!(version);
        let outcome = outcome_of(maas, doc, &jq);
        assert_eq!(
            outcome.verdict,
            Verdict::Passed,
            "{version}: {:?}",
            outcome.messages
        );
    }

    let capa = catalog.rule("capa-stack-release-behind").unwrap();
    for tag in ["1.26.15.1", "1.26.x"] {
        let mut doc = fixture::load(&fixture_path("eks", "capa-release-behind.json")).unwrap();
        doc["release"] = serde_json::json!("1.26");
        let command = doc["_exec"]
            .as_object_mut()
            .unwrap()
            .iter_mut()
            .find(|(line, _)| line.starts_with("aws iam list-role-tags --role-name controllers"))
            .unwrap()
            .1;
        command["stdout"]["Tags"][0]["Value"] = serde_json::json!(tag);
        let outcome = outcome_of(capa, doc, &jq);
        assert_eq!(
            outcome.verdict,
            Verdict::Error,
            "{tag}: {:?}",
            outcome.messages
        );
        let message = outcome.messages.join("\n");
        assert!(
            message.contains(&format!("holds {tag}, and that is no version")),
            "{message}"
        );
    }
}

/// The folder and the files of a run of the `jq` tool over a line of `reproduce`. `world.json`,
/// `entity.json`, and `body.json` hold an empty object, so that every line finds its files.
fn folder_for_reproduce() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in ["world.json", "entity.json", "body.json"] {
        std::fs::write(dir.path().join(file), "{}").unwrap();
    }
    dir
}

/// Run a line of `reproduce` that starts with `jq` in the folder. A person fills the three
/// placeholders of a line, and the test fills them with `null` and `0`.
fn run_reproduce_line(line: &str, dir: &Path) -> std::process::Output {
    let line = line
        .replace("\"$(<the output>)\"", "null")
        .replace("\"$(<the output of the second command>)\"", "null")
        .replace("<the exit code>", "0");
    Command::new("bash")
        .args(["-c", &line])
        .current_dir(dir)
        .env("code", "200")
        .output_locked()
        .unwrap()
}

/// The lines of `reproduce` of one rule on a fixture, as the report prints them for the
/// directory `out`.
fn reproduce_lines(
    catalog: &Catalog,
    rule: &Rule,
    doc: serde_json::Value,
    out: &Path,
    jq: &Jq,
) -> Vec<String> {
    let outcome = outcome_of(rule, doc.clone(), jq);
    let engine = rule.engines.first().map_or("aws", String::as_str);
    let report = report::assemble(
        report::Assembly {
            catalog,
            outcomes: vec![(rule, outcome)],
            confidence: Default::default(),
            world: &FixtureBackend::split(doc).1,
            subject: report::Subject {
                cloud: if engine == "all" { "aws" } else { engine }.to_string(),
                release: None,
                source: "fixture".into(),
                name: String::new(),
                cluster_uid: None,
            },
            options: report::Options {
                tiers: vec![],
                strict: false,
                ready: None,
                out_dir: Some(out.display().to_string()),
            },
            cluster_gone: false,
            generated_at: "2026-01-01T00:00:00Z".into(),
        },
        jq,
    );
    report
        .checks
        .into_iter()
        .flat_map(|c| c.reproduce)
        .collect()
}

/// A person runs a line of `reproduce` in the `jq` tool, so each program of such a line must
/// compile there. The tool exits with 3 for a program that does not compile. `ccm-version` and
/// `capa-stack-release-behind` named `semver_numbers`, and `macro-silent` named `matches`: the
/// tool has neither.
#[test]
fn each_jq_line_of_reproduce_in_the_catalog_compiles_in_the_jq_tool() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let dir = folder_for_reproduce();
    let mut checked = BTreeSet::new();
    let mut failures = Vec::new();
    for rule in catalog.rules() {
        let Some(fault) = rule.tests.iter().find_map(|t| t.fault.as_ref()) else {
            continue;
        };
        let doc = fixture::load(fault).unwrap();
        let out = dir.path();
        for line in reproduce_lines(&catalog, rule, doc, out, &jq) {
            if !line.starts_with("jq ") {
                continue;
            }
            let ran = run_reproduce_line(&line, dir.path());
            checked.insert(rule.id.clone());
            if ran.status.code() == Some(3) {
                failures.push(format!(
                    "{}: {line}\n{}",
                    rule.id,
                    String::from_utf8_lossy(&ran.stderr)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for id in ["ccm-version", "capa-stack-release-behind", "macro-silent"] {
        assert!(checked.contains(id), "no jq line of {id} was compiled");
    }
}

/// Run the program `expr` in the engine and in the `jq` tool, over each input. The tool runs the
/// line that a report prints for a static rule: the definitions of the filters in front, then the
/// program. The line starts with `start`. The two give the same answer, or both fail.
fn assert_the_tool_gives_the_answer_of_the_engine(
    expr: &str,
    start: &str,
    inputs: impl IntoIterator<Item = serde_json::Value>,
) {
    let section = format!(
        "section: kubernetes\ntitle: K\nrules:\n  - id: one-program\n    title: t\n    engines: [aws]\n    tier: static\n    status: seen\n    what: w\n    check:\n      kind: static\n      expr: |\n        {expr}\n"
    );
    let mut catalog = Catalog::default();
    let file = Path::new("k.yaml");
    catalog
        .push(Catalog::parse_section(&section, file).unwrap(), file)
        .unwrap();
    let rule = catalog.rule("one-program").unwrap();
    let jq = Jq::new();
    let dir = folder_for_reproduce();
    let lines = reproduce_lines(&catalog, rule, serde_json::json!({}), dir.path(), &jq);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(start), "{}", lines[0]);

    for input in inputs {
        std::fs::write(dir.path().join("world.json"), input.to_string()).unwrap();
        let tool = run_reproduce_line(&lines[0], dir.path());
        let engine = jq.first("t", expr, &input, None);
        if tool.status.success() {
            let answer: serde_json::Value = serde_json::from_slice(&tool.stdout).unwrap();
            assert_eq!(answer, engine.unwrap(), "{input}");
        } else {
            assert!(
                engine.is_err(),
                "{input}: the tool failed, the engine gave {engine:?}"
            );
        }
    }
}

/// The definition in front of a program that names `semver_numbers` gives the answer of the
/// filter of the engine. The line is the one that a report prints for a static rule.
#[test]
fn the_definition_of_semver_numbers_in_a_reproduce_line_gives_the_answer_of_the_engine() {
    let texts = [
        "4.10.3-rc.1",
        "v4.10",
        "1.0.1+2",
        "LTS__24.x",
        "",
        "1.26.x",
        "1.26.15.1",
        "1.x",
        "LTS__1.26",
        "v1.26.15",
        "4.10.x",
        "vv1.0",
        "v",
        "1",
        "01.02",
        "1.2.3+b-1",
        "1.0.0-rc-1",
        "1.0.0-rc.1+b.2",
        "1.0-a+b",
        "1.0-",
        "1.0+",
        "1.0.0+b+c",
        "1.0.0-rc..1",
        "1..2",
        ".1",
        "1.2.",
        " 1.0",
        "1.0\n",
        "1.0.0-\u{3b1}",
        "\u{663}.\u{664}",
        "+1",
        "-1",
        "1,0",
    ];
    let inputs = texts
        .iter()
        .map(|t| serde_json::json!(t))
        .chain([serde_json::json!(4), serde_json::Value::Null]);
    assert_the_tool_gives_the_answer_of_the_engine(
        "semver_numbers",
        "jq 'def semver_numbers: ",
        inputs,
    );
}

/// The definition in front of a program that names `count_of` gives the words of the filter of the
/// engine. The table has numbers that are not whole numbers of zero or more too. The engine writes
/// the number and the plural noun, as the definition does.
#[test]
fn the_definition_of_count_of_in_a_reproduce_line_gives_the_answer_of_the_engine() {
    let whole =
        [0, 1, 2, 3, 10, 11, 100, 12345, 1_000_000, -1, -2, -100].map(|n| serde_json::json!(n));
    let fractions = [0.5, -0.5, 1.5, 2.5, 0.0, 1.0].map(|n| serde_json::json!(n));
    let counts = whole.into_iter().chain(fractions);
    assert_the_tool_gives_the_answer_of_the_engine(
        r#"count_of("key pair")"#,
        "jq 'def count_of($noun): ",
        counts,
    );
}

/// The ledger holds the run of the plan of the live window of Wave 14, the newest run. It is an
/// AWS run with no release. The plan made six faults and the checks caught each one. The clean
/// entity passed 22 checks.
#[test]
fn the_ledger_ends_with_the_run_of_the_plan_of_the_live_window_of_wave_14() {
    use strata_preflight::confidence::RunResult;
    let ledger = catalog::builtin_ledger().unwrap();
    let run = ledger.runs.last().unwrap();
    assert_eq!(run.run, "20261103T125248Z");
    assert_eq!(run.engine, "aws");
    assert_eq!(run.release, "");
    let count = |result: RunResult| run.results.iter().filter(|r| r.result == result).count();
    assert_eq!(count(RunResult::Caught), 6);
    assert_eq!(count(RunResult::Passed), 22);
    assert_eq!(run.results.len(), 28, "no check was missed or false");
    let caught: Vec<&str> = run
        .results
        .iter()
        .filter(|r| r.result == RunResult::Caught)
        .map(|r| r.rule.as_str())
        .collect();
    assert_eq!(
        caught,
        [
            "worker-cpu-floor",
            "palette-validate",
            "api-endpoint-reach",
            "account-missing",
            "instance-offering",
            "iaas-pod-identity"
        ]
    );
    let times: BTreeSet<&str> = ledger.runs.iter().map(|r| r.run.as_str()).collect();
    assert_eq!(times.len(), ledger.runs.len(), "each run has its own id");
}

/// A user reads the message of a failed quota rule in the report. Each sentence of it has 25 words
/// at most. The review at the end of Wave 13 found five messages of 28 to 37 words in one sentence.
#[test]
fn each_sentence_in_a_message_of_the_quota_rules_has_25_words_at_most() {
    let catalog = catalog::builtin().unwrap();
    for (id, fixture) in [
        ("aws-quotas", "fixtures/aws/vcpu-quota-low.json"),
        ("aws-quotas", "fixtures/aws/vcpu-quota-type-not-listed.json"),
        ("gcp-quotas", "fixtures/gke/cpu-quota-low.json"),
        ("gcp-quotas", "fixtures/gke/cpu-quota-type-not-listed.json"),
        ("azure-vcpu-quota", "fixtures/azure/vcpu-quota-low.json"),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-small-size.json",
        ),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-size-not-listed.json",
        ),
    ] {
        let rule = catalog.rule(id).unwrap();
        let path = catalog::rules_dir().join(fixture);
        let out = outcome_of(rule, fixture::load(&path).unwrap(), &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{id}: {:?}", out.messages);
        for message in &out.messages {
            for sentence in message.split(". ") {
                let words = sentence.split_whitespace().count();
                assert!(words <= 25, "{id}: {words} words in: {sentence}");
            }
        }
    }
}

/// The verdict and the messages of the rule `id` on `fixture`, after `version` replaces the text
/// at `pointer` in the document of the fixture.
fn verdict_with_version(
    (catalog, jq): (&Catalog, &Jq),
    id: &str,
    fixture: &str,
    (pointer, version): (&str, &str),
) -> (Verdict, Vec<String>) {
    let rule = catalog.rule(id).unwrap();
    let mut doc = fixture::load(&catalog::rules_dir().join(fixture)).unwrap();
    *doc.pointer_mut(pointer)
        .unwrap_or_else(|| panic!("{id}: {fixture} has no {pointer}")) = serde_json::json!(version);
    let outcome = outcome_of(rule, doc, jq);
    (outcome.verdict, outcome.messages)
}

/// The rules that read the version of a pack: the rule, its clean fixture, and the place of the
/// version in that fixture.
const VERSION_RULES: [(&str, &str, &str); 4] = [
    (
        "eks-version-not-offered",
        "fixtures/eks/eks-version-not-offered-clean.json",
        "/profiles/0/packs/1/version",
    ),
    (
        "gke-version-not-offered",
        "fixtures/gke/gke-version-not-offered-clean.json",
        "/profiles/0/packs/1/version",
    ),
    (
        "eks-addon-version",
        "fixtures/eks/eks-addon-version-clean.json",
        "/profiles/0/packs/1/version",
    ),
    (
        "maas-os-image",
        "fixtures/maas/clean.json",
        "/profiles/0/packs/0/version",
    ),
];

/// One version text is one version for each rule that reads it. `semver_numbers` reads `1.32.1`,
/// `v1.32.1`, and `1.32+build.5` as the numbers 1 and 32. The four rules once cut the text at the
/// dots, so `v1.32.1` and `1.32+build.5` gave a release line that no cloud offers (#594).
#[test]
fn the_rules_that_read_the_release_line_of_a_pack_read_a_v_a_pre_release_and_build_data() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let passes = |id: &str, versions: &[&str]| {
        let (_, fixture, pointer) = VERSION_RULES.iter().find(|r| r.0 == id).unwrap();
        for version in versions {
            let (verdict, messages) =
                verdict_with_version((&catalog, &jq), id, fixture, (pointer, version));
            assert_eq!(verdict, Verdict::Passed, "{id}, {version}: {messages:?}");
        }
    };
    // EKS offers 1.32 to 1.36
    passes(
        "eks-version-not-offered",
        &["1.32.1", "v1.32.1", "1.32.1-rc.1", "1.32+build.5", "v1.36"],
    );
    // the clean entity asks EKS for the addon versions of 1.32: the command line holds `1.32`
    passes(
        "eks-addon-version",
        &["1.32.1", "v1.32.1", "1.32.1-rc.1", "1.32+build.5"],
    );
    // GKE offers 1.34.11 in the channel REGULAR. A version with three numbers is an exact version
    passes(
        "gke-version-not-offered",
        &[
            "1.34",
            "v1.34",
            "1.34+build.5",
            "1.34.11",
            "v1.34.11",
            "1.34.11-rc.1",
            "1.34.11+build.5",
        ],
    );
    // Palette maps 18.04, 20.04, 22.04, and 24.04. The numbers 22 and 4 are 22.04
    passes(
        "maas-os-image",
        &[
            "22.04",
            "v22.04",
            "22.04.3",
            "22.04-rc.1",
            "22.04+build.5",
            "22.4",
            "18.04",
            "20.04",
            "24.04.1",
        ],
    );
    // a version outside the offer still fails, and the message names the line that the check read
    let fails = |id: &str, version: &str| {
        let (_, fixture, pointer) = VERSION_RULES.iter().find(|r| r.0 == id).unwrap();
        let (verdict, messages) =
            verdict_with_version((&catalog, &jq), id, fixture, (pointer, version));
        assert_eq!(verdict, Verdict::Failed, "{id}, {version}: {messages:?}");
        messages.join("\n")
    };
    assert!(fails("eks-version-not-offered", "v1.31.2").contains("offers no Kubernetes 1.31:"));
    assert!(
        fails("gke-version-not-offered", "v1.34.99").contains("offers no Kubernetes 1.34.99 in")
    );
    assert!(fails("maas-os-image", "v16.04.1").contains("refuses v16.04.1 with"));
}

/// A text that is no version reads in the four rules as it did in release 0.27.0. The rule keeps
/// the first two parts between the dots. The verdicts below are those of release 0.27.0. A smart
/// tag such as `1.32.x` is such a text. Only a text that parses reads another way.
#[test]
fn a_version_that_does_not_parse_reads_in_the_four_rules_as_in_release_0_27_0() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    // the rule, the version, the verdict, and a part of the message
    let table = [
        ("eks-version-not-offered", "1.32.x", Verdict::Passed, ""),
        ("eks-version-not-offered", "1.34.x", Verdict::Passed, ""),
        ("eks-version-not-offered", "1.32.1.1", Verdict::Passed, ""),
        (
            "eks-version-not-offered",
            "1.31.x",
            Verdict::Failed,
            "offers no Kubernetes 1.31:",
        ),
        (
            "eks-version-not-offered",
            "1.x",
            Verdict::Failed,
            "offers no Kubernetes 1.x:",
        ),
        (
            "eks-version-not-offered",
            "LTS__1.32",
            Verdict::Failed,
            "offers no Kubernetes LTS__1.32:",
        ),
        // GKE takes a text with three parts between the dots as an exact version
        ("gke-version-not-offered", "1", Verdict::Passed, ""),
        (
            "gke-version-not-offered",
            "1.34.x",
            Verdict::Failed,
            "offers no Kubernetes 1.34.x in",
        ),
        (
            "gke-version-not-offered",
            "1.34.11.1",
            Verdict::Failed,
            "offers no Kubernetes 1.34.11.1 in",
        ),
        (
            "gke-version-not-offered",
            "1.x",
            Verdict::Failed,
            "offers no Kubernetes 1.x in",
        ),
        // the command line of the rule holds the first two parts of the text
        ("eks-addon-version", "1.32.x", Verdict::Passed, ""),
        ("eks-addon-version", "1.32.1.1", Verdict::Passed, ""),
        (
            "eks-addon-version",
            "1.34.x",
            Verdict::Error,
            "--kubernetes-version 1.34 --addon-name",
        ),
        ("maas-os-image", "22.04.x", Verdict::Passed, ""),
        ("maas-os-image", "20.04.x", Verdict::Passed, ""),
        ("maas-os-image", "22.04.1.5", Verdict::Passed, ""),
        (
            "maas-os-image",
            "22.x",
            Verdict::Failed,
            "refuses 22.x with",
        ),
        (
            "maas-os-image",
            "LTS__22.04",
            Verdict::Failed,
            "refuses LTS__22.04 with",
        ),
    ];
    for (id, version, expected, part) in table {
        let (_, fixture, pointer) = VERSION_RULES.iter().find(|r| r.0 == id).unwrap();
        let (verdict, messages) =
            verdict_with_version((&catalog, &jq), id, fixture, (pointer, version));
        assert_eq!(verdict, expected, "{id}, {version}: {messages:?}");
        assert!(
            messages.iter().all(|m| m.contains(part)),
            "{id}, {version}: {messages:?}"
        );
    }
}

/// The text of each rule in a file of the catalog, with its id. A rule starts at a line
/// `  - id: <id>` and ends where the next rule starts.
fn rule_texts(text: &str) -> Vec<(&str, &str)> {
    let starts: Vec<usize> = text
        .match_indices("\n  - id: ")
        .map(|(at, _)| at + 1)
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(i, &from)| {
            let block = &text[from..starts.get(i + 1).map_or(text.len(), |&next| next)];
            let id = block["  - id: ".len()..].lines().next().unwrap_or("");
            (id.trim(), block)
        })
        .collect()
}

/// True when `rule` reads a version field and cuts a text at the dots with no `semver_numbers`
/// before the cut. `semver_numbers // split(".")` is the one correct form. A text that is no
/// version, such as the smart tag `1.32.x`, keeps the reading of release 0.27.0. The IP programs
/// of `networking.yaml` cut at the dots too, and they read no version field.
fn splits_a_version_at_the_dots(rule: &str) -> bool {
    if !(names_a_word(rule, ".version") || names_a_word(rule, ".tag")) {
        return false;
    }
    [r#"split(".")"#, r#"split(\".\")"#].iter().any(|call| {
        rule.match_indices(call).any(|(at, _)| {
            let before = rule[..at].trim_end();
            !(before.ends_with("semver_numbers //") || before.ends_with("semver_numbers // ("))
        })
    })
}

/// True when `line` writes a plural in brackets, such as `2 key pair(s)`. `http(s)://` is the
/// notation of a URL scheme, and it is no plural.
fn writes_a_plural_in_brackets(line: &str) -> bool {
    ["(s)", "(es)", "(ies)"].iter().any(|mark| {
        line.match_indices(mark)
            .any(|(at, _)| !line[..at].ends_with("http"))
    })
}

/// Palette types `numCPUs` as an integer of 32 bits, so a body with `-1` parses. The rule exists
/// to catch a count under 1, so it must give its message for `-1` and for `0.5`, with exit `0`.
/// Release 0.28.0 stopped with an error: the filter `count_of` refused the number.
#[test]
fn vsphere_instance_floor_names_a_pool_with_a_cpu_count_under_zero_or_not_whole() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("vsphere-instance-floor").unwrap();
    let fault = catalog::rules_dir().join("fixtures/vsphere/cpu-negative.json");
    let (verdict, messages) = verdict(rule, &fault, &Jq::new());
    assert_eq!(verdict, Verdict::Failed, "{messages:?}");
    assert_eq!(
        messages,
        [
            "pool cp-pool has 0.5 CPUs; the converter needs 1 or more, and the tutorials use 4",
            "pool worker-pool has -1 CPUs; the converter needs 1 or more, and the tutorials use 4",
        ]
    );
}

/// The review at the end of Wave 14 found 22 places that wrote `2 key pair(s)`. A message agrees
/// each noun with its count: `count_of` writes `1 key pair`, `2 key pairs`, and `no key pairs`.
#[test]
fn no_rule_writes_a_plural_in_brackets() {
    let mut found = Vec::new();
    for (name, text) in catalog::SECTIONS {
        for (n, line) in text.lines().enumerate() {
            if writes_a_plural_in_brackets(line) {
                found.push(format!("{name}.yaml:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "texts that write a plural in brackets, use `count_of`:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_plural_test_refuses_each_bracket_form_and_passes_a_url_scheme() {
    for bad in [
        "Palette lists 2 key pair(s)",
        "reads the macro(s) a, b",
        "has 3 box(es)",
        "has 3 entr(ies)",
        r#""\($n) host(s)""#,
    ] {
        assert!(writes_a_plural_in_brackets(bad), "{bad}");
    }
    for good in [
        "apiEndpoint is not http(s)://host:port/MAAS",
        "Palette lists \\($n | count_of(\"key pair\"))",
        "has 2 key pairs",
        "a (simple) text",
    ] {
        assert!(!writes_a_plural_in_brackets(good), "{good}");
    }
}

/// The review at the end of Wave 14 found four rules that cut the version of a pack at the dots.
/// Three other rules read it with `semver_numbers`. Two readings of one text give two answers
/// (#594). A rule that reads `.version` or `.tag` reads the numbers first.
#[test]
fn no_rule_cuts_a_version_at_the_dots_before_it_reads_the_numbers() {
    let mut found = Vec::new();
    for (name, text) in catalog::SECTIONS {
        for (id, rule) in rule_texts(text) {
            if splits_a_version_at_the_dots(rule) {
                found.push(format!("{name}.yaml: {id}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "rules that cut a version at the dots, read it with `semver_numbers` first:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_split_test_refuses_a_cut_of_a_version_and_passes_the_numbers_and_an_address() {
    let rules = rule_texts(
        "section: s\nrules:\n  - id: a\n    expr: '.version | split(\".\") | .[:2]'\n  - id: b\n    expr: '.tag | split(\".\")'\n  - id: c\n    expr: |\n      .version | semver_numbers // split(\".\") | .[:2]\n  - id: d\n    expr: '.version | semver_numbers // (split(\".\") | .[:2])'\n  - id: e\n    expr: '.cidr | split(\".\") | map(tonumber)'\n  - id: f\n    argv: '{{ .packs[0].version | split(\\\".\\\") }}'\n  - id: g\n    expr: '.versions | split(\".\")'\n",
    );
    let found: Vec<&str> = rules
        .iter()
        .filter(|(_, rule)| splits_a_version_at_the_dots(rule))
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(found, ["a", "b", "f"]);
}

/// The messages of `rule` on each fault fixture that it names.
fn fault_messages(catalog: &Catalog, jq: &Jq, id: &str) -> Vec<String> {
    let rule = catalog.rule(id).unwrap();
    rule.tests
        .iter()
        .filter_map(|t| t.fault.as_ref())
        .flat_map(|fault| verdict(rule, fault, jq).1)
        .collect()
}

/// A message agrees each noun with its count (#594). Some fault fixtures hold one thing, and
/// others hold many. So the table shows `1 taint`, `3 enabled regions`, and `no free t3.xlarge
/// slots`.
#[test]
fn the_messages_of_the_rules_agree_each_noun_with_its_count_on_their_fault_fixtures() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    for (id, words) in [
        ("aws-region-not-enabled", "Palette lists 3 enabled regions:"),
        ("eks-fargate-private", "the route table of 2 subnets ("),
        ("dedicated-hosts", "has no free t3.xlarge slots, and"),
        ("aks-system-pool", "carries 1 taint:"),
        (
            "edge-two-node",
            "isTwoNodeCluster with 3 control plane hosts:",
        ),
        ("edge-host-in-use", "part of 1 cluster (other-cluster):"),
        ("edge-host-resources", "has 1 core;"),
        ("ssh-key-pair", "Palette lists 1 key pair"),
        ("gke-pool-roll", "GKE holds no node pools for cluster fred"),
        (
            "gke-pool-roll",
            "the entity has 1 worker pool (worker-pool):",
        ),
        ("macro-silent", "reads 1 macro (strata_probe):"),
        ("eks-one-zone", "names 1 zone (us-east-2a);"),
        ("variables-in-use", "is in use by 1 cluster (payments):"),
        ("pvc-lb-leftover", "2 EBS volumes with the tag"),
        (
            "leftovers-before-build",
            "2 instances of a cluster named fred",
        ),
        (
            "edge-host-duplicate",
            "is in the pools control-plane-pool, worker-pool:",
        ),
    ] {
        let messages = fault_messages(&catalog, &jq, id);
        assert!(
            messages.iter().any(|m| m.contains(words)),
            "{id}: no message has `{words}`: {messages:?}"
        );
    }
}

/// A verb or a pronoun agrees with a count of one and of many: `1 EBS volume ... is available`
/// and `2 instances ... are still`. A count of zero reads `no key pairs`.
#[test]
fn a_message_agrees_its_verb_and_its_pronoun_with_a_count_of_one_and_of_many_and_zero() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let messages = |id: &str, fixture: &str, change: &dyn Fn(&mut serde_json::Value)| {
        let rule = catalog.rule(id).unwrap();
        let mut doc = fixture::load(&catalog::rules_dir().join(fixture)).unwrap();
        change(&mut doc);
        outcome_of(rule, doc, &jq).messages
    };

    // the fixture holds three volumes, and two are available
    let volumes = |keep: usize| {
        messages(
            "pvc-lb-leftover",
            "fixtures/aws/volumes-leftover.json",
            &|doc| {
                let command = doc["_exec"]
                    .as_object_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|(line, _)| line.starts_with("aws ec2 describe-volumes"))
                    .unwrap()
                    .1;
                let all = command["stdout"]["Volumes"].as_array().unwrap().clone();
                let free: Vec<_> = all
                    .into_iter()
                    .filter(|v| v["State"] == "available")
                    .collect();
                command["stdout"]["Volumes"] = free.into_iter().take(keep).collect();
            },
        )
    };
    assert!(
        volumes(1)[0].starts_with("1 EBS volume with the tag"),
        "{:?}",
        volumes(1)
    );
    assert!(
        volumes(1)[0].contains("fred is available and attached"),
        "{:?}",
        volumes(1)
    );
    assert!(
        volumes(2)[0].starts_with("2 EBS volumes with the tag"),
        "{:?}",
        volumes(2)
    );
    assert!(
        volumes(2)[0].contains("fred are available and attached"),
        "{:?}",
        volumes(2)
    );

    // the fixture holds two instances
    let instances = |keep: usize| {
        messages(
            "leftovers-before-build",
            "fixtures/eks/instances-left.json",
            &|doc| {
                let command = doc["_exec"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap();
                let reservations = command["stdout"]["Reservations"].as_array_mut().unwrap();
                let mut left = keep;
                for reservation in reservations.iter_mut() {
                    let list = reservation["Instances"].as_array_mut().unwrap();
                    list.truncate(left);
                    left -= list.len();
                }
            },
        )
    };
    let one = &instances(1)[0];
    assert!(
        one.starts_with("1 instance of a cluster named fred is still in us-east-2:"),
        "{one}"
    );
    assert!(one.contains("; a run before left it, and"), "{one}");
    let two = &instances(2)[0];
    assert!(
        two.starts_with("2 instances of a cluster named fred are still in us-east-2:"),
        "{two}"
    );
    assert!(two.contains("; a run before left them, and"), "{two}");

    // the zero and the many of a count that the message of the key pair shows
    let keys = |names: &[&str]| {
        messages(
            "ssh-key-pair",
            "fixtures/eks/ssh-key-missing.json",
            &|doc| {
                let route =
                    "GET /v1/clouds/aws/regions/us-east-2/keypairs?cloudAccountUid=acc-eks-1";
                doc["_responses"][route]["body"]["keyNames"] = serde_json::json!(names);
            },
        )
    };
    assert!(
        keys(&[])[0].ends_with("Palette lists no key pairs"),
        "{:?}",
        keys(&[])
    );
    assert!(
        keys(&["a", "b"])[0].ends_with("Palette lists 2 key pairs"),
        "{:?}",
        keys(&["a", "b"])
    );
}

/// An image that a vendor rebuilds under one tag is a case for the operator, and not for Palette
/// (#630). The row is a note, because a warning for each image fires on nearly each pack.
#[test]
fn the_rule_of_an_image_named_by_a_tag_is_a_note_for_the_operator_with_the_two_fixes() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("image-tag-not-digest").unwrap();
    assert_eq!(rule.severity, Severity::Note);
    let fix = rule.fix.as_ref().expect("the rule states a fix");
    assert_eq!(fix.owner, Owner::Operator);
    for fix_text in [
        "Name the digest where the chart or the manifest reads it",
        "copy each image to a registry with immutable tags",
    ] {
        assert!(fix.text.contains(fix_text), "{}", fix.text);
    }
    assert!(
        rule.check.is_none(),
        "a note that no check before the build finds"
    );
}

/// The directory of the scripts of the live example. No machine outside the lab has it, so a line
/// that names it sends a person to a path that does not exist (#613).
const LAB_SCRIPTS: &str = "examples/live";

/// The words that start a command or a subcommand that changes something, in lower case, and the
/// names of the options that do. A command counts by its part before the first dash, so
/// `delete-named` and `delete-nodegroup` are deletes, and `describe-vpcs` is no change. A command
/// also counts by its whole name, so `get-credentials` is a change: it writes the kubeconfig. A
/// login writes a file of credentials, so it is a change too. An option counts by its whole name,
/// so `--delete` is a delete, and `--tag-filters` is no change.
const CHANGES: [&str; 54] = [
    "add",
    "annotate",
    "apply",
    "attach",
    "chmod",
    "chown",
    "cordon",
    "cp",
    "create",
    "delete",
    "deregister",
    "destroy",
    "detach",
    "disable",
    "drain",
    "edit",
    "enable",
    "get-credentials",
    "import",
    "install",
    "kill",
    "label",
    "lock",
    "login",
    "logout",
    "modify",
    "mv",
    "patch",
    "push",
    "put",
    "reboot",
    "register",
    "remove",
    "replace",
    "reset",
    "resize",
    "restart",
    "revoke",
    "rm",
    "run",
    "scale",
    "set",
    "start",
    "stop",
    "taint",
    "tee",
    "terminate",
    "uncordon",
    "uninstall",
    "unlock",
    "untag",
    "update",
    "upgrade",
    "use-context",
];

/// The text of a shell line that the shell reads as the words of its commands. Each quoted part,
/// each `{{ }}` template, and each `<placeholder>` becomes a space, and the comment goes. So a jq
/// program, a filter, and a note are no words of a command.
fn command_text(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    let mut word_start = true;
    while let Some(c) = chars.next() {
        let end = match c {
            '\'' | '"' => c,
            '<' if chars.peek().is_some_and(char::is_ascii_alphabetic) => '>',
            '{' if chars.peek() == Some(&'{') => '}',
            '#' if word_start => break,
            '\\' => {
                chars.next();
                word_start = false;
                continue;
            }
            _ => {
                out.push(c);
                word_start = c.is_whitespace();
                continue;
            }
        };
        // a double quote ends at a double quote with no backslash before it. A template ends at
        // its first `}`, and the second `}` is the next character.
        while let Some(inner) = chars.next() {
            if inner == '\\' && end == '"' {
                chars.next();
            } else if inner == end {
                break;
            }
        }
        if end == '}' {
            chars.next();
        }
        out.push(' ');
        word_start = false;
    }
    out
}

/// True when the `curl` of `words` sends a `POST`: `-X POST`, or an option of data, with which
/// `curl` sends a `POST`. A `PUT`, a `PATCH`, and a `DELETE` are words of [`CHANGES`].
fn curl_posts(words: &[&str]) -> bool {
    words.contains(&"curl")
        && words.iter().enumerate().any(|(at, word)| {
            matches!(*word, "-d" | "-F" | "--form" | "--json")
                || word.starts_with("--data")
                || word.eq_ignore_ascii_case("-XPOST")
                || (matches!(*word, "-X" | "--request")
                    && words
                        .get(at + 1)
                        .is_some_and(|method| method.eq_ignore_ascii_case("post")))
        })
}

/// The file that each `>` of `text` writes, such as `request.json` in `jq . > request.json`. A
/// `>&` copies a stream, and writes no file.
fn written_files(text: &str) -> Vec<&str> {
    text.match_indices('>')
        .filter_map(|(at, _)| {
            let rest = text[at + 1..].trim_start_matches('>').trim_start();
            let file = rest
                .split(|c: char| c.is_whitespace() || ";|&)".contains(c))
                .next()?;
            (!rest.starts_with('&') && !file.is_empty()).then_some(file)
        })
        .collect()
}

/// What `line` changes, when it changes something. A change is a word of [`CHANGES`], or a
/// `POST` to a route that is not a validate route. A write of a file outside the current
/// directory is a change too. A validate route of Palette checks a body. `request.json` and
/// `body.json` of the lines that a report adds are files of the current directory.
fn change_in(line: &str) -> Option<String> {
    let text = command_text(line);
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || "|;&()$`<>".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    let changes = |name: &str| CHANGES.contains(&name.to_ascii_lowercase().as_str());
    if let Some(word) = words.iter().find(|word| match word.strip_prefix('-') {
        Some(option) => changes(option.trim_start_matches('-')),
        None => changes(word) || changes(word.split('-').next().unwrap_or_default()),
    }) {
        return Some(format!("the word `{word}`"));
    }
    if curl_posts(&words) && !line.contains("/validate") {
        return Some("a POST to a route that is not a validate route".to_string());
    }
    written_files(&text)
        .into_iter()
        .find(|file| *file != "/dev/null" && file.contains('/'))
        .map(|file| format!("a write of {file}"))
}

/// A person runs a reproduce line by hand to see what a check saw, so the line reads and changes
/// nothing. The report of #613 printed `api.sh delete-named` for `versions-outside-state`: the
/// line deletes each version of the profile, also the version that the cluster runs. The test
/// reads each line of each engine. It also reads the lines that a report prints for the fault
/// fixture of each rule. The report adds lines for the check to them.
#[test]
fn no_reproduce_line_of_the_catalog_deletes_or_changes_anything() {
    let catalog = catalog::builtin().unwrap();
    let jq = Jq::new();
    let dir = folder_for_reproduce();
    let mut found = Vec::new();
    for rule in catalog.rules() {
        let mut lines: Vec<String> = rule.reproduce.lines().map(str::to_string).collect();
        if let Some(fault) = rule.tests.iter().find_map(|t| t.fault.as_ref()) {
            let doc = fixture::load(fault).unwrap();
            lines.extend(reproduce_lines(&catalog, rule, doc, dir.path(), &jq));
        }
        for line in lines {
            if let Some(change) = change_in(&line) {
                found.push(format!("{}: {change}: {line}", rule.id));
            }
        }
    }
    assert!(
        found.is_empty(),
        "reproduce lines that change something:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_change_test_refuses_each_form_of_a_change_and_passes_each_read() {
    for bad in [
        "examples/live/api.sh delete-named aws-stack   # the fix of the lab",
        "examples/live/eks-nodegroups.sh --delete   # deletes the node groups",
        "aws eks delete-nodegroup --cluster-name a --nodegroup-name b",
        "kubectl delete pod x",
        "tofu destroy -auto-approve",
        "gcloud compute instances delete a --zone b",
        r#"curl -s -X DELETE "https://h/v1/clusterprofiles/x""#,
        r#"curl -s -X PUT --data @body.json "https://h/v1/x""#,
        r#"curl -s -X POST -d '{"spectroComponents":"unlock"}' "https://h/v1/spectroclusters/u/upgrade/settings"   # resume"#,
        r#"code=$(curl -s -d @body.json "https://h/v1/x")"#,
        "jq . world.json > ~/.kube/config",
        "rm -rf out",
        "aws ecr get-login-password --region r | crane auth login <account>.dkr.ecr.r.amazonaws.com -u AWS --password-stdin",
        "aws ecr get-login-password --region r | docker login -u AWS --password-stdin <account>.dkr.ecr.r.amazonaws.com",
        "helm registry login <account>.dkr.ecr.r.amazonaws.com -u AWS --password-stdin",
        "crane auth logout <account>.dkr.ecr.r.amazonaws.com",
        "gcloud container clusters get-credentials c --region r",
        "kubectl config use-context c",
    ] {
        assert!(change_in(bad).is_some(), "{bad}");
    }
    for good in [
        "aws ec2 describe-vpcs --filters Name=tag-key,Values=x --query 'Vpcs[].VpcId' --output text   # no id before a force delete",
        "aws resourcegroupstaggingapi get-resources --tag-filters Key=k --output text",
        "maas $PROFILE tag machines palette | jq -r '.[] | .hostname'",
        r#"curl -s -X POST -H 'Content-Type: application/json' --data @request.json "https://h/v1/spectroclusters/aws/validate""#,
        "jq '.validateBody' out/world.json > request.json",
        r#"for t in tofu jq; do command -v $t >/dev/null || echo "$t is missing"; done"#,
        "aws iam simulate-principal-policy --action-names eks:CreateCluster ec2:RunInstances --output json",
        r#"gcloud logging read 'protoPayload.methodName="CreateCluster" severity>=WARNING' --limit 3"#,
        r"curl -s -o /dev/null -w '%{http_code}\n' https://<registry host>/v2/",
        r#"jq '"a '\''delete'\'' b"' world.json"#,
        "# only the lab can reproduce this row; a company deletes nothing",
        "aws ecr get-login-password --region r   # must print a token",
        "crane digest <account>.dkr.ecr.r.amazonaws.com/app:1.0.0",
        "gcloud auth print-access-token",
        r#"strata list profiles --json | jq -r --arg n {{ .profiles[0].source.name // "<name>" }} '.profiles[] | .version'"#,
    ] {
        assert_eq!(change_in(good), None, "{good}");
    }
}

/// No machine outside the lab has the scripts of the live example. So no text of the catalog
/// names their directory: no reproduce line, no message, no fix, and no evidence. A report holds
/// no other text of the catalog. A rule that only the lab can reproduce says so in words (#613).
#[test]
fn no_text_of_the_catalog_names_the_directory_of_the_scripts_of_the_lab() {
    let mut found = Vec::new();
    for (name, text) in catalog::SECTIONS {
        for (n, line) in text.lines().enumerate() {
            if line.contains(LAB_SCRIPTS) {
                found.push(format!("{name}.yaml:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "texts that name {LAB_SCRIPTS}:\n{}",
        found.join("\n")
    );
}

/// `inventory-managed` is a fault of the inventory scripts of the lab. Only the lab can reproduce
/// it, and its reproduce line says so in words for each engine of the rule.
#[test]
fn a_rule_that_only_the_lab_can_reproduce_says_so_in_words() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("inventory-managed").unwrap();
    for engine in rule.engines.iter().filter(|e| e.as_str() != "lab") {
        let lines = rule.reproduce.for_engine(engine);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("# only the lab can reproduce")),
            "{engine}: {lines:?}"
        );
    }
}
