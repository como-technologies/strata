//! Every rule of the catalog that names a test runs it: the check fails on the fault fixture and
//! passes on the clean one. A rule whose `when` does not hold on its fixture fails the test too,
//! so a guard cannot hide a check that never runs. Each fixture file under rules/fixtures/ belongs
//! to a rule, and each fixture a rule names exists.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use strata_preflight::catalog;
use strata_preflight::fixture;
use strata_preflight::jq::Jq;
use strata_preflight::probe::{self, FixtureBackend, Verdict};
use strata_preflight::rule::{Check, Evidence, Rule};

fn verdict(rule: &Rule, fixture_path: &Path, jq: &Jq) -> (Verdict, Vec<String>) {
    let doc = fixture::load(fixture_path)
        .unwrap_or_else(|e| panic!("{}: fixture {}: {e}", rule.id, fixture_path.display()));
    let (backend, world) = FixtureBackend::split(doc);
    if let Some(when) = &rule.when {
        let holds = jq
            .first(&rule.id, when, &world, None)
            .unwrap_or_else(|e| panic!("{}: when: {e}", rule.id));
        assert_eq!(
            holds,
            serde_json::Value::Bool(true),
            "{}: `when` ({when}) does not hold on {}, so the check would not run",
            rule.id,
            fixture_path.display()
        );
    }
    let outcome = match rule.check {
        Some(Check::Watch { .. }) => probe::judge_watch(rule, &world, jq),
        _ => probe::run_check(rule, &world, &backend, jq),
    };
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
            kind: "source".to_string(),
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
                .any(|e| e.kind == "source" && e.note.is_empty())
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

/// The rows of the catalog of #91, in the order of its three comments.
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
    "gke-pool-multiple-3",
    "instance-offering",
    "spot-capacity",
    "eks-worker-needed",
    "eks-ami-type",
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
    "profile-cloud-mismatch",
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
    let (backend, world) = FixtureBackend::split(fixture::load(clean).unwrap());
    let out = probe::run_check(rule, &world, &backend, &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
    assert_eq!(
        out.calls.iter().map(|c| c.ran.as_str()).collect::<Vec<_>>(),
        [
            "GET /v1/edgehosts/edge-host-cp-1",
            "GET /v1/edgehosts/edge-host-w-1",
            "GET /v1/edgehosts/edge-host-w-2",
        ]
    );
}

/// The messages of `instance-offering` name the pool, the type, and the zone (#143), also when the
/// region does not list the type, and the rule asks one request for every pool.
#[test]
fn instance_offering_names_the_pool_the_type_and_the_zone_for_aws_and_gcp() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    for (fixture, ran, expected) in [
        (
            "fixtures/aws/instance-zone-not-offered.json",
            "GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-aws-1",
            vec![
                "pool cp-pool: the type t3.xlarge is not offered in the zone us-east-2a",
                "pool worker-pool: the type t3.xlarge is not offered in the zone us-east-2a",
            ],
        ),
        (
            "fixtures/aws/instance-type-not-listed.json",
            "GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-aws-1",
            vec![
                "pool cp-pool: the type t3.xlarge is not in the list of region us-east-2",
                "pool worker-pool: the type t3.xlarge is not in the list of region us-east-2",
            ],
        ),
        (
            "fixtures/gcp/instance-zone-not-offered.json",
            "GET /v1/clouds/gcp/regions/us-east1/instancetypes?cloudAccountUid=acc-gcp-1",
            vec![
                "pool cp-pool: the type e2-standard-4 is not offered in the zone us-east1-b",
                "pool worker-pool: the type e2-standard-4 is not offered in the zone us-east1-b",
            ],
        ),
    ] {
        let path = catalog::rules_dir().join(fixture);
        let (backend, world) = FixtureBackend::split(fixture::load(&path).unwrap());
        let out = probe::run_check(rule, &world, &backend, &Jq::new());
        assert_eq!(
            out.verdict,
            Verdict::Failed,
            "{fixture}: {:?}",
            out.messages
        );
        assert_eq!(out.messages, expected, "{fixture}");
        assert_eq!(
            out.ran.as_deref(),
            Some(ran),
            "{fixture}: one request answers every pool"
        );
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
    let (backend, world) = FixtureBackend::split(doc);
    let out = probe::run_check(rule, &world, &backend, &Jq::new());
    assert_eq!(out.verdict, Verdict::Passed, "{:?}", out.messages);
}

/// The check of `instance-offering` fails, and says so, when Palette refuses the request or gives
/// no list: a wrong answer is never a pass.
#[test]
fn instance_offering_fails_when_palette_refuses_or_gives_no_list_of_types() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("instance-offering").unwrap();
    let path = catalog::rules_dir().join("fixtures/aws/instance-zone-offered.json");
    let key = "GET /v1/clouds/aws/regions/us-east-2/instancetypes?cloudAccountUid=acc-aws-1";
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
        let (backend, world) = FixtureBackend::split(doc);
        let out = probe::run_check(rule, &world, &backend, &Jq::new());
        assert_eq!(out.verdict, Verdict::Failed, "{:?}", out.messages);
        assert_eq!(out.messages, [expected]);
    }
}

/// The message of `ssh-key-pair` names the key pair and the region, for AWS and for EKS with a
/// dynamic VPC (#143).
#[test]
fn ssh_key_pair_names_the_key_pair_and_the_region_for_aws_and_eks() {
    let catalog = catalog::builtin().unwrap();
    let rule = catalog.rule("ssh-key-pair").unwrap();
    for fixture in [
        "fixtures/aws/ssh-key-missing.json",
        "fixtures/eks/ssh-key-missing.json",
    ] {
        let path = catalog::rules_dir().join(fixture);
        let (backend, world) = FixtureBackend::split(fixture::load(&path).unwrap());
        assert_eq!(
            world["entity"]["spec"]["cloudConfig"]["vpcId"], "",
            "{fixture}: a dynamic VPC"
        );
        let out = probe::run_check(rule, &world, &backend, &Jq::new());
        assert_eq!(
            out.verdict,
            Verdict::Failed,
            "{fixture}: {:?}",
            out.messages
        );
        assert_eq!(
            out.messages,
            [
                "sshKeyName iris-on-palette is not a key pair of the account in region us-east-2; Palette lists 1 key pair(s)"
            ],
            "{fixture}"
        );
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
        let (backend, world) = FixtureBackend::split(fixture::load(&path).unwrap());
        let out = probe::run_check(rule, &world, &backend, &Jq::new());
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
