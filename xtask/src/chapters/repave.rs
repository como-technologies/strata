//! The chapter `repave`: the answer of Palette about a repave, beside the impact table.

use super::DEV_ENV;
use super::edits::RETAIN;
use super::ids::{A111, A112, A116, A117, AWS_STANDARD, DEV, PAYMENTS};
use crate::book::Runner;
use crate::tenant::{Edit, PackRefusal};

/// The chapter `repave`: the answer of Palette beside the impact table, for a change that repaves.
pub(super) fn run(r: &mut Runner) {
    // Palette's own answer beside the impact table: a change that repaves, and one that does not
    r.chapter("repave", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile(
        DEV,
        A116,
        "1.1.6",
        "aws-stack@1.1.1",
        &[Edit::KubeadmCommand("echo strata-repave-test")],
    );
    r.profile(DEV, A112, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, true));
    r.step(
        "repave-kubeadm",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave",
    );
    r.excerpt(
        "repave-kubeadm-lines",
        "repave-kubeadm",
        r"^(aws-stack|root |note |palette|[ ~] \[k8s|      impact)",
    );
    r.step(
        "repave-kubeadm-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave --json | jq .paletteRepave",
    );
    // The comment of the pull request that moves the cluster: the answer of Palette decides the
    // label. The table says `may-repave` for this change, which is below the threshold
    r.step(
        "repave-pr-comment",
        0,
        true,
        "STRATA_QUIET=1 scripts/ci/pr-comment.sh cluster:payments aws-stack@1.1.6 --repave --dry-run > comment.md",
    );
    r.step(
        "repave-pr-comment",
        0,
        false,
        "grep '^\\*\\*Palette' comment.md",
    );
    repave_records(r);
    r.serve_here(); // state 1: Palette says that the kubeadm change repaves
    repave_csi(r);
    repave_disagree(r);
    repave_validate(r);
    repave_approval(r);
    repave_template(r);
}

/// The part of the chapter `repave` for two records with no cluster: the error, and the two
/// commands that it names.
fn repave_records(r: &mut Runner) {
    for version in ["1.1.1", "1.1.6"] {
        r.step(
            "repave-records-usage",
            0,
            true,
            &format!("strata snapshot aws-stack@{version} -q -o {version}.json"),
        );
    }
    for question in ["--repave", "--validate"] {
        r.step(
            "repave-records-usage",
            2,
            true,
            &format!("strata compare 1.1.1.json 1.1.6.json {question}"),
        );
    }
    // a record is the profile side beside a cluster, and plain compare gives the impact
    r.step(
        "repave-records-forward",
        1,
        false,
        "strata compare cluster:payments 1.1.6.json --repave | grep -E '^(aws-stack|palette)'",
    );
    r.step(
        "repave-records-forward",
        1,
        false,
        "strata compare 1.1.1.json 1.1.6.json | grep -E '^(aws-stack|      impact)'",
    );
}

/// The part of the chapter `repave` for a change that does not repave.
fn repave_csi(r: &mut Runner) {
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, false));
    r.step(
        "repave-csi",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.2 --repave",
    );
    r.excerpt(
        "repave-csi-lines",
        "repave-csi",
        r"^(aws-stack|root |note |palette|[ ~] \[csi|      impact)",
    );
    r.step(
        "repave-pr-comment-csi",
        0,
        true,
        "STRATA_QUIET=1 scripts/ci/pr-comment.sh cluster:payments aws-stack@1.1.2 --repave --dry-run > comment.md",
    );
    r.step(
        "repave-pr-comment-csi",
        0,
        false,
        "grep '^\\*\\*Palette' comment.md",
    );
    r.profile(
        DEV,
        A117,
        "1.1.7",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "kubernetes",
            version: "1.36.3",
        }],
    );
    r.serve_here(); // state 2: Palette says that no change repaves, and 1.1.7 exists
}

