//! The chapter `projects`: one tenant with a project for each environment, and each question of
//! the table of the introduction across the two projects.

use super::super::DEV_PROJECT;
use super::super::edits::RETAIN;
use super::super::ids::{
    A110, A110_PROD, A111, A111_PROD, DELETED_PROJECT, DEV, PAYMENTS, PAYMENTS_PROD, PROD,
};
use crate::book::Runner;

/// The chapter `projects`: one tenant with a project for each environment. One run reads the
/// two projects, for each question of the table of the introduction (#486).
pub(super) fn projects(r: &mut Runner) {
    r.chapter("projects", &[]);
    r.tenant(|t| t.projects(&[(DEV_PROJECT, DEV), ("prod", PROD)]));
    r.tenant(|t| t.registries(DEV));
    r.tenant(|t| t.registries(PROD));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    // 1.1.0 reached prod by a promotion, and somebody built 1.1.1 by hand in prod
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the projects chapter gives prod a changed value",
    );
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A110_PROD], &[]));
    let sides = "--project-name-a dev --project-name-b prod";
    r.step(
        "projects-same",
        0,
        false,
        &format!("strata same-release aws-stack@1.1.0 aws-stack@1.1.0 {sides}"),
    );
    r.step(
        "projects-different",
        1,
        false,
        &format!("strata same-release aws-stack@1.1.1 aws-stack@1.1.1 {sides}"),
    );
    r.step(
        "projects-compare",
        1,
        false,
        &format!("strata compare aws-stack@1.1.1 aws-stack@1.1.1 {sides}"),
    );
    projects_refs(r);
    projects_records(r);
}

/// The part of the chapter `projects` where each ref names its project (#703).
fn projects_refs(r: &mut Runner) {
    r.step(
        "projects-ref-same",
        0,
        false,
        "strata same-release project:dev/aws-stack@1.1.0 project:prod/aws-stack@1.1.0",
    );
    r.step(
        "projects-ref-cluster",
        1,
        false,
        "strata compare project:dev/aws-stack@1.1.1 project:prod/cluster:payments",
    );
    r.step(
        "projects-ref-missing",
        2,
        true,
        "strata snapshot project:staging/aws-stack@1.1.0",
    );
    r.step(
        "projects-ref-missing-uid",
        2,
        true,
        &format!("strata snapshot project:{DELETED_PROJECT}/aws-stack@1.1.0"),
    );
    r.step(
        "projects-ref-no-version",
        2,
        true,
        "strata snapshot project:prod/aws-stack",
    );
    r.step(
        "projects-ref-file",
        2,
        true,
        "strata same-release project:dev/releases/1.1.0/dev.json project:prod/aws-stack@1.1.0",
    );
}

/// The part of the chapter `projects` that records each project, and asks about the records.
fn projects_records(r: &mut Runner) {
    for project in [DEV_PROJECT, "prod"] {
        r.step(
            "projects-record",
            0,
            true,
            &format!(
                "strata snapshot aws-stack@1.1.0 --project-name {project} -o releases/1.1.0/{project}.json"
            ),
        );
    }
    r.step(
        "projects-verify",
        0,
        false,
        "strata verify 'releases/*/*.json'",
    );
    r.step(
        "projects-cluster",
        0,
        false,
        "strata compare releases/1.1.0/prod.json cluster:payments --project-name prod",
    );
    r.step(
        "projects-behind",
        1,
        false,
        "strata behind 'releases/*/*.json' --no-impact",
    );
    r.step(
        "projects-consumers",
        1,
        true,
        "strata consumers aws-stack@1.1.0 --project-name prod",
    );
    r.step(
        "projects-list",
        0,
        false,
        "strata list clusters --project-name prod",
    );
}
