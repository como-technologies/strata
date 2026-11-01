//! The answers of Palette to the validate routes.
//!
//! `compare --validate` records a [`PaletteValidation`] in the diff, and `snapshot --validate`
//! records a [`ProfileValidation`] in the snapshot. Each answer is a [`ProfileValidation`]: the
//! [`PaletteRefusal`] errors that Palette gives, and it accepts when there is none.

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::json::{arr, text};
use crate::text::and_list;

/// Palette's answer to `POST /v1/spectroclusters/{uid}/validate/packs`, for one cluster.
///
/// The change is from what the cluster runs to a profile version. The document holds the cluster,
/// and the fields of the answer beside `clusterUid`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaletteValidation {
    /// The cluster that strata asked Palette about.
    pub cluster_uid: String,
    /// Whether Palette accepts, and each error that it gives. Palette gives the error of a pack
    /// under each profile that holds a pack of that name, so strata keeps one copy.
    #[serde(flatten)]
    pub answer: ProfileValidation,
}

/// Palette's answer to a validate route: each error that it gives, and it accepts when there is
/// none.
///
/// So no answer says `accepts` with an error, or refuses with no error. A document holds the answer
/// as `accepts` and `errors`: see [`PaletteValidation`] for the answer for a cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileValidation {
    /// Each error that Palette gives, sorted by pack, code, message, and clusters, with no copy.
    errors: Vec<PaletteRefusal>,
}

/// Palette's answer to a validate route.
///
/// `POST /v1/clusterprofiles/validate/packs` answers for a candidate, and
/// `POST /v1/clusterprofiles/{uid}/validate/packs` for a version that Palette holds. A
/// [`PaletteValidation`] holds the answer for a cluster.
// the fields of a `ProfileValidation` in a document, and the text of its schema. The answer
// borrows its errors to write them, and owns them when it reads them.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename = "ProfileValidation")]
struct ValidationDocument<'a> {
    /// True when Palette gives no error.
    accepts: bool,
    /// Each error that Palette gives, sorted by pack, code, message, and clusters. Palette gives
    /// them in no fixed order.
    #[serde(default, skip_serializing_if = "<[_]>::is_empty")]
    errors: Cow<'a, [PaletteRefusal]>,
}

impl Serialize for ProfileValidation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ValidationDocument {
            accepts: self.accepts(),
            errors: Cow::Borrowed(&self.errors),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProfileValidation {
    /// Read the fields of a document. A document that says `accepts` and gives an error, or that
    /// refuses with no error, is an error: Palette accepts when it gives no error.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let document = ValidationDocument::deserialize(deserializer)?;
        let errors = document.errors.into_owned();
        if document.accepts != errors.is_empty() {
            let fault = if errors.is_empty() {
                "accepts false with no error"
            } else {
                "accepts true with an error"
            };
            return Err(serde::de::Error::custom(format!(
                "the answer says {fault}; Palette accepts when it gives no error"
            )));
        }
        Ok(ProfileValidation::refused(errors))
    }
}

impl JsonSchema for ProfileValidation {
    fn schema_name() -> Cow<'static, str> {
        ValidationDocument::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        ValidationDocument::schema_id()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ValidationDocument::json_schema(generator)
    }
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
    /// The clusters that the error is about, sorted. Empty when the error is about no cluster, or
    /// when Palette names none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clusters: Vec<String>,
}

impl PaletteRefusal {
    /// `<code> on <clusters>` for an error about clusters. Else `<code> <pack>: <message>`, with
    /// no pack for a refusal of the whole request.
    pub fn line(&self) -> String {
        if !self.clusters.is_empty() {
            format!("{} on {}", self.code, and_list(&self.clusters))
        } else if self.pack.is_empty() {
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
                    clusters: Vec::new(),
                })
            })
            .collect()
    }

    /// The error with the clusters that it is about: the names of `candidates` that its message
    /// holds as a whole word. A name character is a letter, a digit, `-`, or `_`, so `prod-1` is
    /// not in a message that names `prod-10`.
    ///
    /// Palette names the clusters in the message of an error about a profile. It gives them in no
    /// field of their own.
    pub(crate) fn naming(mut self, candidates: &[String]) -> PaletteRefusal {
        let is_name = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_');
        let held = |name: &str| {
            !name.is_empty()
                && self.message.match_indices(name).any(|(at, _)| {
                    let before = self.message[..at].chars().next_back();
                    let after = self.message[at + name.len()..].chars().next();
                    !before.is_some_and(is_name) && !after.is_some_and(is_name)
                })
        };
        let mut named: Vec<String> = candidates
            .iter()
            .filter(|name| held(name))
            .cloned()
            .collect();
        named.sort();
        named.dedup();
        self.clusters = named;
        self
    }

    /// The refusal of the whole request in the body of a Palette error: its `code` and its
    /// `message`, with no pack. The code is text, or a number when the check of the body against
    /// the API schema refuses it. `None` when the body is not JSON with a `code`.
    pub(crate) fn from_error_body(body: &str) -> Option<PaletteRefusal> {
        PaletteRefusal::from_error(&serde_json::from_str(body).ok()?)
    }

    /// The refusal of the whole request in `error`, the body of a Palette error that the caller
    /// parsed already. `None` when `error` has no `code` of text or of a number.
    pub(crate) fn from_error(error: &Value) -> Option<PaletteRefusal> {
        let code = match error.get("code")? {
            Value::String(code) => code.clone(),
            Value::Number(code) => code.to_string(),
            _ => return None,
        };
        Some(PaletteRefusal {
            pack: String::new(),
            code,
            message: text(error, "/message"),
            clusters: Vec::new(),
        })
    }
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
    /// Read Palette's answer for the cluster `cluster_uid`: the errors under `profiles[].packs`
    /// and under `machinePools`.
    pub fn from_palette(cluster_uid: &str, answer: &Value) -> PaletteValidation {
        let errors = arr(answer, "/profiles")
            .iter()
            .filter_map(|p| p.get("packs"))
            .chain(answer.get("machinePools"))
            .flat_map(PaletteRefusal::from_constraints)
            .collect();
        PaletteValidation::refused(cluster_uid, errors)
    }

    /// The answer for the cluster `cluster_uid` with these errors, sorted and with no copy.
    ///
    /// It accepts when there is no error.
    pub fn refused(cluster_uid: &str, errors: Vec<PaletteRefusal>) -> PaletteValidation {
        PaletteValidation {
            cluster_uid: cluster_uid.to_string(),
            answer: ProfileValidation::refused(errors),
        }
    }
}

