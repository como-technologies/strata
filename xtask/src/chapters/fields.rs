//! The chapter `fields`: documents that carry the fields which a fact reads and no page shows.
//!
//! The contract run compares the type of each field in the answer of a real tenant with the types
//! in the documents of the stand-in. A field that no document holds has no type to compare, so the
//! run reports a difference for each tenant that gives the field. The command of a page prints some
//! fields, such as the note of an audit entry, only in a state that no page builds. A document here
//! holds each such field. The chapter runs no command and writes no output.

use serde_json::json;

use super::ids::DEV;
use crate::book::Runner;
use crate::tenant::{
    AUDITS, CLUSTER_LISTING, app_profile_route, app_tier_manifest_route, app_tiers_route,
    group_route, pack_listing, profile_route, registry_summary_route, variables_route,
};

/// The uids of the objects of this chapter. No other chapter uses them.
const CLUSTER: &str = "64f7000000000000000000a1";
const ACCOUNT: &str = "64f7000000000000000000a2";
const GROUP: &str = "64f7000000000000000000b1";
const APP_PROFILE: &str = "64f7000000000000000000c1";
const PROFILE: &str = "64f7000000000000000000d1";

/// Build the documents of the chapter.
pub(super) fn run(r: &mut Runner) {
    r.chapter("fields", &[]);
    cluster_and_group(r);
    app_profile(r);
    profile(r);
    registries_and_packs(r);
    audit_log(r);
}

/// A cluster that Palette deletes, as the listing gives it, and a cluster group that is active.
fn cluster_and_group(r: &mut Runner) {
    r.answer(
        DEV,
        CLUSTER_LISTING,
        &json!([{
            "metadata": {"name": "ledger", "uid": CLUSTER,
                "creationTimestamp": "2026-09-01T10:00:00.000Z",
                "lastModifiedTimestamp": "2026-09-17T08:00:00.000Z",
                "deletionTimestamp": "2026-09-18T09:00:00.000Z",
                "labels": {"managed-by": "opentofu"}},
            "spec": {"cloudType": "aws", "clusterType": "PureManage",
                "cloudAccountUid": ACCOUNT, "cloudRegion": "us-east-2", "archType": "amd64",
                "location": {"countryCode": "US", "countryName": "United States",
                    "regionCode": "OH", "regionName": "Ohio"},
                "tags": ["env:dev"]},
            "status": {"state": "Deleting"},
        }]),
    );
    r.answer(
        DEV,
        &group_route(GROUP),
        &json!({"metadata": {"name": "vclusters", "uid": GROUP},
            "spec": {"type": "hostCluster", "clusterProfileTemplates": [],
                "clustersConfig": {"endpointType": "LoadBalancer"}},
            "status": {"isActive": true}}),
    );
}

/// An app profile with a container tier, whose manifest has a digest.
fn app_profile(r: &mut Runner) {
    let route = app_profile_route(APP_PROFILE);
    r.answer(
        DEV,
        &route,
        &json!({"metadata": {"name": "jobs", "uid": APP_PROFILE,
                "lastModifiedTimestamp": "2026-09-17T09:00:00.000Z"},
            "spec": {"version": "1.0.0"},
            "status": {"inUseApps": []}}),
    );
    r.answer(
        DEV,
        &app_tiers_route(APP_PROFILE),
        &json!({"spec": {"appTiers": [
            {"metadata": {"name": "worker", "uid": format!("{APP_PROFILE}1")},
                "spec": {"type": "container", "version": "1.0.0",
                    "containerRegistryUid": "64f7000000000000000000e1",
                    "values": "replicas: 1\n", "properties": []}},
            {"metadata": {"name": "settings", "uid": format!("{APP_PROFILE}2")},
                "spec": {"type": "manifest", "version": "1.0.0",
                    "manifests": [{"kind": "manifest", "name": "settings",
                        "uid": format!("{APP_PROFILE}3")}]}},
        ]}}),
    );
    r.answer(
        DEV,
        &app_tier_manifest_route(APP_PROFILE, &format!("{APP_PROFILE}2"), &format!("{APP_PROFILE}3")),
        &json!({"metadata": {"name": "settings", "uid": format!("{APP_PROFILE}3")},
            "spec": {"published": {
                "content": "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: settings\n",
                "digest": "sha256:0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0"}}}),
    );
}

