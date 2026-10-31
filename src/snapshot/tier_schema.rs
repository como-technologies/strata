//! The schema of the tiers of a cluster record: one schema for each tier, by name.
//!
//! The `fields` of a tier hold the document of Palette that the tier reads, so the type of
//! [`ClusterTier::fields`] is a JSON value. A validator that has only the schema needs the keys of
//! each tier. This module names them. It lists no key as required, because a record of an older
//! release holds fewer keys, and a record of a later release holds more.

use schemars::{Schema, SchemaGenerator, json_schema};
use serde_json::{Map, Value, json};

use super::cluster::{ClusterTier, Tier};
use super::{policy, shape};
use crate::closed_set::{ClosedSet, Open};

/// The schema of [`ClusterSource::tiers`](super::ClusterSource::tiers).
///
/// A name is the text of a tier that this release knows, or the text of a tier of a later
/// release. `properties` holds a schema for each tier that a record carries, and
/// `additionalProperties` holds the schema of a tier of a later release. The tier `labels` is a
/// tier of `compare` alone, so it has no entry.
pub(super) fn tiers(generator: &mut SchemaGenerator) -> Schema {
    let tier = |generator: &mut SchemaGenerator, fields: Value| {
        json!({"allOf": [
            generator.subschema_for::<ClusterTier>(),
            {"properties": {"fields": fields}},
        ]})
    };
    let properties: Map<String, Value> = [
        (Tier::Applied, applied()),
        (Tier::Policy, policy_fields()),
        (Tier::Running, running()),
        (Tier::Shape, shape_fields()),
    ]
    .into_iter()
    .map(|(name, fields)| (name.as_str().to_string(), tier(generator, fields)))
    .collect();
    json_schema!({
        "type": "object",
        "propertyNames": generator.subschema_for::<Open<Tier>>(),
        "properties": properties,
        "additionalProperties": generator.subschema_for::<ClusterTier>(),
    })
}

/// A property with `description` and the type `kind`.
fn typed(description: &str, kind: &str) -> Value {
    json!({"description": description, "type": kind})
}

/// A property with `description` and a value of any type.
fn open(description: &str) -> Value {
    json!({ "description": description })
}

/// The fields of the tier `applied`.
fn applied() -> Value {
    json!({
        "description": "The hashes of the spec that Palette applied to the cluster.",
        "type": "object",
        "properties": {
            "spcHash": typed("The hash that Palette computes of the spec it applied. Each cluster has its own.", "string"),
            "spcInfraHash": typed("The hash that Palette computes of the spec it applied. Two clusters from one definition have one.", "string"),
        },
    })
}

/// The fields of the tier `running`: a list of the packs that the cluster reconciled.
fn running() -> Value {
    json!({
        "description": "The pack version that the cluster reconciled, for each pack of a covered profile.",
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": typed("The name of the pack.", "string"),
                "version": typed("The version of the pack that the cluster runs.", "string"),
                "profileUid": typed("The uid of the profile that holds the pack.", "string"),
            },
        },
    })
}