impl ProfileValidation {
    /// The answer with these errors, sorted and with no copy. It accepts when there is none.
    pub(crate) fn refused(mut errors: Vec<PaletteRefusal>) -> ProfileValidation {
        errors.sort();
        errors.dedup();
        ProfileValidation { errors }
    }

    /// True when Palette gives no error.
    pub fn accepts(&self) -> bool {
        self.errors.is_empty()
    }

    /// Each error that Palette gives, sorted by pack, code, message, and clusters, with no copy.
    pub fn errors(&self) -> &[PaletteRefusal] {
        &self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refusal() -> PaletteRefusal {
        PaletteRefusal {
            pack: "kubernetes".to_string(),
            code: "PackDependencyNotFound".to_string(),
            message: "cni is required".to_string(),
            clusters: Vec::new(),
        }
    }

    #[test]
    fn the_answer_for_a_cluster_holds_the_fields_of_the_answer_beside_the_cluster_uid() {
        let refusal = refusal();
        let v = PaletteValidation::refused("c1", vec![refusal.clone(), refusal.clone()]);
        assert_eq!(
            v.answer,
            ProfileValidation::refused(vec![refusal]),
            "the two copies of the error are one"
        );
        assert!(!v.answer.accepts());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(
            json.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["accepts", "clusterUid", "errors"]
        );
        assert_eq!(json["clusterUid"], "c1");
        assert_eq!(json["accepts"], false);
        assert_eq!(
            serde_json::from_value::<PaletteValidation>(json).unwrap(),
            v
        );

        let accepted = serde_json::to_value(PaletteValidation::refused("c1", Vec::new())).unwrap();
        assert_eq!(accepted, json!({"clusterUid": "c1", "accepts": true}));
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn an_error_names_the_clusters_that_its_message_holds_as_a_whole_word() {
        let error = PaletteRefusal {
            message: "Pack 'kubernetes' cannot move on clusters: prod-10, (stage_1) and edge."
                .to_string(),
            ..refusal()
        };
        let candidates = names(&[
            "prod-1", "prod-10", "stage_1", "edge", "stag", "", "stage_1",
        ]);
        let named = error.clone().naming(&candidates);
        assert_eq!(named.clusters, ["edge", "prod-10", "stage_1"]);
        assert_eq!(
            named.line(),
            "PackDependencyNotFound on edge, prod-10, and stage_1"
        );

        // a message that names no candidate leaves the error as Palette gave it
        let none = error.clone().naming(&names(&["prod-1"]));
        assert!(none.clusters.is_empty());
        assert_eq!(
            none.line(),
            format!("PackDependencyNotFound kubernetes: {}", error.message)
        );
        assert_eq!(error.naming(&[]).clusters, Vec::<String>::new());
    }

    #[test]
    fn the_clusters_of_an_error_are_in_the_document_only_when_there_are_some() {
        let plain = serde_json::to_value(refusal()).unwrap();
        assert!(plain.get("clusters").is_none(), "{plain}");
        let on = PaletteRefusal {
            clusters: vec!["payments".to_string()],
            ..refusal()
        };
        assert_eq!(on.line(), "PackDependencyNotFound on payments");
        let json = serde_json::to_value(&on).unwrap();
        assert_eq!(json["clusters"], json!(["payments"]));
        assert_eq!(serde_json::from_value::<PaletteRefusal>(json).unwrap(), on);
        // a document written before the field reads
        let old = json!({"pack": "kubernetes", "code": "X", "message": "m"});
        assert!(
            serde_json::from_value::<PaletteRefusal>(old)
                .unwrap()
                .clusters
                .is_empty()
        );
    }

    #[test]
    fn an_answer_that_refuses_with_no_error_cannot_be_built_or_read() {
        // the answer has no field for `accepts`: `refused` calculates it from the errors
        assert!(ProfileValidation::refused(Vec::new()).accepts());
        let v = ProfileValidation::refused(vec![refusal()]);
        assert!(!v.accepts());
        assert_eq!(v.errors(), [refusal()]);
        assert_eq!(
            serde_json::from_value::<ProfileValidation>(serde_json::to_value(&v).unwrap()).unwrap(),
            v
        );

        // a document that says one and gives the other does not read
        let err = serde_json::from_value::<ProfileValidation>(json!({"accepts": false}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("accepts false with no error"), "{err}");
        let err = serde_json::from_value::<ProfileValidation>(
            json!({"accepts": true, "errors": [refusal()]}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("accepts true with an error"), "{err}");
        let err = serde_json::from_value::<PaletteValidation>(
            json!({"clusterUid": "c1", "accepts": false}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("accepts false with no error"), "{err}");
    }
}
