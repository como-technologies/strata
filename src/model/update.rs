//! The rules of Palette for the update of a cluster that no validate route checks, and the checks
//! that strata makes for them.
//!
//! `behind` and `compare --validate` ask a validate route about a move, and the route checks the
//! packs. Palette checks two more rules only at the update. One update moves the Kubernetes pack
//! one minor version at most. Each required variable has a value or a default value. No route
//! answers them before the update, so strata checks them, and names Palette as the owner of each
//! rule. "The gaps that strata fills" in the book lists them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::codes::{INVALID_VARIABLE_CODE, K8S_UPGRADE_NOT_SUPPORTED_CODE};
use crate::model::layer::Layer;
use crate::reference::version;
use crate::snapshot::{Pack, Snapshot, Variable};
use crate::text::{and_list, plural};

/// The most minor versions that one update moves the Kubernetes pack of a cluster.
///
/// Palette refuses a move of more, or to a higher major version, with
/// [`K8S_UPGRADE_NOT_SUPPORTED_CODE`].
const KUBERNETES_MINOR_STEP: u64 = 1;

/// What strata checks about a move beside the answer of a validate route: the rules that Palette
/// checks only at the update.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChecks {
    /// The required variables with no default value that the move adds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_variables: Option<RequiredVariables>,
    /// The move of the Kubernetes pack, when it skips a minor version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_minor: Option<MinorSkip>,
}

/// The required variables with no default value that a move adds.
///
/// No validate route checks a variable. Palette refuses an update that gives such a variable no
/// value, with [`INVALID_VARIABLE_CODE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequiredVariables {
    /// The code of the refusal of Palette at the update: [`INVALID_VARIABLE_CODE`].
    pub code: String,
    /// The name of each variable, sorted.
    pub names: Vec<String>,
}

impl RequiredVariables {
    /// The variables of `to` that `from` does not hold and that need a value, or None when there
    /// is none.
    ///
    /// A variable needs a value when it is required, and has no default value and no options.
    /// Palette takes the default value when the update gives no value. A variable with options
    /// can also take its default option, and a record does not hold which option that is. So such
    /// a variable does not count.
    fn added(from: &[Variable], to: &[Variable]) -> Option<RequiredVariables> {
        let held = |name: &str| from.iter().any(|v| v.name == name);
        let mut names: Vec<String> = to
            .iter()
            .filter(|v| v.required && v.default_value.is_empty() && v.options.is_empty())
            .filter(|v| !held(&v.name))
            .map(|v| v.name.clone())
            .collect();
        names.sort();
        names.dedup();
        (!names.is_empty()).then(|| RequiredVariables {
            code: INVALID_VARIABLE_CODE.to_string(),
            names,
        })
    }

    /// `the move adds the required variable a with no default value: give it a value at the
    /// update, or Palette refuses the update with InvalidVariable`.
    pub(crate) fn line(&self) -> String {
        let n = self.names.len();
        format!(
            "the move adds the required {} {} with no default value: give {} a value at the \
             update, or Palette refuses the update with {}",
            plural(n, "variable", "variables"),
            and_list(&self.names),
            plural(n, "it", "each one"),
            self.code
        )
    }
}

/// A move of the Kubernetes pack of more than one minor version, or to a higher major version.
///
/// Palette refuses the update with [`K8S_UPGRADE_NOT_SUPPORTED_CODE`]. It compares the version
/// that each cluster runs with the version of the move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MinorSkip {
    /// The code of the refusal of Palette at the update: [`K8S_UPGRADE_NOT_SUPPORTED_CODE`].
    pub code: String,
    /// The name of the pack of the `k8s` layer.
    pub pack: String,
    /// The version that the cluster runs, or that the record pins.
    pub from: String,
    /// The version of the move.
    pub to: String,
    /// The minor version that Palette allows next, as `<major>.<minor>`: `1.35` for a move from
    /// `1.34.9`.
    pub next_minor: String,
    /// The newest version of `nextMinor` that the registry offers. Only `behind` reads the
    /// registry, for a pin. Absent when the registry offers none, and for each other move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
}