/// A profile of a project, whose manifest pack has a manifest with a uid in the published template
/// and in the draft, and the variable of that profile with every property that Palette gives.
fn profile(r: &mut Runner) {
    let manifests = json!([{"name": "namespace-policy", "uid": format!("{PROFILE}m")}]);
    let pack = json!({"name": "namespace-policy", "layer": "addon", "type": "manifest",
        "tag": "1.0.0", "version": "1.0.0", "manifests": manifests});
    r.answer(
        DEV,
        &profile_route(PROFILE),
        &json!({"metadata": {"name": "platform-policy", "uid": PROFILE,
                "annotations": {"scope": "project", "projectUid": DEV}},
            "spec": {"version": "1.0.0",
                "published": {"type": "add-on", "cloudType": "all", "packs": [pack]},
                "draft": {"type": "add-on", "cloudType": "all", "packs": [pack]}},
            "status": {"isPublished": true, "inUseClusterUids": []}}),
    );
    r.answer(
        DEV,
        &variables_route(PROFILE),
        &json!({"variables": [{
            "name": "owner", "displayName": "Owner",
            "description": "The team that owns the cluster", "defaultValue": "platform",
            "format": "string", "inputType": "text", "regex": "^[a-z]+$",
            "required": true, "hidden": false, "immutable": false, "isSensitive": false,
            "options": [{"label": "Platform", "value": "platform",
                "description": "The platform team"}],
        }]}),
    );
}

/// The summary of the registries of each kind: a registry whose sync stopped has a message, and a
/// registry whose sync is complete has none. A pack version that Spectro Cloud marks `disabled`.
fn registries_and_packs(r: &mut Runner) {
    let recovered = json!({"isSyncSupported": true, "status": "Completed",
        "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-09-17T09:00:00.000Z",
        "syncRecovery": {"syncRecoveryState": "Recovered"}});
    let stopped = json!({"isSyncSupported": true, "status": "InProgress",
        "message": "sync in progress",
        "lastRunTime": "2026-09-17T09:00:00.000Z", "lastSyncedTime": "2026-08-29T00:00:11.032Z",
        "syncRecovery": {"syncRecoveryState": "Recovering", "message": "sync interrupted"}});
    r.answer(
        DEV,
        &registry_summary_route("oci"),
        &json!({"items": [
            {"metadata": {"uid": "64f7000000000000000000e2"}, "spec": {"providerType": "pack"},
                "status": {"sync": recovered}},
            {"metadata": {"uid": "64f7000000000000000000e3"}, "spec": {"providerType": "helm"},
                "status": {"sync": stopped}},
        ]}),
    );
    r.answer(
        DEV,
        &registry_summary_route("pack"),
        &json!({"items": [
            {"metadata": {"uid": "64f7000000000000000000e4"}, "status": {"sync": recovered}},
            {"metadata": {"uid": "64f7000000000000000000e5"}, "status": {"sync": stopped}},
        ]}),
    );
    r.answer(
        DEV,
        &pack_listing("gloo"),
        &json!({"items": [{"metadata": {"uid": "64f7000000000000000000f1"},
                "spec": {"name": "gloo", "version": "1.2.18",
                    "annotations": {"disabled": "true"}}}],
            "listmeta": {"count": 1}}),
    );
}

/// An audit log with an entry of a Palette service, and an entry with a summary and a note.
fn audit_log(r: &mut Runner) {
    r.answer(
        DEV,
        AUDITS,
        &json!({"items": [
            {"metadata": {"uid": "64fa00000000000000000101",
                    "creationTimestamp": "2026-09-18T14:05:11.482Z"},
                "spec": {"actionType": "update",
                    "actor": {"actorType": "service", "serviceName": "system"},
                    "resource": {"kind": "clusterprofile", "name": "platform-policy",
                        "uid": PROFILE}}},
            {"metadata": {"uid": "64fa00000000000000000102",
                    "creationTimestamp": "2026-09-18T14:06:11.482Z"},
                "spec": {"actionType": "update",
                    "actor": {"actorType": "user",
                        "user": {"uid": "64fb00000000000000000001",
                            "firstName": "John", "lastName": "Doe",
                            "emailId": "john.doe@example.com"}},
                    "contentMsg": "version is updated 1.0.1 > 1.0.2",
                    "userMsg": "bumped for the patch window",
                    "resource": {"kind": "clusterprofile", "name": "platform-policy",
                        "uid": PROFILE}}},
        ], "listmeta": {"continue": "", "limit": 100}}),
    );
}