/// The part of the chapter `repave` where the table and Palette disagree.
fn repave_disagree(r: &mut Runner) {
    r.step(
        "repave-disagree",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --repave",
    );
    r.excerpt(
        "repave-disagree-lines",
        "repave-disagree",
        r"^(aws-stack|root |note |palette|[ ~] \[k8s|      impact)",
    );
    // the table says repave for a new Kubernetes version, and the stand-in says no, as Palette
    // answers for an Edge cluster. The label follows the answer, and the comment carries the note
    // of the disagreement
    r.step(
        "repave-pr-comment-disagree",
        0,
        true,
        "STRATA_QUIET=1 scripts/ci/pr-comment.sh cluster:payments aws-stack@1.1.7 --repave --dry-run > comment.md",
    );
    r.step(
        "repave-pr-comment-disagree",
        0,
        false,
        "grep -E '^\\*\\*(Palette|Note)' comment.md",
    );
    // two versions have no cluster to ask about, so the table decides
    r.step(
        "repave-pr-comment-gap",
        0,
        true,
        "STRATA_QUIET=1 scripts/ci/pr-comment.sh aws-stack@1.1.1 aws-stack@1.1.6 --repave --dry-run > comment.md",
    );
    r.step(
        "repave-usage",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.6 --repave",
    );
}

/// The part of the chapter `repave` for the check of the packs of a change on a cluster.
fn repave_validate(r: &mut Runner) {
    // Palette's own check of the packs of the change on the cluster: a refusal, and an accept
    r.tenant(|t| {
        t.packs_answer(
            DEV,
            PAYMENTS,
            Some(PackRefusal {
                profile: A117,
                pack: "kubernetes",
                required: "cni-cilium-oss",
                version: "1.21.0",
            }),
        )
    });
    r.step(
        "validate-refuses",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --validate",
    );
    r.excerpt(
        "validate-refuses-lines",
        "validate-refuses",
        r"^(aws-stack|root |palette|[ ~] \[k8s|  \[cni)",
    );
    r.step(
        "validate-refuses-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.7 --validate --json | jq .paletteValidation",
    );
    r.tenant(|t| t.packs_answer(DEV, PAYMENTS, None));
    r.step(
        "validate-accepts",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.2 --validate",
    );
    r.excerpt(
        "validate-accepts-lines",
        "validate-accepts",
        r"^(aws-stack|root |palette|[ ~] \[csi)",
    );
    r.step(
        "validate-usage",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.7 --validate",
    );
    // payments runs aws-stack@1.1.1. The route of the cluster checks no sent tag for a profile that
    // the cluster runs, so strata asks the route of the version. The stand-in refuses there
    r.tenant(|t| {
        t.profile_packs_answer(
            DEV,
            Some(A111),
            Some(("kubernetes", "cni-cilium-oss", "1.21.0")),
        )
    });
    r.step(
        "validate-running",
        0,
        false,
        "strata compare cluster:payments aws-stack@1.1.1 --validate",
    );
    r.excerpt(
        "validate-running-lines",
        "validate-running",
        r"^(aws-stack|root |palette)",
    );
}

/// The part of the chapter `repave` for a repave that waits for approval.
fn repave_approval(r: &mut Runner) {
    // a repave that waits for approval: the cluster record holds it outside the root, and verify
    // names it (#107)
    r.step(
        "repave-waits-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.tenant(|t| t.repave_waits(DEV, PAYMENTS, "2026-09-10T10:15:00Z"));
    r.step(
        "repave-waits-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );
    r.excerpt(
        "repave-waits-verify-lines",
        "repave-waits-verify",
        r"^(aws-stack|root |note |OK)",
    );
    r.step(
        "repave-waits-json",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments-now.json",
    );
    r.step(
        "repave-waits-json",
        0,
        false,
        "jq '.source.cluster | {repavePending, pendingRepave}' runs/payments-now.json",
    );
}

/// The part of the chapter `repave` for a cluster that a cluster template governs.
fn repave_template(r: &mut Runner) {
    // A cluster that a cluster template governs. On the route of the cluster, Palette compares
    // nothing and answers no repave, so --repave asks the route of the template (#130)
    r.tenant(|t| t.cluster_in_template(DEV, PAYMENTS, AWS_STANDARD));
    r.tenant(|t| t.template_repave_answer(DEV, AWS_STANDARD, true));
    r.step(
        "repave-template",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave",
    );
    r.excerpt(
        "repave-template-lines",
        "repave-template",
        r"^(aws-stack|root |palette|[ ~] \[k8s|      impact)",
    );
    r.step(
        "repave-template-json",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.6 --repave --json | jq .paletteRepave",
    );
    r.step(
        "repave-template-governs",
        2,
        true,
        "strata compare cluster:payments aws-stack@1.1.1 --repave",
    );
}
