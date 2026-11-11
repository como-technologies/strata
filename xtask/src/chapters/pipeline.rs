//! The chapters of a pipeline: the sample pipeline and its versions, of How it works, and a release
//! under new version numbers, of the use cases.

use super::DEV_ENV;
use super::edits::{RC_PINS, RETAIN};
use super::ids::{
    A111, DEV, PAYMENTS, RENUMBER_EDITED, RENUMBER_FINAL, RENUMBER_PRE, SAMPLE_CANDIDATE,
    SAMPLE_FINAL, SAMPLE_PIN_FINAL, SAMPLE_PIN_PRE, SAMPLE_PRE,
};
use crate::book::Runner;
use crate::tenant::Edit;

/// The pre-release of the CSI pack that a merge of the pack repository publishes.
const CSI_PRE: &str = "1.66.0-g4f1c2a9b7d30";
/// The final version of the CSI pack, which the pipeline builds again when the graph passes.
const CSI_FINAL: &str = "1.66.0";

/// Run the chapters of a pipeline.
pub(super) fn run(r: &mut Runner) {
    sample_pipeline(r);
    renumber(r);
}

/// The chapter `sample-pipeline`: a merge request publishes a candidate, a merge publishes a
/// pre-release, and the pipeline gives the final number when the tests pass.
fn sample_pipeline(r: &mut Runner) {
    r.chapter("sample-pipeline", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // a merge request of the profile publishes its candidate, and the merge publishes the same
    // content as a pre-release
    r.profile(
        DEV,
        SAMPLE_CANDIDATE,
        "1.1.2-mr12.345",
        "aws-stack@1.1.1",
        RETAIN,
    );
    r.profile(
        DEV,
        SAMPLE_PRE,
        "1.1.2-g0123456789ab",
        "aws-stack@1.1.1",
        RETAIN,
    );
    // a merge of the pack repository publishes a pre-release of the pack, and the bump of the
    // profile pins it
    r.tenant(|t| t.copy_pack_version(DEV, "csi-aws-ebs", "1.65.0", CSI_PRE));
    r.profile(
        DEV,
        SAMPLE_PIN_PRE,
        "1.1.3-g8e2d1c0b9a7f",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "csi-aws-ebs",
            version: CSI_PRE,
        }],
    );
    // a CI cluster runs the pre-release, for the smoke tests
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[SAMPLE_PIN_PRE], &[]));
    the_merge(r);
    r.serve_here(); // state 1: the candidate, the pre-releases, and a CI cluster on one of them
    the_final_numbers(r);
}

/// The part of the chapter `sample-pipeline` that a merge request and a merge reach: the comment
/// of the candidate, and the pre-release that the merge publishes.
fn the_merge(r: &mut Runner) {
    r.quiet("strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json");
    // the progress of the script names a temporary file, so the page keeps standard output alone
    r.step(
        "pipeline-candidate",
        0,
        false,
        "scripts/ci/pr-comment.sh releases/1.1.1/dev.json aws-stack@1.1.2-mr12.345 --dry-run > comment.md",
    );
    r.step(
        "pipeline-candidate",
        0,
        false,
        r"grep -e '^###' -e '^\*\*Result' comment.md",
    );
    r.step(
        "pipeline-pre-release",
        0,
        false,
        "strata same-release aws-stack@1.1.2-mr12.345 aws-stack@1.1.2-g0123456789ab",
    );
    r.quiet("strata snapshot aws-stack@latest -o latest.json");
    r.step(
        "pipeline-latest",
        0,
        false,
        r#"jq -r '.source | "\(.name)@\(.version)"' latest.json"#,
    );
    r.step(
        "pipeline-behind-final",
        0,
        false,
        "strata behind releases/1.1.1/dev.json --no-impact | head -1",
    );
}

/// The part of the chapter `sample-pipeline` after the graph passes: the final versions, the
/// sweep, and the record of a final version.
fn the_final_numbers(r: &mut Runner) {
    // each member of the graph passes: the pipeline builds the final of the pack, and publishes
    // the final of each profile
    r.tenant(|t| t.copy_pack_version(DEV, "csi-aws-ebs", CSI_PRE, CSI_FINAL));
    r.profile(DEV, SAMPLE_FINAL, "1.1.2", "aws-stack@1.1.1", RETAIN);
    r.profile(
        DEV,
        SAMPLE_PIN_FINAL,
        "1.1.3",
        "aws-stack@1.1.1",
        &[Edit::Upgrade {
            pack: "csi-aws-ebs",
            version: CSI_FINAL,
        }],
    );
    the_check_of_the_final(r);
    r.quiet("strata snapshot aws-stack@1.1.3-g8e2d1c0b9a7f -o pre/dev.json");
    r.step(
        "pipeline-behind-pre-release",
        1,
        false,
        "strata behind pre/dev.json --no-impact",
    );
    the_sweep(r);
}