/// The fields of the tier `policy`: those of a cluster, those of a cluster group, and those of
/// a record of an older release.
fn policy_fields() -> Value {
    let list_or_null =
        |description: &str| json!({"description": description, "type": ["array", "null"]});
    let mut properties = Map::new();
    for (key, field) in [
        (
            "osPatch",
            open("The OS patch policy of the cluster, as Palette gives it, or null."),
        ),
        (
            "lifecycle",
            open("Whether a person paused the cluster, as Palette gives it, or null."),
        ),
        (
            "machineHealth",
            open("The machine health checks of the cluster, as Palette gives them, or null."),
        ),
        (
            "timezone",
            typed(
                "The timezone of the scheduled tasks of the cluster.",
                "string",
            ),
        ),
        ("fips", typed("The FIPS mode of the cluster.", "string")),
        (
            policy::RBACS,
            list_or_null("Each role binding of the cluster. Null when strata did not read them."),
        ),
        (
            policy::NAMESPACES,
            list_or_null("Each namespace of the cluster. Null when strata did not read them."),
        ),
        (
            "updateWorkerPoolsInParallel",
            typed(
                "True when Palette updates all the worker pools at one time.",
                "boolean",
            ),
        ),
        (
            "controlPlaneHealthCheckTimeout",
            typed(
                "How long Palette waits for a new control plane node to become ready.",
                "string",
            ),
        ),
        (
            "clusterMetaAttribute",
            typed(
                "The text that a person adds to the cluster as more data.",
                "string",
            ),
        ),
        (
            "applySetting",
            typed(
                "When the cluster applies an update, as Palette gives it.",
                "string",
            ),
        ),
        (
            policy::AGENT_UPGRADES_FORBIDDEN,
            open(
                "The value of the annotation, as Palette gives it. A cluster that has no such annotation has no such field.",
            ),
        ),
        // the config that a cluster group gives each virtual cluster
        (
            "endpointType",
            typed(
                "The endpoint type of the virtual clusters of a cluster group.",
                "string",
            ),
        ),
        (
            "kubernetesDistroType",
            typed(
                "The Kubernetes distribution of the virtual clusters of a cluster group.",
                "string",
            ),
        ),
        (
            "limitConfig",
            open(
                "The resource limits of the virtual clusters of a cluster group, as Palette gives them, or null.",
            ),
        ),
        (
            "values",
            open(
                "The values that a cluster group applies to each virtual cluster, as JSON with sorted keys, or null when they do not parse.",
            ),
        ),
        (
            "valuesHash",
            typed(
                "The hash of the values of a cluster group, after the mask of the passwords.",
                "string",
            ),
        ),
        (
            "valuesParseError",
            typed("Why the values of a cluster group did not parse.", "string"),
        ),
        (
            "hostClusters",
            json!({
                "description": "The host clusters of a cluster group, sorted by uid.",
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "uid": typed("The uid of the host cluster.", "string"),
                        "endpointConfig": open("The endpoint config for the host cluster, as Palette gives it, or null."),
                    },
                },
            }),
        ),
        // the fields of a record of an older release
        (
            policy::LEGACY_RESOURCES,
            open(
                "A record of an older release: the references to the bindings and the namespaces.",
            ),
        ),
        (
            "rbac",
            open("A record of an older release: the role bindings."),
        ),
        (
            policy::LEGACY_AGENT_UPGRADES,
            typed(
                "A record of strata 0.18: `allowed` or `paused`, from the annotation that pauses the upgrades.",
                "string",
            ),
        ),
    ] {
        properties.insert(key.to_string(), field);
    }
    json!({
        "description": "The settings of the cluster, or the config that a cluster group gives each virtual cluster.",
        "type": "object",
        "properties": properties,
    })
}

/// The fields of the tier `shape`: the keys that a pool and the cloud part have on each cloud.
/// The keys that only some clouds have stay open.
fn shape_fields() -> Value {
    // a key with the schema `true` takes a value of any type, as Palette gives it
    let keys = |keys: &[&str]| -> Map<String, Value> {
        keys.iter()
            .map(|key| (key.to_string(), Value::Bool(true)))
            .collect()
    };
    json!({
        "description": "The cloud type, the cloud part, and the machine pools of the cluster.",
        "type": "object",
        "properties": {
            shape::CLOUD_TYPE: typed("The cloud type of the cluster. A record of an older release has none.", "string"),
            "cloud": {
                "description": "The fields of the cloud part that a person chooses. The keys here are those of each cloud. Each cloud adds its own keys.",
                "type": "object",
                "properties": keys(shape::CLOUD_FIELDS),
            },
            "pools": {
                "description": "The machine pools, in name order. The keys here are those of each cloud. Each cloud adds its own keys.",
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": keys(shape::POOL_FIELDS),
                },
            },
        },
    })
}
