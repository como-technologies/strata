//! The chapters of the objects of Palette other than a profile: cluster tiers, the shape of a
//! cluster, edge hosts, cluster groups, and app profiles.

use super::DEV_ENV;
use super::ids::{
    A111, ADDONS_DEV, ADDONS_PACK, DEV, DEV_GROUP, LAB, PAYMENTS, SEARCH, SHOP, SHOP_DEV, STORE_1,
    STORE_2, TEAM_A_APPS, TENANT_A, TENANT_B,
};
use crate::book::{EXAMPLE_KEY, Runner};
use crate::common_palette::system_rbacs;
use crate::tenant::rbacs_route;
use serde_json::json;

/// Run the chapters of the objects of Palette other than a profile.
pub(super) fn run(r: &mut Runner) {
    cluster_tiers(r);
    cluster_shape(r);
    edge_hosts(r);
    cluster_groups(r);
    app_profiles(r);
}

/// The chapter `cluster-tiers`: the tiers that Palette reports of a cluster beside its release.
fn cluster_tiers(r: &mut Runner) {
    // More of a cluster than its release: the tiers that Palette reports of the cluster
    r.chapter("cluster-tiers", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111], &[]));
    r.tenant(|t| t.cluster(DEV, SEARCH, "search", &[A111], &[]));
    r.serve_here(); // state 1: two clusters on one release, nothing changed
    r.step(
        "tiers-record-payments",
        0,
        false,
        "strata snapshot cluster:payments --shape -o runs/payments.json",
    );
    r.step(
        "tiers-record-search",
        0,
        false,
        "strata snapshot cluster:search --shape -o runs/search.json",
    );
    r.step(
        "tiers-show",
        0,
        false,
        "jq '.source.cluster | {labels, tiers: (.tiers | map_values(.hash[0:12]))}' runs/payments.json",
    );
    r.step(
        "tiers-policy",
        0,
        false,
        "jq '.source.cluster.tiers.policy.fields' runs/payments.json",
    );
    r.step(
        "tiers-same",
        0,
        false,
        "strata compare runs/payments.json runs/search.json",
    );
    r.excerpt(
        "tiers-same-lines",
        "tiers-same",
        r"^(runs/|root |  variables|  cluster tiers)",
    );
    // a person edits the record: the version of a profile in the cluster is null
    r.quiet(
        "jq '.source.cluster.profiles[0].version = null' runs/payments.json > runs/payments-edited.json",
    );
    r.step(
        "tiers-edited-verify",
        2,
        true,
        "strata verify runs/payments-edited.json",
    );
    cluster_tiers_change(r);
    cluster_tiers_verify(r);
    cluster_tiers_system(r);
    cluster_tiers_forbidden(r);
}

/// The part of the chapter `cluster-tiers` where one cluster changes its shape and its policy.
fn cluster_tiers_change(r: &mut Runner) {
    // search takes a larger worker pool with a taint, and another timezone and label
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/cloudconfig/spec/machinePoolConfig/1/size",
            json!(3),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/cloudconfig/spec/machinePoolConfig/1/taints",
            json!([{"key": "dedicated", "value": "search", "effect": "NoSchedule"}]),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/spec/clusterConfig/timezone",
            json!("America/Chicago"),
        )
    });
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/metadata/labels",
            json!({"managed-by": "opentofu", "team": "search"}),
        )
    });
    // a person adds a user to the role binding of search: the binding keeps its uid
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            SEARCH,
            "/rbacs/items/0/spec/bindings/0/subjects",
            json!([
                {"type": "Group", "name": "platform", "namespace": ""},
                {"type": "User", "name": "fred@example.com", "namespace": ""}
            ]),
        )
    });
    // payments reconciled an older Kubernetes version than its profile declares
    r.tenant(|t| t.cluster_field(DEV, PAYMENTS, "/status/packs/1/version", json!("1.36.1")));
    r.serve_here(); // state 2: search has another shape and policy, and payments is behind
}

