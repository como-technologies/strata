//! The text of each value of each closed set in a document that strata writes and reads.
//!
//! Strata reads a value that it does not know as `Open::Unknown`, with no error. So when a
//! release renames or removes a value, a record of an earlier release gives no error. These tests
//! make that change fail. They hold the values of each set, and make sure that serde, the JSON
//! Schema, and `ClosedSet::as_str` give one text for each value. The checks are in
//! `tests/common/closed_set.rs`, and `crates/strata-preflight/tests/closed_set.rs` calls them too.

use std::path::Path;

use crate::common::closed_set::{
    Set, check_all_in_order, check_listed, check_one_text_for_each_value, check_released, set,
};
use crate::common::walk;
use strata::api::Method;
use strata::diff::{ChangeFlag, ImpactLevel, Op, Status, VersionMove};
use strata::model::cloud::Cloud;
use strata::model::cluster_state::ClusterState;
use strata::model::conjunction::Conjunction;
use strata::model::oci_provider::OciProvider;
use strata::model::pack_state::PackState;
use strata::model::pack_type::PackType;
use strata::model::profile_type::ProfileType;
use strata::model::registry_kind::RegistryKind;
use strata::model::scope::ProfileScope;
use strata::ops::{PairStatus, Policy, RecordStatus};
use strata::snapshot::{ClusterRead, Implementation, SubjectKind, Template, Tier, UpstreamSource};

/// Each closed set in a document that strata writes and reads, with the text of each value that
/// a release wrote.
///
/// This list is a copy of the text on purpose. When a release adds a value to a set, add its text
/// here. A release must not rename or remove a value and keep the schema id. If it does, a record
/// of an earlier release reads the value as unknown.
fn sets() -> [Set; 24] {
    [
        set::<Method>(&["GET", "POST"]),
        set::<Implementation>(&["rust", "bash"]),
        set::<Template>(&["published", "draft", "applied", "candidate", "registry"]),
        set::<SubjectKind>(&["cluster", "edgeHost", "clusterGroup"]),
        set::<Tier>(&["applied", "labels", "policy", "running", "shape"]),
        set::<ClusterRead>(&["updatesPending", "pendingRepave"]),
        set::<UpstreamSource>(&["packRef.digest", "packSummary.digest", "none"]),
        set::<ImpactLevel>(&[
            "none",
            "sync",
            "in-place",
            "cluster-wide",
            "may-repave",
            "repave",
        ]),
        set::<ChangeFlag>(&["IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"]),
        set::<Status>(&["unchanged", "changed", "added", "removed"]),
        set::<VersionMove>(&["forward", "back"]),
        set::<Op>(&["changed", "added", "removed"]),
        set::<RecordStatus>(&["unchanged", "drifted", "unverified", "error"]),
        set::<PairStatus>(&[
            "identical",
            "same-content",
            "registry-move",
            "renumber",
            "different",
            "unknown-upstream",
        ]),
        set::<Policy>(&["exact", "registry-move"]),
        set::<PackState>(&["deprecated", "disabled", "deleted", "absent"]),
        set::<OciProvider>(&["pack", "helm", "zarf"]),
        set::<PackType>(&["spectro", "helm", "oci", "manifest"]),
        set::<ProfileType>(&["cluster", "infra", "add-on", "system", "app", "pack"]),
        set::<ProfileScope>(&["project", "tenant", "system"]),
        set::<RegistryKind>(&["pack", "helm", "oci"]),
        set::<Conjunction>(&["and", "or"]),
        set::<Cloud>(&[
            "all",
            "aws",
            "eks",
            "gcp",
            "gke",
            "azure",
            "aks",
            "vsphere",
            "edge-native",
            "maas",
            "apache-cloudstack",
            "baremetal",
            "nested",
            "generic",
            "openstack",
            "custom",
        ]),
        set::<ClusterState>(&[
            "Running",
            "Provisioning",
            "Pending",
            "Importing",
            "Deleting",
            "Deleted",
            "Error",
        ]),
    ]
}

#[test]
fn serde_the_schema_and_as_str_give_one_text_for_each_value_of_a_closed_set() {
    check_one_text_for_each_value(&sets());
}

#[test]
fn the_list_of_a_closed_set_holds_each_value_of_its_schema_in_order() {
    check_all_in_order(&sets());
}

#[test]
fn a_closed_set_holds_each_value_that_a_release_wrote_and_no_other() {
    check_released(&sets());
}

#[test]
fn the_list_of_closed_sets_names_each_closed_set_of_the_crate() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // the trait module implements the trait for a set of its own tests
    let trait_module = src.join("closed_set").join("mod.rs");
    let files: Vec<_> = walk(&src)
        .into_iter()
        .filter(|file| *file != trait_module)
        .collect();
    check_listed(&sets(), &files);
}
