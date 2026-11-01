//! The chapter `automation`: how a job reads the exit code and the JSON of a command.

use super::DEV_ENV;
use super::ids::{A100, A110, A111, DEV};
use crate::book::Runner;

/// The chapter `automation`: the exit codes and the JSON that a job reads.
pub(super) fn run(r: &mut Runner) {
    r.chapter("automation", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.step(
        "auto-max-impact",
        1,
        false,
        "strata compare aws-stack@1.0.0 aws-stack@1.1.0 --json | jq -r .summary.maxImpact",
    );
    r.step(
        "auto-gate",
        1,
        false,
        "strata compare aws-stack@1.1.0 aws-stack@1.1.1 --json > report.json",
    );
    r.step(
        "auto-gate",
        0,
        false,
        r#"jq -e '.summary.maxImpact != "repave"' report.json"#,
    );
    r.step(
        "auto-error",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@9.9.9 --json --quiet",
    );
    r.step(
        "auto-error-text",
        2,
        true,
        "strata compare aws-stack@1.1.1 aws-stack@9.9.9 --quiet",
    );
}