/// The part of the chapter `cluster-tiers` that verifies and compares the changed clusters.
fn cluster_tiers_verify(r: &mut Runner) {
    r.step(
        "tiers-verify-search",
        0,
        false,
        "strata verify runs/search.json",
    );
    r.excerpt(
        "tiers-verify-search-lines",
        "tiers-verify-search",
        r"^(root |  cluster tiers|        ~ policy|        \+ policy|OK)",
    );
    r.step(
        "tiers-record-search-again",
        0,
        false,
        "strata snapshot cluster:search --shape -o runs/search.json",
    );
    r.step(
        "tiers-differ",
        0,
        false,
        "strata compare runs/payments.json runs/search.json",
    );
    r.excerpt(
        "tiers-differ-lines",
        "tiers-differ",
        r"^(runs/|root |  variables|  cluster tiers|        [~+-] )",
    );
    r.step(
        "tiers-differ-json",
        0,
        false,
        "strata compare runs/payments.json runs/search.json --json | jq -c '.clusterTiers[] | {tier, same}'",
    );
    r.step(
        "tiers-behind",
        0,
        false,
        "strata compare cluster:payments aws-stack@1.1.1",
    );
    r.excerpt(
        "tiers-behind-lines",
        "tiers-behind",
        r"^(aws-stack|root |note )",
    );
    r.step("tiers-verify", 0, false, "strata verify runs/payments.json");
    r.step(
        "tiers-behind-json",
        0,
        false,
        "strata compare cluster:payments aws-stack@1.1.1 --json | jq -c '.runningBehind'",
    );
}

/// The key of a second user, wilma, in the part of the chapter `cluster-tiers` that reads a role
/// binding of the system.
const WILMA_KEY: &str = "wilma-example-key";

/// The part of the chapter `cluster-tiers` where payments has a role binding of the system, and
/// the keys of two users record it (#838).
fn cluster_tiers_system(r: &mut Runner) {
    // The binding of the group stays as it was. Palette gives the binding of the system to each
    // key only with the subject that is the user of the key.
    r.tenant(|t| t.cluster_field(DEV, PAYMENTS, "/rbacs", system_rbacs()));
    r.tenant(|t| {
        t.key_users(&[
            (EXAMPLE_KEY, "fred@example.com"),
            (WILMA_KEY, "wilma@example.com"),
        ])
    });
    r.serve_here(); // state 3: payments has a role binding of the system
    r.quiet("strata snapshot cluster:payments -o runs/payments-fred.json");
    r.quiet(&format!(
        "PALETTE_API_KEY={WILMA_KEY} strata snapshot cluster:payments -o runs/payments-wilma.json"
    ));
    r.step(
        "tiers-system",
        0,
        false,
        r#"jq -c '.source.cluster.tiers.policy | {hash: .hash[0:12], systemRbacs: [.systemRbacs[] | "\(.role.name): \(.subjects[].name)"]}' runs/payments-fred.json runs/payments-wilma.json"#,
    );
    r.step(
        "tiers-system-compare",
        0,
        false,
        "strata compare runs/payments-fred.json runs/payments-wilma.json",
    );
    r.excerpt(
        "tiers-system-compare-lines",
        "tiers-system-compare",
        r"^(runs/|root |  cluster tiers)",
    );
}

/// The part of the chapter `cluster-tiers` where Palette refuses the role bindings.
fn cluster_tiers_forbidden(r: &mut Runner) {
    // The key loses the permission to read the role bindings of payments. The record holds the
    // error in the policy tier and no value for the field. Verify does not compare the field
    // (#251, #274)
    r.tenant(|t| t.forbid(DEV, &rbacs_route(PAYMENTS)));
    r.serve_here(); // state 4: Palette refuses the role bindings of payments
    r.step(
        "tiers-forbidden-record",
        0,
        true,
        "strata snapshot cluster:payments -o runs/payments-now.json",
    );
    r.step(
        "tiers-forbidden-errors",
        0,
        false,
        "jq '.source.cluster.tiers.policy | {rbacsInFields: (.fields | has(\"rbacs\")), errors}' \
         runs/payments-now.json",
    );
    r.step(
        "tiers-forbidden-verify",
        0,
        false,
        "strata verify runs/payments.json",
    );
}

