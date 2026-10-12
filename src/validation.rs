//! The answers of Palette to the validate routes: `compare --validate` records a
//! [`PaletteValidation`] in the diff, and `snapshot --validate` records a [`ProfileValidation`] in
//! the snapshot. Each holds the [`PaletteRefusal`] errors that Palette gives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::json::{arr, text};

/// Palette's answer to `POST /v1/spectroclusters/{uid}/validate/packs`, for a change from what a
/// cluster runs to a profile version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteValidation {
    /// The cluster that strata asked Palette about.
    pub cluster_uid: String,
    /// True when Palette gives no error.
    pub accepts: bool,
    /// Each error that Palette gives, sorted by pack, code, and message: Palette sorts the
    /// profiles in no fixed order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<PaletteRefusal>,
}

/// Palette's answer to `POST /v1/clusterprofiles/validate/packs` for a candidate, or to `POST
/// /v1/clusterprofiles/{uid}/validate/packs` for a version that Palette holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValidation {
    /// True when Palette gives no error.
    pub accepts: bool,
    /// Each error that Palette gives, sorted by pack, code, and message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<PaletteRefusal>,
}

/// One error that Palette gives for a pack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteRefusal {
    /// The pack that the error is about. Empty when Palette refuses the whole request.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pack: String,
    /// For example `PackDependencyNotFound`.
    pub code: String,
    /// Palette's sentence.
    pub message: String,
}

impl PaletteRefusal {
    /// `<code> <pack>: <message>`, with no pack for a refusal of the whole request.
    pub fn line(&self) -> String {
        if self.pack.is_empty() {
            format!("{}: {}", self.code, self.message)
        } else {
            format!("{} {}: {}", self.code, self.pack, self.message)
        }
    }

    /// The errors of a `v1ConstraintValidatorResponse`: `results[]`, each with the `name` of its
    /// pack and `errors[]` of `code` and `message`.
    pub fn from_constraints(answer: &Value) -> Vec<PaletteRefusal> {
        arr(answer, "/results")
            .iter()
            .flat_map(|r| {
                let pack = text(r, "/name");
                arr(r, "/errors").iter().map(move |e| PaletteRefusal {
                    pack: pack.clone(),
                    code: text(e, "/code"),
                    message: text(e, "/message"),
                })
            })
            .collect()
    }

    /// The refusal of the whole request in the body of a Palette error: its `code` and its
    /// `message`, with no pack. The code is text, or a number when the check of the body against
    /// the API schema refuses it. `None` when the body is not JSON with a `code`.
    pub(crate) fn from_error_body(body: &str) -> Option<PaletteRefusal> {
        let error: Value = serde_json::from_str(body).ok()?;
        let code = match error.get("code")? {
            Value::String(code) => code.clone(),
            Value::Number(code) => code.to_string(),
            _ => return None,
        };
        Some(PaletteRefusal {
            pack: String::new(),
            code,
            message: text(&error, "/message"),
        })
    }
}

/// The errors sorted and with no copy, and true when there is none.
fn sorted(mut errors: Vec<PaletteRefusal>) -> (bool, Vec<PaletteRefusal>) {
    errors.sort();
    errors.dedup();
    (errors.is_empty(), errors)
}

/// The lines of an answer of Palette to `--validate`: `accepts` when there is no error, or else
/// one line for each error, such as `refuses: PackDependencyNotFound strata-live-pack:
/// <message>`.
pub fn answer_lines(errors: &[PaletteRefusal]) -> Vec<String> {
    if errors.is_empty() {
        return vec!["accepts".to_string()];
    }
    errors
        .iter()
        .map(|e| format!("refuses: {}", e.line()))
        .collect()
}

impl PaletteValidation {
    /// Read Palette's answer: the errors under `profiles[].packs` and under `machinePools`.
    /// Palette gives the error of a pack under each profile that holds a pack of that name, so
    /// strata keeps one copy.
    pub fn from_palette(cluster_uid: &str, answer: &Value) -> PaletteValidation {
        let errors = arr(answer, "/profiles")
            .iter()
            .filter_map(|p| p.get("packs"))
            .chain(answer.get("machinePools"))
            .flat_map(PaletteRefusal::from_constraints)
            .collect();
        PaletteValidation::refused(cluster_uid, errors)
    }

    /// The answer with these errors, sorted and with no copy. It accepts when there is none.
    pub fn refused(cluster_uid: &str, errors: Vec<PaletteRefusal>) -> PaletteValidation {
        let (accepts, errors) = sorted(errors);
        PaletteValidation {
            cluster_uid: cluster_uid.to_string(),
            accepts,
            errors,
        }
    }
}

impl ProfileValidation {
    /// The answer with these errors, sorted and with no copy. It accepts when there is none.
    pub fn refused(errors: Vec<PaletteRefusal>) -> ProfileValidation {
        let (accepts, errors) = sorted(errors);
        ProfileValidation { accepts, errors }
    }
}
