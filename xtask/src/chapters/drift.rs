//! The chapters of the drift of a version, and of the clusters that run one.

use super::DEV_ENV;
use super::edits::RETAIN;
use super::ids::{A110, A111, A112, A120, ADDONS_DEV, ADDONS_PACK, DEV, LEDGER, PAYMENTS, SEARCH};
use crate::book::Runner;
use crate::tenant::Edit;

/// Run the chapters of the drift of a version, and of the clusters that run one.
pub(super) fn run(r: &mut Runner) {
    edited_in_place(r);
    draft(r);
    overwritten(r);
    cluster_release(r);
    cluster_values(r);
}

/// The chapter `edited-in-place`: a person edits a published version through the API.
fn edited_in_place(r: &mut Runner) {
    r.chapter("edited-in-place", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "edited-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "edited-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "edited-verify-ok",
        0,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.serve_here(); // state 1: the records are taken, and nobody has edited the version
    r.tenant(|t| t.edit(DEV, A111, RETAIN));
    r.step(
        "edited-verify",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    // the verdict alone: it ends with the time of the last change of the document (#140)
    r.excerpt("edited-verify-verdict", "edited-verify", r"^DRIFT");
    // the same drift, with the unified diff of the edit: the page teaches the lines that --full adds
    r.step(
        "edited-verify-full-all",
        1,
        false,
        "strata verify releases/1.1.1/dev.json --full",
    );
    r.excerpt(
        "edited-verify-full",
        "edited-verify-full-all",
        r"^([$]|DRIFT|--- |[+][+][+] |[-+] +reclaimPolicy)",
    );
    // the same drift, and who made it: the page teaches the lines that --who adds
    r.step(
        "edited-verify-who-full",
        1,
        false,
        "strata verify releases/1.1.1/dev.json --who",
    );
    r.excerpt(
        "edited-verify-who",
        "edited-verify-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    edited_in_place_cluster(r);
    edited_in_place_clusters(r);
    edited_in_place_stale(r);
}

/// The part of the chapter `edited-in-place` for the cluster that runs the version, and the update
/// that waits.
fn edited_in_place_cluster(r: &mut Runner) {
    // the edit waits for payments, which runs the version: the listing says so
    r.step(
        "edited-list-behind",
        0,
        false,
        "strata list clusters --applied",
    );
    r.step(
        "edited-verify-who-json",
        1,
        false,
        "strata verify releases/1.1.1/dev.json --who --json | jq '.verify.who'",
    );
    r.step(
        "edited-compare",
        1,
        false,
        "strata compare releases/1.1.1/dev.json aws-stack@1.1.1",
    );
    r.step(
        "edited-cluster-kept",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // The step shows what the update that waits changes on payments, before anybody accepts it
    // (#78)
    r.step(
        "edited-pending-record",
        0,
        true,
        "strata snapshot cluster:payments@pending -o review/payments-pending.json",
    );
    r.step(
        "edited-pending",
        1,
        false,
        "strata compare cluster:payments cluster:payments@pending",
    );
    // the update waits: does accepting it repave? A change of the CSI values repaves nothing.
    r.tenant(|t| t.repave_answer(DEV, PAYMENTS, false));
    r.step(
        "edited-repave",
        1,
        false,
        "strata compare cluster:payments cluster:payments@pending --repave",
    );
    r.excerpt(
        "edited-repave-lines",
        "edited-repave",
        r"^(aws-stack|root |note |palette|      impact)",
    );
}

/// The part of the chapter `edited-in-place` for two clusters, and the reports of a scheduled job.
fn edited_in_place_clusters(r: &mut Runner) {
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.serve_here(); // state 2: the edit is in Palette, and search started after it
    r.step(
        "edited-two-clusters",
        1,
        false,
        "strata compare cluster:payments cluster:search",
    );
    // Palette answers for one cluster: two clusters ask nothing
    r.step(
        "edited-repave-usage",
        2,
        true,
        "strata compare cluster:payments cluster:search --repave",
    );
    r.step(
        "edited-bulk",
        1,
        false,
        "strata verify 'releases/**/*.json' 'runs/*.json'",
    );
    r.step(
        "edited-bulk-json",
        1,
        false,
        "strata verify 'releases/**/*.json' 'runs/*.json' --json | jq '.summary'",
    );
    // with --full, the row of a record that drifted carries the unified text of each changed pack
    r.step(
        "edited-bulk-full-json",
        1,
        false,
        "strata verify 'releases/**/*.json' 'runs/*.json' --full --json | jq -c '.records[] | {path, status, unifiedFor: [.diff.packs[]? | select(.values.unified) | .name]}'",
    );
    r.step(
        "edited-report",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'runs/*.json' --dry-run",
    );
    // the same report with --who: the body of the issue names who made the edit, and when
    r.step(
        "edited-report-who-full",
        1,
        true,
        "scripts/ci/verify-report.sh 'releases/**/*.json' 'runs/*.json' --who --dry-run",
    );
    r.excerpt(
        "edited-report-who",
        "edited-report-who-full",
        r"^(<!-- strata-verify|\*\*DRIFT|who:|- 20)",
    );
}

/// The part of the chapter `edited-in-place` for a record of the update that is older than the
/// cluster.
fn edited_in_place_stale(r: &mut Runner) {
    // A later change moves payments to another profile, so the record of the update that waits is
    // older than the cluster. The cluster document shows it, and the refusal has its own kind
    r.profile(DEV, A112, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A112], &[]));
    r.step(
        "edited-stale-record",
        2,
        true,
        "strata compare cluster:payments review/payments-pending.json --repave",
    );
    r.step(
        "edited-stale-record-json",
        2,
        true,
        "strata compare cluster:payments review/payments-pending.json --repave --json --quiet",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "edited-cluster-updated",
        1,
        false,
        "strata verify runs/payments.json",
    );
}

/// The chapter `draft`: a person edits a version in the console and does not publish.
fn draft(r: &mut Runner) {
    r.chapter("draft", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "draft-release",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "draft-none",
        2,
        true,
        "strata snapshot aws-stack@1.1.1@draft",
    );
    r.serve_here(); // state 1: aws-stack@1.1.1 has no draft, and the release record is there
    // a person edits the version in the console, and does not publish
    r.tenant(|t| t.draft(DEV, A111, RETAIN));
    r.serve_here(); // state 2: a draft of aws-stack@1.1.1 waits
    r.step(
        "draft-compare",
        1,
        false,
        "strata compare aws-stack@1.1.1 aws-stack@1.1.1@draft",
    );
    r.step(
        "draft-version-holds",
        0,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "draft-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.1@draft -o review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-template",
        0,
        false,
        "jq -r '.source | \"\\(.ref)  \\(.template)  uid \\(.uid)\"' review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-candidate",
        0,
        true,
        "strata candidate aws-stack@1.1.1@draft -o aws-stack.yaml",
    );
    // the reviewer approves, and the person publishes the draft
    r.tenant(|t| t.publish(DEV, A111));
    r.serve_here(); // state 3: the draft is published
    r.step(
        "draft-published",
        0,
        false,
        "strata verify review/aws-stack-1.1.1-draft.json",
    );
    r.step(
        "draft-release-drift",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "draft-gone",
        2,
        true,
        "strata snapshot aws-stack@1.1.1@draft",
    );
}

/// The chapter `overwritten`: a bump that takes the uid of the version, with the flag off.
fn overwritten(r: &mut Runner) {
    r.chapter("overwritten", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.step(
        "overwritten-records",
        0,
        true,
        "strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json",
    );
    r.step(
        "overwritten-records",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.serve_here(); // state 1: the records are taken, and nobody has bumped the version
    r.tenant(|t| {
        t.edit(
            DEV,
            A111,
            &[Edit::Version("1.2.0"), Edit::Reclaim("Retain")],
        )
    });
    r.serve_here(); // state 2: the bump took the uid of 1.1.1, with the flag off
    r.step("overwritten-list", 0, false, "strata list profiles");
    r.step(
        "overwritten-verify",
        1,
        false,
        "strata verify releases/1.1.1/dev.json",
    );
    r.step(
        "overwritten-cluster",
        1,
        false,
        "strata verify runs/payments.json",
    );
    overwritten_consumers(r);
}

/// The part of the chapter `overwritten` for the writer of the bump, and for what the bump leaves
/// behind.
fn overwritten_consumers(r: &mut Runner) {
    // the bump is a write to the profile, and not to the cluster: --who reads both logs (#140)
    r.step(
        "overwritten-cluster-who-full",
        1,
        false,
        "strata verify runs/payments.json --who",
    );
    r.excerpt(
        "overwritten-cluster-who",
        "overwritten-cluster-who-full",
        r"^(aws-stack|DRIFT|who:|  \d{4}-)",
    );
    r.step(
        "overwritten-consumers",
        1,
        false,
        "strata consumers aws-stack@1.2.0",
    );
    r.step(
        "overwritten-compare",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.2.0",
    );
    r.step("overwritten-candidate", 0, true, "strata candidate releases/1.1.1/dev.json -o aws-stack-1.1.1.yaml --files-dir aws-stack-1.1.1");
    r.tenant(|t| {
        t.edit(
            DEV,
            A111,
            &[Edit::Version("1.1.1"), Edit::Reclaim("Delete")],
        )
    });
    r.profile(DEV, A120, "1.2.0", "aws-stack@1.1.1", RETAIN);
    // state 3, which serve_last reaches at the next chapter: the same bump with the flag on
    r.step("overwritten-flag-list", 0, false, "strata list profiles");
    r.step(
        "overwritten-flag-verify",
        0,
        false,
        "strata verify releases/1.1.1/dev.json runs/payments.json",
    );
}

/// The chapter `cluster-release`: the record of a release against the clusters that run it.
fn cluster_release(r: &mut Runner) {
    r.chapter("cluster-release", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110, ADDONS_DEV], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, LEDGER, "ledger", &[A110], &[]));
    r.tenant(|t| t.on_cluster_reclaim(DEV, LEDGER, "Retain"));
    r.step(
        "cluster-release-record",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o releases/1.1.0/dev.json",
    );
    r.step(
        "cluster-applied",
        0,
        false,
        "strata list clusters --applied",
    );
    r.step(
        "cluster-identical",
        0,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments/aws-stack",
    );
    r.step(
        "cluster-all-profiles",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:payments",
    );
    r.step(
        "cluster-moved",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:search",
    );
    r.step(
        "cluster-edited",
        1,
        false,
        "strata compare releases/1.1.0/dev.json cluster:ledger",
    );
    // the page tells the reader to run this next, so the page must show it
    r.step(
        "cluster-edited-verify",
        0,
        false,
        "strata verify releases/1.1.0/dev.json",
    );
    r.step(
        "cluster-run-record",
        0,
        true,
        "strata snapshot cluster:payments/aws-stack -o runs/payments.json",
    );
    r.step(
        "cluster-run-record",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // the page teaches that verify fetches the cluster again with the same selection: the record,
    // the two roots and the verdict. The packs of the profile are not its subject.
    r.excerpt(
        "cluster-run-verify",
        "cluster-run-record",
        r"^([$] |strata: wrote|root |OK:)",
    );
}

/// The chapter `cluster-values`: the variables that two clusters of one release hold.
fn cluster_values(r: &mut Runner) {
    r.chapter("cluster-values", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| {
        t.cluster(
            DEV,
            PAYMENTS,
            "payments",
            &[A111],
            &[("platform_owner", "team-payments")],
        )
    });
    r.tenant(|t| {
        t.cluster(
            DEV,
            SEARCH,
            "search",
            &[A111],
            &[("platform_owner", "team-search")],
        )
    });
    r.step(
        "values-compare",
        0,
        false,
        "strata compare cluster:payments cluster:search",
    );
    r.step(
        "cluster-values-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments.json",
    );
    r.step(
        "values-record",
        0,
        false,
        "jq .source.cluster.variables runs/payments.json",
    );
    r.serve_here(); // state 1: each cluster keeps the value it started with
    r.tenant(|t| t.set_value(DEV, PAYMENTS, "platform_owner", "team-platform"));
    // state 2, which serve_last reaches at the next chapter: a person changed the value
    r.step(
        "values-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );
    // the page tells a job to read records[].clusterVariables, so the page must show that shape
    r.step(
        "values-json",
        0,
        true,
        "strata snapshot cluster:search -o runs/search.json",
    );
    r.step(
        "values-json",
        0,
        false,
        r#"strata verify 'runs/*.json' --format json | jq '.records[] | {path, clusterVariables}'"#,
    );
}