/// The chapter `cluster-shape`: the fields of the cloud type of a vSphere cluster.
fn cluster_shape(r: &mut Runner) {
    // The shape tier reads the fields of the cloud type of the cluster. The cluster lab runs on
    // vSphere. Its cloud config has the model of the API reference of Palette.
    r.chapter("cluster-shape", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.cluster(DEV, LAB, "lab", &[A111], &[]));
    r.tenant(|t| t.cloud(DEV, LAB, "vsphere", vsphere_spec()));
    r.serve_here();
    r.step(
        "shape-vsphere-record",
        0,
        false,
        "strata snapshot cluster:lab --shape -o runs/lab.json",
    );
    r.step(
        "shape-vsphere",
        0,
        false,
        "jq '.source.cluster.tiers.shape.fields | {cloudType, cloud, pool: .pools[0]}' runs/lab.json",
    );
}

/// The chapter `edge-hosts`: an add-on profile bound to two edge hosts and one cluster.
fn edge_hosts(r: &mut Runner) {
    // Edge hosts: an add-on profile bound to two edge hosts and applied to one cluster
    r.chapter("edge-hosts", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.profile(DEV, A111, "1.1.1", "aws-stack@1.1.1", &[]);
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "baseline", "1.0.0"));
    r.tenant(|t| t.cluster(DEV, PAYMENTS, "payments", &[A111, ADDONS_DEV], &[]));
    r.tenant(|t| t.edge_host(DEV, STORE_1, "store-1", &[ADDONS_DEV]));
    r.tenant(|t| t.edge_host(DEV, STORE_2, "store-2", &[ADDONS_DEV]));
    r.serve_here(); // state 1: one cluster and two edge hosts on platform-addons@1.0.0
    r.step(
        "edge-consumers",
        1,
        false,
        "strata consumers platform-addons@1.0.0",
    );
    r.step(
        "edge-record",
        0,
        true,
        "strata snapshot edgehost:store-1 -o runs/store-1.json",
    );
    r.step(
        "edge-record-cluster",
        0,
        true,
        "strata snapshot cluster:payments/platform-addons -o runs/payments.json",
    );
    r.step(
        "edge-compare",
        0,
        false,
        "strata compare platform-addons@1.0.0 runs/store-1.json",
    );
    // an edit in place of the manifest: the edge host and the cluster each keep their own copy
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "restricted", "1.0.0"));
    r.serve_here(); // state 2: the manifest of platform-addons@1.0.0 is edited in place
    r.step("edge-verify", 0, false, "strata verify 'runs/*.json'");
    r.step(
        "edge-verify-profile",
        1,
        false,
        "strata compare runs/store-1.json platform-addons@1.0.0",
    );
}

/// The chapter `cluster-groups`: a cluster group that pins an add-on profile for its virtual
/// clusters.
fn cluster_groups(r: &mut Runner) {
    // A cluster group pins an add-on profile for its virtual clusters
    r.chapter("cluster-groups", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.addons(DEV, ADDONS_DEV, ADDONS_PACK, "baseline", "1.0.0"));
    r.tenant(|t| {
        t.addons_named(
            DEV,
            TEAM_A_APPS,
            "team-a-apps",
            "64e2000000000000000000d3",
            "restricted",
            "1.0.0",
        )
    });
    r.tenant(|t| t.cluster(DEV, TENANT_A, "tenant-a", &[ADDONS_DEV, TEAM_A_APPS], &[]));
    r.tenant(|t| t.cluster(DEV, TENANT_B, "tenant-b", &[ADDONS_DEV], &[]));
    r.tenant(|t| {
        t.cluster_group(
            DEV,
            DEV_GROUP,
            "dev-group",
            &[ADDONS_DEV],
            &[TENANT_A, TENANT_B],
        )
    });
    r.serve_here(); // state 1: the group pins platform-addons@1.0.0, and two members run it
    r.step(
        "group-record",
        0,
        true,
        "strata snapshot clustergroup:dev-group -o runs/dev-group.json",
    );
    r.step(
        "group-show",
        0,
        false,
        "jq -c '.source.cluster | {kind, profiles: [.profiles[].name], members: [.members[].name]}' runs/dev-group.json",
    );
    r.step(
        "group-compare",
        0,
        false,
        "strata compare runs/dev-group.json cluster:tenant-a",
    );
    cluster_groups_virtual(r);
    cluster_groups_policy(r);
}