impl MinorSkip {
    /// The skip of the move of the pack `pack` of the layer `layer` from the version `from` to
    /// `to`. None for a pack of another layer, and for a move that skips no minor version.
    pub fn of(layer: &str, pack: &str, from: &str, to: &str) -> Option<MinorSkip> {
        if Layer::parse(layer) != Layer::K8s
            || !version::minor_above(from, to, KUBERNETES_MINOR_STEP)
        {
            return None;
        }
        Some(MinorSkip {
            code: K8S_UPGRADE_NOT_SUPPORTED_CODE.to_string(),
            pack: pack.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            next_minor: version::minor_line_above(from, KUBERNETES_MINOR_STEP)?,
            next_step: None,
        })
    }

    /// The skip with `next_step`: the newest of `newer` on the minor version that Palette allows
    /// next. `newer` holds the newer versions of the pin, newest first.
    pub(crate) fn with_next_step<'v>(mut self, newer: impl IntoIterator<Item = &'v str>) -> Self {
        self.next_step = newer
            .into_iter()
            .find(|v| version::line(v, 2).as_deref() == Some(self.next_minor.as_str()))
            .map(str::to_string);
        self
    }

    /// `kubernetes 1.34.9 -> 1.36.2 skips a minor version: Palette refuses the update with
    /// K8sUpgradeNotSupported; move to 1.35.6 first`. With no version of the next minor version,
    /// the line ends with `move to a version of 1.35 first`.
    pub fn line(&self) -> String {
        let next = match &self.next_step {
            Some(version) => version.clone(),
            None => format!("a version of {}", self.next_minor),
        };
        format!(
            "{} {} -> {} skips a minor version: Palette refuses the update with {}; move to {next} \
             first",
            self.pack, self.from, self.to, self.code
        )
    }

    /// The skip of the Kubernetes move from the packs `from` to the packs `to`: the pack of the
    /// `k8s` layer on each side. None when a side has no such pack, or the move skips no minor
    /// version.
    fn between(from: &[Pack], to: &[Pack]) -> Option<MinorSkip> {
        let kubernetes = |packs: &[Pack]| {
            packs
                .iter()
                .position(|p| Layer::parse(&p.layer) == Layer::K8s)
        };
        let (old, new) = (&from[kubernetes(from)?], &to[kubernetes(to)?]);
        MinorSkip::of(&new.layer, &new.name, &old.version, &new.version)
    }
}

impl UpdateChecks {
    /// The checks of a move from the record `from` to the record `to`: the required variables
    /// that `to` adds, and the Kubernetes move.
    ///
    /// A cluster that runs `from` has a value of each variable of `from`, so only a variable that
    /// `from` does not hold counts. The Kubernetes pack is the pack of the `k8s` layer on each
    /// side. A side with no such pack, such as an add-on profile, moves no Kubernetes pack.
    pub(crate) fn between(from: &Snapshot, to: &Snapshot) -> UpdateChecks {
        UpdateChecks {
            required_variables: RequiredVariables::added(&from.variables, &to.variables),
            skipped_minor: MinorSkip::between(&from.packs, &to.packs),
        }
    }

    /// True when strata finds nothing that the update refuses or needs.
    pub fn is_empty(&self) -> bool {
        self.required_variables.is_none() && self.skipped_minor.is_none()
    }

