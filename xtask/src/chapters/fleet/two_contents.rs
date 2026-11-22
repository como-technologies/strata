//! The chapter `two-contents`: one version label with two contents. The tenant holds a profile
//! version, and a project holds a copy with the same name and version and another content.

use super::super::DEV_PROJECT;
use super::super::edits::RETAIN;
use super::super::ids::{A111_PROD, A111_TENANT, DEV, PAYMENTS_PROD, PROD};
use crate::book::Runner;

/// The chapter `two-contents`: the release of the tenant, and a copy in prod with its name and its
/// version. A cluster of prod runs the copy, and then moves to the release of the tenant.
pub(super) fn two_contents(r: &mut Runner) {
    r.chapter("two-contents", &[]);
    r.tenant(|t| t.projects(&[(DEV_PROJECT, DEV), ("prod", PROD)]));
    r.tenant_profile(A111_TENANT, "1.1.1", "aws-stack@1.1.1", &[]);
    r.profile_own_copy(
        PROD,
        A111_PROD,
        "1.1.1",
        "aws-stack@1.1.1",
        RETAIN,
        "the two-contents chapter gives the copy of prod a changed value",
    );
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_PROD], &[]));
    r.serve_here(); // state 1: the cluster payments of prod runs the copy
    r.step(
        "two-contents-list",
        0,
        false,
        "strata list profiles --project-name prod",
    );
    let release = "project:dev/aws-stack@1.1.1";
    let copy = format!("project:prod/{A111_PROD}");
    r.step(
        "two-contents-ambiguous",
        2,
        true,
        &format!("strata same-release {release} project:prod/aws-stack@1.1.1"),
    );
    r.step(
        "two-contents-different",
        1,
        false,
        &format!("strata same-release {release} {copy}"),
    );
    r.step(
        "two-contents-compare",
        1,
        false,
        &format!("strata compare {release} {copy}"),
    );
    r.step(
        "two-contents-consumers",
        1,
        false,
        &format!("strata consumers {copy}"),
    );
    // the fix: the team of prod moves payments to the release of the tenant
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A111_TENANT], &[]));
    r.step(
        "two-contents-fixed",
        0,
        false,
        &format!("strata compare {release} project:prod/cluster:payments"),
    );
    r.step(
        "two-contents-free",
        0,
        false,
        &format!("strata consumers {copy}"),
    );
}
