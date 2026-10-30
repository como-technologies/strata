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
use strata_preflight::rule::{Check, Rule};

fn verdict(rule: &Rule, fixture_path: &Path, jq: &Jq) -> (Verdict, Vec<String>) {
    let doc = fixture::load(fixture_path)
        .unwrap_or_else(|e| panic!("{}: fixture {}: {e}", rule.id, fixture_path.display()));
    let (backend, world) = FixtureBackend::split(doc);
    if let Some(when) = &rule.when {
        let holds = jq
            .first(&rule.id, when, &world)
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
        assert_eq!(jq.first(id, when, &manual).unwrap(), false, "{id}");
        assert_eq!(jq.first(id, when, &managed).unwrap(), true, "{id}");
    }
}

/// Rule 7 of the book: a rule states what Palette does, and names no file of the Palette code.
const PALETTE_CODE: [&str; 6] = [
    "hubble",
    "hapi",
    "palette-sdk-go",
    "terraform-provider-spectrocloud",
    "optic src/",
    "pkg/",
];

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
            if PALETTE_CODE.iter().any(|m| lower.contains(m)) || names_a_go_file(&lower) {
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