/// The part of the chapter `sample-pipeline` that checks a final version against its pre-release:
/// `same-release`, with and without `--allow renumber`, and the move of a CI cluster.
fn the_check_of_the_final(r: &mut Runner) {
    r.step(
        "pipeline-final-same",
        0,
        false,
        "strata same-release aws-stack@1.1.2-g0123456789ab aws-stack@1.1.2",
    );
    r.step(
        "pipeline-final-moved",
        1,
        false,
        "strata same-release aws-stack@1.1.3-g8e2d1c0b9a7f aws-stack@1.1.3",
    );
    r.step(
        "pipeline-final-renumber",
        0,
        false,
        "strata same-release --allow renumber aws-stack@1.1.3-g8e2d1c0b9a7f aws-stack@1.1.3",
    );
    // the CI cluster runs the pre-release: its move to the final version is a renumber
    r.step(
        "pipeline-move-cluster",
        1,
        false,
        "strata compare cluster:payments aws-stack@1.1.3",
    );
    r.excerpt(
        "pipeline-move-cluster-lines",
        "pipeline-move-cluster",
        r"^(aws-stack|root |~ \[csi|      impact)",
    );
}

/// The part of the chapter `sample-pipeline` after the final versions: the pins that the final
/// commit moves, the sweep, the record of a final version, and the order of the versions.
fn the_sweep(r: &mut Runner) {
    // the page teaches the pin of the pre-release, and not the pins that are current
    r.excerpt(
        "pipeline-behind-pre-release-lines",
        "pipeline-behind-pre-release",
        r"^(\d+ pins|~ \[csi)",
    );
    r.step(
        "pipeline-sweep",
        0,
        false,
        "strata consumers aws-stack@1.1.2-g0123456789ab",
    );
    r.step(
        "pipeline-sweep-held",
        1,
        false,
        "strata consumers aws-stack@1.1.3-g8e2d1c0b9a7f --list-only",
    );
    r.quiet("strata snapshot aws-stack@1.1.2 -o releases/1.1.2/dev.json");
    r.step(
        "pipeline-verify",
        0,
        false,
        "strata verify releases/1.1.2/dev.json",
    );
    r.excerpt("pipeline-verify-lines", "pipeline-verify", r"^(root |OK:)");
    // the order of the versions: a pre-release is below its version, and two pre-releases of one
    // version compare by their text
    r.step("pipeline-versions", 0, false, "strata list profiles");
}

/// The chapter `renumber`: a pipeline publishes a pre-release, and then the same content under the
/// final numbers. 1.2.1 is a final version with one more value.
fn renumber(r: &mut Runner) {
    r.chapter("renumber", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.registries(DEV));
    // the pipeline builds the content of the Kubernetes and the CSI pack of aws-stack@1.1.1 as
    // pre-releases. Each build has its own uid and digest, because its `pack.json` holds its
    // version. The final builds are the versions of aws-stack@1.1.1
    r.tenant(|t| {
        t.copy_pack_version(DEV, "kubernetes", "1.36.2", "1.36.2-rc.1");
        t.copy_pack_version(DEV, "csi-aws-ebs", "1.65.0", "1.65.0-rc.1");
    });
    r.profile(DEV, RENUMBER_PRE, "1.2.0-rc.1", "aws-stack@1.1.1", RC_PINS);
    r.profile(DEV, RENUMBER_FINAL, "1.2.0", "aws-stack@1.1.1", &[]);
    r.profile(DEV, RENUMBER_EDITED, "1.2.1", "aws-stack@1.1.1", RETAIN);
    r.step(
        "renumber-exact",
        1,
        false,
        "strata same-release aws-stack@1.2.0-rc.1 aws-stack@1.2.0",
    );
    r.step(
        "renumber-allow",
        0,
        false,
        "strata same-release --allow renumber aws-stack@1.2.0-rc.1 aws-stack@1.2.0",
    );
    r.step(
        "renumber-json",
        0,
        false,
        "strata same-release --allow renumber aws-stack@1.2.0-rc.1 aws-stack@1.2.0 --json | jq '.pairs[0] | {status, renumbers, archivesNotCompared}'",
    );
    r.step(
        "renumber-compare",
        1,
        false,
        "strata compare aws-stack@1.2.0-rc.1 aws-stack@1.2.0",
    );
    r.step(
        "renumber-compare-json",
        1,
        false,
        "strata compare aws-stack@1.2.0-rc.1 aws-stack@1.2.0 --json | jq -c '.summary.maxImpact, (.packs[] | select(.renumbered) | {key, flags: [.identity, .upstream, .authored], impact})'",
    );
    r.step(
        "renumber-edited",
        1,
        false,
        "strata same-release --allow renumber aws-stack@1.2.0-rc.1 aws-stack@1.2.1",
    );
}
