//! The schema of the tiers of a cluster record: one schema for each tier, by name.
//!
//! The `fields` of a tier hold the document of Palette that the tier reads, so the type of
//! [`ClusterTier::fields`] is a JSON value. A validator that has only the schema needs the keys of
//! each tier. This module names them. It lists no key as required, because a record of an older
//! release holds fewer keys, and a record of a later release holds more.

use schemars::{Schema, SchemaGenerator, json_schema};
use serde_json::{Map, Value, json};

use super::cluster::{ClusterTier, Tier};
use super::policy::{self, FieldType, PolicyKey};
use super::shape;
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
/// a record of an older release. [`policy::all_keys`] lists the keys.
fn policy_fields() -> Value {
    let properties: Map<String, Value> = policy::all_keys()
        .map(|key| (key.key.to_string(), policy_key(key)))
        .collect();
    json!({
        "description": "The settings of the cluster, or the config that a cluster group gives each virtual cluster.",
        "type": "object",
        "properties": properties,
    })
}

/// The schema of the key `key` of the policy fields.
fn policy_key(key: &PolicyKey) -> Value {
    match key.field_type {
        FieldType::Text => typed(key.about, "string"),
        FieldType::Flag => typed(key.about, "boolean"),
        FieldType::Any => open(key.about),
        FieldType::ListOrNull => json!({"description": key.about, "type": ["array", "null"]}),
        FieldType::HostClusters => json!({
            "description": key.about,
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "uid": typed("The uid of the host cluster.", "string"),
                    "endpointConfig": open("The endpoint config for the host cluster, as Palette gives it, or null."),
                },
            },
        }),
    }
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
                "description": "The fields of the cloud part that a person chooses. The keys here are those that every cloud has. Each cloud adds its own keys.",
                "type": "object",
                "properties": keys(shape::CLOUD_FIELDS),
            },
            "pools": {
                "description": "The machine pools, in name order. The keys here are those that every cloud has. Each cloud adds its own keys.",
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": keys(shape::POOL_FIELDS),
                },
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_of_the_policy_tier_names_each_key_of_the_tables_and_describes_it() {
        let schema = policy_fields();
        let properties = schema["properties"].as_object().unwrap();
        let named: Vec<&str> = properties.keys().map(String::as_str).collect();
        let mut table: Vec<&str> = policy::all_keys().map(|k| k.key).collect();
        table.sort_unstable();
        assert_eq!(named, table);
        for (key, property) in properties {
            let about = property["description"].as_str().unwrap_or_default();
            assert!(about.ends_with('.') && about.len() > 10, "{key}: {about}");
        }
    }

    #[test]
    fn a_key_has_the_type_of_its_table_row() {
        let schema = policy_fields();
        for key in policy::all_keys() {
            let property = &schema["properties"][key.key];
            match key.field_type {
                FieldType::Text => assert_eq!(property["type"], "string", "{}", key.key),
                FieldType::Flag => assert_eq!(property["type"], "boolean", "{}", key.key),
                FieldType::ListOrNull => {
                    assert_eq!(property["type"], json!(["array", "null"]), "{}", key.key);
                }
                FieldType::HostClusters => {
                    assert_eq!(property["items"]["type"], "object", "{}", key.key);
                }
                // a key of any type has no `type`
                FieldType::Any => assert!(property.get("type").is_none(), "{}", key.key),
            }
        }
    }

    #[test]
    fn the_descriptions_of_the_shape_tier_say_the_keys_that_every_cloud_has() {
        let schema = shape_fields();
        for part in ["cloud", "pools"] {
            let about = schema["properties"][part]["description"].as_str().unwrap();
            assert!(
                about.contains(
                    "The keys here are those that every cloud has. Each cloud adds its own keys."
                ),
                "{part}: {about}"
            );
        }
    }
}
