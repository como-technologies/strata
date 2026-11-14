//! The chapters of the first pages of the book: the help text, and the quick start.

use super::ids::{A100, A110, A111, DEV, PROD};
use super::{DEV_ENV, payments_addons, payments_cluster};
use crate::book::Runner;

/// Run the chapters of the first pages.
pub(super) fn run(r: &mut Runner) {
    help(r);
    quickstart(r);
}

/// The chapter `help`: the help text of the program and of each command.
fn help(r: &mut Runner) {
    r.chapter("help", &[]);
    r.step("help", 0, false, "strata --help");
    for c in [
        "list",
        "packs",
        "snapshot",
        "candidate",
        "compare",
        "same-release",
        "verify",
        "consumers",
        "behind",
        "report",
        "schema",
        "preflight",
    ] {
        r.step(
            &format!("help-{c}"),
            0,
            false,
            &format!("strata {c} --help"),
        );
    }
    // the page of compare states its exit codes from the help, so the two cannot differ (#624)
    r.excerpt(
        "help-compare-exit",
        "help-compare",
        r"^(Exit codes:|  [0-9]  |.* do not change the exit code)",
    );
}

/// The chapter `quickstart`: the first compare, snapshot, and verify of a new reader.
fn quickstart(r: &mut Runner) {
    r.chapter("quickstart", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    payments_addons(r);
    // the cluster runs the release and three add-on profiles, and the compare answers for the
    // release (#615)
    payments_cluster(r, A110);
    r.step("quickstart-offline", 1, false, "strata compare examples/profiles/aws-stack@1.1.0.json examples/profiles/aws-stack@1.1.1.json");
    r.step("quickstart-list", 0, false, "strata list");
    r.step(
        "quickstart-snapshot",
        0,
        true,
        "strata snapshot aws-stack@1.1.0 -o aws-stack-1.1.0.json",
    );
    r.step(
        "quickstart-verify",
        0,
        false,
        "strata verify aws-stack-1.1.0.json",
    );
    r.step(
        "quickstart-cluster",
        0,
        false,
        "strata compare aws-stack-1.1.0.json cluster:payments",
    );
    r.step(
        "quickstart-versions",
        1,
        false,
        "strata compare aws-stack@1.1.0 aws-stack@1.1.1",
    );
}