/// The part of the chapter `cluster-groups` for a virtual cluster, and the listings that show it.
fn cluster_groups_virtual(r: &mut Runner) {
    // tenant-b is a virtual cluster, and the consumer list of the profile does not name it
    r.tenant(|t| t.cluster_field(DEV, TENANT_B, "/spec/cloudType", json!("nested")));
    r.tenant(|t| t.drop_consumer(DEV, ADDONS_DEV, TENANT_B));
    r.step(
        "group-consumers",
        1,
        false,
        "strata consumers platform-addons@1.0.0",
    );
    // the cluster listing omits the virtual cluster, so list asks the search for it (#116)
    r.step("group-list", 0, false, "strata list clusters");
    r.step(
        "group-list-json",
        0,
        false,
        r#"strata list clusters --json | jq -c '.includesVirtual, (.clusters[] | select(.virtual) | {name, virtual})'"#,
    );
    r.tenant(|t| {
        t.cluster_field(
            DEV,
            TENANT_B,
            "/metadata/labels",
            json!({"managed-by": "opentofu", "team": "b"}),
        )
    });
    r.step(
        "group-list-tag",
        0,
        false,
        "strata list clusters --tag team=b",
    );
    r.step(
        "group-member-by-name",
        0,
        false,
        "strata compare clustergroup:dev-group cluster:tenant-b",
    );
    r.step(
        "group-policy",
        0,
        false,
        "jq -c '.source.cluster.tiers.policy.fields | {endpointType, kubernetesDistroType, limitConfig, hostClusters: [.hostClusters[].uid]}' runs/dev-group.json",
    );
    r.step(
        "group-policy-values",
        0,
        false,
        "jq -c '.source.cluster.tiers.policy.fields.values' runs/dev-group.json",
    );
}

/// The part of the chapter `cluster-groups` where the group gives each virtual cluster less CPU.
fn cluster_groups_policy(r: &mut Runner) {
    // the group gives each virtual cluster 4 cores in place of 6
    r.tenant(|t| {
        t.cluster_group_field(
            DEV,
            DEV_GROUP,
            "/spec/clustersConfig/limitConfig",
            json!({"cpu": 4, "cpuMilliCore": 4000, "memory": 12288, "memoryMiB": 12288,
            "overSubscription": 120, "storageGiB": 10}),
        )
    });
    r.serve_here(); // state 2: the group gives each virtual cluster less CPU
    r.step(
        "group-policy-verify",
        0,
        false,
        "strata verify runs/dev-group.json",
    );
    r.step(
        "group-policy-compare",
        0,
        false,
        "strata compare runs/dev-group.json clustergroup:dev-group",
    );
    cluster_groups_template(r);
}

/// The part of the chapter `cluster-groups` where the values hold a template with no quotes. Such
/// values have no canonical form, and a new password changes no tier.
fn cluster_groups_template(r: &mut Runner) {
    let values = |password: &str| {
        json!(format!(
            "database:\n  password: {password}\nhost: {{{{ .spectro.var.host }}}}\n"
        ))
    };
    r.tenant(|t| {
        t.cluster_group_field(
            DEV,
            DEV_GROUP,
            "/spec/clustersConfig/values",
            values("s3cr3t"),
        )
    });
    r.quiet("strata snapshot clustergroup:dev-group -o runs/dev-group-template.json");
    r.step(
        "group-template-values",
        0,
        false,
        "jq -c '.source.cluster.tiers.policy.fields | {values, valuesHash}' runs/dev-group-template.json",
    );
    // a person gives the group a new password, and the record still verifies
    r.tenant(|t| {
        t.cluster_group_field(
            DEV,
            DEV_GROUP,
            "/spec/clustersConfig/values",
            values("an0ther"),
        )
    });
    r.step(
        "group-template-verify",
        0,
        false,
        "strata verify runs/dev-group-template.json",
    );
}

