//! The chapters of the retire of a version: the consumers of a profile of a project, and of a
//! profile of the tenant that each project can use.

use super::super::DEV_ENV;
use super::super::ids::{
    A100, A110, A110_PROD, A111, A200_TENANT, AWS_STANDARD, CHECKOUT_PROD, DEV, PAYMENTS,
    PAYMENTS_PROD, PROD, SEARCH, STAGING,
};
use crate::book::Runner;

/// The chapter `retire`: the consumers of a version that a team retires.
pub(super) fn retire(r: &mut Runner) {
    r.chapter("retire", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A100, "1.0.0", "aws-stack@1.0.0", &[]);
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A110], &[]));
    r.step(
        "retire-in-use",
        1,
        false,
        "strata consumers aws-stack@1.1.0",
    );
    r.step(
        "consumers-list-only",
        1,
        false,
        "strata consumers aws-stack@1.1.0 --list-only",
    );
    r.step("retire-free", 0, false, "strata consumers aws-stack@1.0.0");
    r.step(
        "retire-gate",
        0,
        false,
        r#"strata consumers aws-stack@1.0.0 && echo "no cluster uses aws-stack@1.0.0""#,
    );
    r.step("retire-json", 1, false, r#"strata consumers aws-stack@1.1.0 --json | jq -r '.consumers[] | "\(.name) \(.state) \(.version)"'"#);
    // no cluster runs 1.1.1, and a cluster template links it: Palette refuses the delete
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster_template(DEV, AWS_STANDARD, "aws-standard", &[A111]));
    r.step(
        "retire-template",
        1,
        false,
        "strata consumers aws-stack@1.1.1",
    );
}

/// The chapter `retire-tenant`: a profile of the tenant, which each project can use, and a key
/// that cannot read one of the projects.
pub(super) fn retire_tenant(r: &mut Runner) {
    r.chapter("retire-tenant", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV), ("prod", PROD), ("staging", STAGING)]));
    r.profile(DEV, A110, "1.1.0", "aws-stack@1.1.0", &[]);
    r.profile(PROD, A110_PROD, "1.1.0", "aws-stack@1.1.0", &[]);
    r.tenant_profile(A200_TENANT, "2.0.0", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A110], &[]));
    r.tenant(|t| t.cluster(PROD, PAYMENTS_PROD, "payments", &[A110_PROD], &[]));
    r.serve_here(); // state 1: the key reads each project, and no cluster runs 2.0.0
    r.step(
        "retire-tenant",
        0,
        false,
        "strata consumers aws-stack@2.0.0",
    );
    // the role of the key gives no listing in staging
    r.tenant(|t| t.deny_project(STAGING, "staging"));
    r.step(
        "retire-tenant-denied",
        2,
        false,
        "strata consumers aws-stack@2.0.0",
    );
    r.step(
        "retire-tenant-json",
        2,
        false,
        r#"strata consumers aws-stack@2.0.0 --json | jq -c '.profile, (.projectsRead | {read, unread: [.unread[] | {name, denied}]})'"#,
    );
    r.serve_here(); // state 2: the key cannot read staging
    // a cluster of prod runs 2.0.0, and the consumer list of the tenant profile names it
    r.tenant(|t| t.cluster(PROD, CHECKOUT_PROD, "checkout", &[A200_TENANT], &[]));
    r.tenant(|t| t.deny_project(STAGING, "staging"));
    r.step(
        "retire-tenant-in-use",
        1,
        false,
        "strata consumers aws-stack@2.0.0",
    );
}