    /// One line for each check that finds something: the required variables, then the skip of a
    /// minor version. See [`RequiredVariables::line`] and [`MinorSkip::line`].
    pub(crate) fn lines(&self) -> Vec<String> {
        self.required_variables
            .as_ref()
            .map(RequiredVariables::line)
            .into_iter()
            .chain(self.skipped_minor.as_ref().map(MinorSkip::line))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pack(layer: &str, name: &str, version: &str) -> Pack {
        Pack::from_api(
            &json!({"name": name, "layer": layer, "tag": version, "version": version}),
            &[],
        )
    }

    fn variable(fields: serde_json::Value) -> Variable {
        serde_json::from_value(fields).unwrap()
    }

    #[test]
    fn a_kubernetes_move_of_two_minor_versions_skips_one_and_names_the_next_minor_version() {
        let skip = MinorSkip::of("k8s", "kubernetes-eks", "1.31.14", "1.37.0").unwrap();
        assert_eq!(skip.code, K8S_UPGRADE_NOT_SUPPORTED_CODE);
        assert_eq!(skip.next_minor, "1.32");
        assert_eq!(skip.next_step, None);
        assert_eq!(
            skip.line(),
            format!(
                "kubernetes-eks 1.31.14 -> 1.37.0 skips a minor version: Palette refuses the \
                 update with {K8S_UPGRADE_NOT_SUPPORTED_CODE}; move to a version of 1.32 first"
            )
        );
        // the newest version of the next minor version, from the newer versions newest first
        let skip = skip.with_next_step(["1.37.0", "1.33.1", "1.32.13", "1.32.10", "1.31.15"]);
        assert_eq!(skip.next_step.as_deref(), Some("1.32.13"));
        assert!(
            skip.line().ends_with("; move to 1.32.13 first"),
            "{}",
            skip.line()
        );

        // one minor version, a pack of another layer, and a version that Palette does not read
        assert_eq!(MinorSkip::of("k8s", "kubernetes", "1.35.3", "1.36.2"), None);
        assert_eq!(
            MinorSkip::of("csi", "csi-aws-ebs", "1.63.1", "1.65.0"),
            None
        );
        assert_eq!(MinorSkip::of("k8s", "kubernetes", "1.29.x", "1.31.2"), None);
    }

    #[test]
    fn a_move_adds_each_required_variable_with_no_default_value_that_the_cluster_does_not_hold() {
        let from = [variable(json!({"name": "region", "required": true}))];
        let to = [
            variable(json!({"name": "region", "required": true})),
            variable(json!({"name": "owner", "required": true})),
            variable(json!({"name": "cost_center", "required": true})),
            variable(json!({"name": "tier", "required": true, "defaultValue": "gold"})),
            variable(json!({"name": "note"})),
            variable(
                json!({"name": "size", "required": true, "inputType": "dropdown",
                "options": [{"label": "small", "value": "s"}]}),
            ),
        ];
        let added = RequiredVariables::added(&from, &to).unwrap();
        assert_eq!(added.code, INVALID_VARIABLE_CODE);
        assert_eq!(added.names, ["cost_center", "owner"]);
        assert_eq!(
            added.line(),
            format!(
                "the move adds the required variables cost_center and owner with no default \
                 value: give each one a value at the update, or Palette refuses the update with \
                 {INVALID_VARIABLE_CODE}"
            )
        );
        let one = RequiredVariables {
            names: vec!["owner".to_string()],
            ..added
        };
        assert!(
            one.line().starts_with(
                "the move adds the required variable owner with no default value: give it a value"
            ),
            "{}",
            one.line()
        );
        assert_eq!(RequiredVariables::added(&to, &to), None);
        assert!(UpdateChecks::default().is_empty() && UpdateChecks::default().lines().is_empty());
    }

    #[test]
    fn the_kubernetes_move_is_the_move_of_the_pack_of_the_k8s_layer_on_each_side() {
        let from = [
            pack("os", "ubuntu-aws", "22.04"),
            pack("k8s", "kubernetes", "1.34.9"),
        ];
        let to = [
            pack("os", "ubuntu-aws", "24.04"),
            pack("k8s", "kubernetes", "1.36.2"),
        ];
        let skip = MinorSkip::between(&from, &to).unwrap();
        assert_eq!(
            (skip.pack.as_str(), skip.from.as_str(), skip.to.as_str()),
            ("kubernetes", "1.34.9", "1.36.2")
        );
        // an add-on profile moves no Kubernetes pack
        assert_eq!(
            MinorSkip::between(&from, &[pack("addon", "cert-manager", "1.21.1")]),
            None
        );
        assert_eq!(MinorSkip::between(&from, &from), None);
    }
}