/// The chapter `app-profiles`: an app profile, its plain updates, and its deployment.
fn app_profiles(r: &mut Runner) {
    // An app profile, and a plain update of it
    r.chapter("app-profiles", DEV_ENV);
    r.tenant(|t| t.projects(&[("dev", DEV)]));
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "2", 0));
    r.serve_here(); // state 1: shop@1.0.0
    r.step(
        "app-record",
        0,
        true,
        "strata snapshot appprofile:shop@1.0.0 -o runs/shop.json",
    );
    r.step(
        "app-show",
        0,
        false,
        "jq -c '.packs[] | {key, type, tag, params, upstream: .hash.upstreamSource}' runs/shop.json",
    );
    // a tier has no digest, so verify cannot vouch for the chart of the helm tier
    r.step(
        "app-verify-unverified",
        1,
        false,
        "strata verify runs/shop.json",
    );
    r.step(
        "app-verify-ok",
        0,
        false,
        "strata verify runs/shop.json --allow-no-digest",
    );
    // no app deployment uses the app profile yet
    r.step(
        "app-consumers-free",
        0,
        false,
        "strata consumers appprofile:shop@1.0.0",
    );
    // a plain update: the replicas of the web tier, with the same uid and version
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "3", 0));
    r.serve_here(); // state 2: the web tier of shop@1.0.0 has another replica count
    r.step(
        "app-verify",
        1,
        false,
        "strata verify runs/shop.json --allow-no-digest",
    );
    // a plain update of the install order: web installs after config
    r.tenant(|t| t.app_profile(DEV, SHOP, "1.0.0", "3", 1));
    r.serve_here(); // state 3: the web tier of shop@1.0.0 has the install order 1
    r.step(
        "app-order-compare",
        1,
        false,
        "strata compare runs/shop.json appprofile:shop@1.0.0",
    );
    // a team deploys the app profile, so Palette refuses its delete
    r.tenant(|t| t.app_deployment(DEV, SHOP_DEV, "shop-dev", SHOP));
    r.serve_here(); // state 4: the app deployment shop-dev uses shop@1.0.0
    r.step(
        "app-consumers",
        1,
        false,
        "strata consumers appprofile:shop@1.0.0",
    );
    r.step(
        "app-consumers-json",
        1,
        false,
        "strata consumers runs/shop.json --json | jq -c '.consumers[]'",
    );
}

/// The spec of the cloud config of a vSphere cluster, in the model of the API reference of
/// Palette. It holds a placement for the cluster and for each pool, with the uid that Palette gives
/// each one. It also holds the network of the control plane.
fn vsphere_spec() -> serde_json::Value {
    let network = json!({"networkName": "VM Network", "staticIp": true});
    let placement = |uid: &str| {
        json!({"uid": uid, "cluster": "compute-1", "datastore": "ds-fast",
            "resourcePool": "rp-lab", "folder": "spectro/lab", "network": network})
    };
    json!({
        "clusterConfig": {
            "placement": {"uid": "64f9000000000000000000a1", "datacenter": "dc-east",
                "folder": "spectro/lab", "network": network},
            "ntpServers": ["pool.ntp.org"], "staticIp": true,
            "sshKeys": ["ssh-ed25519 AAAAC3Nza lab"],
            "controlPlaneEndpoint": {"type": "IP", "host": "10.10.0.10"}},
        "machinePoolConfig": [
            {"name": "cp-pool", "isControlPlane": true, "size": 1,
                "instanceType": {"diskGiB": 60, "memoryMiB": 8192, "numCPUs": 4},
                "placements": [placement("64f9000000000000000000a2")],
                "machinePoolProperties": {"archType": "amd64"}, "labels": ["control-plane"]},
            {"name": "worker-pool", "isControlPlane": false, "size": 2,
                "instanceType": {"diskGiB": 80, "memoryMiB": 16384, "numCPUs": 8},
                "placements": [placement("64f9000000000000000000a3")],
                "machinePoolProperties": {"archType": "amd64"}, "labels": ["worker"]},
        ]
    })
}
