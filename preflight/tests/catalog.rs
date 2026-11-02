//! Every rule of the catalog that names a test runs it: the check fails on the fault fixture and
//! passes on the clean one. A rule whose `when` does not hold on its fixture fails the test too,
//! so a guard cannot hide a check that never runs. Each fixture file under rules/fixtures/ belongs
//! to a rule, and each fixture a rule names exists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use strata_preflight::catalog;
use strata_preflight::fixture;
use strata_preflight::jq::Jq;
use strata_preflight::probe::{self, Call, FixtureBackend, Outcome, Verdict};
use strata_preflight::rule::{Check, Evidence, EvidenceKind, ExecCheck, PaletteCheck, Rule};

/// The request of the instance type route for the fixtures of the AWS, GCP, and Azure rules, as the
/// rules send it. Its query lifts the default filter of Palette, which drops each type with fewer
/// than 2 vCPUs or less than 4 GiB of memory (#393).
const AWS_TYPES: &str = "GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-aws-1&cpuGtEq=0&memoryGtEq=0";
const GCP_TYPES: &str = "GET /v1/clouds/gcp/regions/us-east1/instancetypes?cloudAccountUid=acc-gcp-1&cpuGtEq=0&memoryGtEq=0";
const AZURE_TYPES: &str = "GET /v1/clouds/azure/regions/eastus/instancetypes?cloudAccountUid=acc-azure-1&subscriptionId=11111111-2222-3333-4444-555555555555&cpuGtEq=0&memoryGtEq=0";

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

/// True when `text` has a call of a Go function with no selector, such as `CheckPool(`: a name
/// that starts with a capital letter, and `(` right after it.
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

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn the_ids_of_the_catalog_of_the_issue_are_all_here_or_listed_as_pending() {
    // the 149 rows of #91, by id. A row that is not a rule yet is in PENDING below, and the count
    // there goes down as the catalog grows; it never goes up.
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
    "iam-ten-policies",
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

/// A rule that no validate route answers stays in the catalog as a gap that preflight fills, and
/// its note names the route that takes the fault (#415).
#[test]
fn a_rule_that_the_cluster_validate_route_does_not_answer_says_so_in_its_note() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("pack-cloud-support").unwrap();
    assert!(rule.what.contains("no route answers"), "{}", rule.what);
    assert!(rule.what.contains("palette-validate"), "{}", rule.what);
}

/// The fault is in the second item, not the first (#144): a profile that comes second, a host of
/// the second pool, a second pool with a host resource group, and a second compute cluster. Each
/// check reads every item, and each message names the item that holds the fault and no other.
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

/// The check of `instance-offering` fails, and says so, when Palette refuses the request or gives
/// no list: a wrong answer is never a pass.
#[test]
fn instance_offering_fails_when_palette_refuses_or_gives_no_list_of_types() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/instance-zone-offered.json");
    let key = AWS_TYPES;
    for (answer, expected) in [
        (
            serde_json::json!({"ok": false, "status": 403, "body": {"code": "AccessDenied", "message": "no"}}),
            "Palette cannot list the instance types of region us-east-2: AccessDenied: no",
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
}

/// The three quota rules count the vCPUs of a pool from the field `cpu` of the instance type route
/// (#393), and not from the name of the type. Each sends one request beside its command. The quota
/// still comes from the tool of the cloud.
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
/// type route when the query says nothing. The three quota rules send `cpuGtEq=0&memoryGtEq=0`, so a
/// pool of a small type adds its vCPUs: the pools of the fixtures ask for more than the quota gives,
/// and the rule says so. With no query, the route lists no small type, and the rule counts 0 vCPUs
/// for that pool.
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
            "the pools ask for 8 vCPUs in eastus, and the regional quota has 2 free of 10 (8 in use): Azure refuses the machines with OperationNotAllowed; the family quota of each type can be lower still",
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

/// A type that the route still does not list, such as an old family, does not add 0 vCPUs: the rule
/// fails closed for that pool, and names the pool and the type. A listed entry with no number in
/// `cpu` is the same. The message of a short quota counts the listed pools alone.
#[test]
fn a_quota_rule_fails_for_a_pool_whose_type_the_route_does_not_list() {
    let catalog = catalog::builtin().unwrap();
    for (id, fixture, expected) in [
        (
            "aws-quotas",
            "fixtures/aws/vcpu-quota-type-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the type m3.large in region us-east-2, so the check cannot count the vCPUs of the pool",
        ),
        (
            "gcp-quotas",
            "fixtures/gke/cpu-quota-type-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the type n1-standard-1 in region us-east1, so the check cannot count the CPUs of the pool",
        ),
        (
            "azure-vcpu-quota",
            "fixtures/azure/vcpu-quota-size-not-listed.json",
            "pool worker-pool: the instance type route of Palette lists no cpu for the size Standard_A1 in region eastus, so the check cannot count the vCPUs of the pool",
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
            "pool worker-pool: the instance type route of Palette lists no cpu for the type m3.large in region us-east-2, so the check cannot count the vCPUs of the pool",
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

/// A list that Palette refuses, or gives no `instanceTypes`, is a failure that says so: a quota rule
/// cannot count the vCPUs of any pool then.
#[test]
fn a_quota_rule_fails_when_palette_refuses_the_list_of_instance_types() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("aws-quotas").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/vcpu-quota-clean.json");
    let doc = fixture::load(&path).unwrap();
    for (answer, expected) in [
        (
            serde_json::json!({"ok": false, "status": 403, "body": {"code": "AccessDenied", "message": "no"}}),
            "Palette cannot list the instance types of region us-east-2: AccessDenied: no",
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
            "sshKeyName iris-on-palette is not a key pair of the account in region us-east-2; Palette lists 1 key pair(s)"
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
    for engine in [
        "aws", "gcp", "gke", "azure", "aks", "maas", "edge", "vsphere",
    ] {
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
