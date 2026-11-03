//! The routes that `tests/common/palette.rs` writes are the routes that strata builds.
//!
//! A test writes the route that Palette serves, and does not build it with the code under test:
//! see the head of that file. The file then repeats each route that strata builds. This test is
//! the only place that compares the two, so a change of a route in the library fails here, and
//! the file is the one to change.

use crate::common::{
    cloud_account_route, cloud_config_route, cluster_events_route, cluster_route,
    cluster_summary_route, edge_host_route, group_route,
};
use strata::api::{
    cloud_account_path, cloud_config_path, cluster_events_path, cluster_summary_path, subject_path,
};
use strata::snapshot::SubjectKind;

#[test]
fn the_routes_of_the_tests_are_the_routes_that_strata_builds() {
    for (kind, route) in [
        (SubjectKind::Cluster, cluster_route("u1")),
        (SubjectKind::EdgeHost, edge_host_route("u1")),
        (SubjectKind::ClusterGroup, group_route("u1")),
    ] {
        assert_eq!(route, subject_path(kind, "u1"), "{kind:?}");
    }
    assert_eq!(cluster_summary_route("u1"), cluster_summary_path("u1"));
    assert_eq!(cluster_events_route("u1"), cluster_events_path("u1"));
    assert_eq!(
        cloud_config_route("aws", "u1"),
        cloud_config_path("aws", "u1")
    );
    assert_eq!(
        cloud_account_route("aws", "u1"),
        cloud_account_path("aws", "u1")
    );
}
