//! The comparison of the profile variables of two records, by name.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::leaf::{Leaf, leaf_diff};
use super::zip_by_key;
use crate::snapshot::Variable;

/// Profile variables compared by name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariablesDiff {
    /// True when the hashes differ.
    pub changed: bool,
    /// Variables hash of A.
    pub hash_a: String,
    /// Variables hash of B.
    pub hash_b: String,
    /// Leaf changes with paths like `<variable name>.<field>`
    pub diff: Vec<Leaf>,
    /// The same changes grouped per variable, the form the text reports print
    #[serde(default)]
    pub by_name: VariablesByName,
}

/// Variable changes grouped by variable name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariablesByName {
    /// Variables only in B.
    pub added: Vec<VariableSummary>,
    /// Variables only in A.
    pub removed: Vec<VariableSummary>,
    /// Variables on both sides with different fields.
    pub changed: Vec<VariableChange>,
}

/// One profile variable in brief: its name and the traits a reviewer cares about.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariableSummary {
    /// Variable name.
    pub name: String,
    /// The variable must be set.
    pub required: bool,
    /// The value is a secret (`isSensitive` on the variable).
    pub sensitive: bool,
    /// The variable is not shown in the UI.
    pub hidden: bool,
    /// The value cannot change after the cluster is created.
    pub immutable: bool,
}

impl VariableSummary {
    /// The traits that are set, in the order the reports print them.
    pub fn traits(&self) -> Vec<&'static str> {
        [
            (self.required, "required"),
            (self.sensitive, "sensitive"),
            (self.hidden, "hidden"),
            (self.immutable, "immutable"),
        ]
        .into_iter()
        .filter_map(|(on, word)| on.then_some(word))
        .collect()
    }

    fn of(v: &Variable) -> VariableSummary {
        VariableSummary {
            name: v.name.clone(),
            required: v.required,
            sensitive: v.is_sensitive,
            hidden: v.hidden,
            immutable: v.immutable,
        }
    }
}

/// One variable present on both sides with different fields.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VariableChange {
    /// Variable name.
    pub name: String,
    /// The fields that differ, as leaves with paths relative to the variable, for example `defaultValue`
    pub fields: Vec<Leaf>,
}

/// Each variable of a normalized list, with its name. Two variables of one name are one, the
/// last.
fn named(vars: &[Variable]) -> impl Iterator<Item = (String, &Variable)> {
    vars.iter().map(|v| (v.name.clone(), v))
}

/// The variables of a normalized list as one document, by name: see [`named`].
pub(super) fn by_name(vars: &[Variable]) -> Value {
    Value::Object(named(vars).map(|(n, v)| (n, v.to_value())).collect())
}

/// Group two normalized variable lists by name: added, removed, and changed with the fields that
/// differ. Names are sorted.
pub fn group_variables(a: &[Variable], b: &[Variable]) -> VariablesByName {
    let mut g = VariablesByName::default();
    for (name, x, y) in zip_by_key(named(a), named(b)) {
        match (x, y) {
            (None, Some(y)) => g.added.push(VariableSummary::of(y)),
            (Some(x), None) => g.removed.push(VariableSummary::of(x)),
            (Some(x), Some(y)) => {
                let fields = leaf_diff(&x.to_value(), &y.to_value());
                if !fields.is_empty() {
                    g.changed.push(VariableChange { name, fields });
                }
            }
            (None, None) => {}
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::diff::Segment;

    #[test]
    fn variables_are_grouped_by_name_with_relative_field_paths() {
        let var = |name: &str, default: &str, sensitive: bool| {
            Variable::from_palette(
                &json!({"name": name, "defaultValue": default, "required": true,
                   "isSensitive": sensitive}),
            )
        };
        let a = vec![var("cluster_env", "dev", false), var("gone", "", true)];
        let b = vec![
            var("cluster_env", "staging", false),
            var("db_password", "", true),
        ];
        let g = group_variables(&a, &b);
        assert_eq!(g.added.len(), 1);
        assert_eq!(g.added[0].name, "db_password");
        assert!(g.added[0].required && g.added[0].sensitive && !g.added[0].hidden);
        assert_eq!(g.removed.len(), 1);
        assert_eq!(g.removed[0].name, "gone");
        assert_eq!(g.changed.len(), 1);
        assert_eq!(g.changed[0].name, "cluster_env");
        assert_eq!(g.changed[0].fields.len(), 1);
        assert_eq!(g.changed[0].fields[0].path, "defaultValue");
        assert_eq!(
            g.changed[0].fields[0].segments,
            vec![Segment::Key("defaultValue".into())]
        );
        assert_eq!(g.changed[0].fields[0].old, Some(json!("dev")));
        assert_eq!(g.changed[0].fields[0].new, Some(json!("staging")));
        assert_eq!(
            g.added[0].traits(),
            vec!["required", "sensitive"],
            "the words the report prints in parentheses"
        );
    }
}
