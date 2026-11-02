//! The fact: one thing that Palette does and that strata depends on, as the list states it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::files::files_with_ext;

/// The schema id that each file of the list names, and that `xtask facts` prints.
pub const SCHEMA: &str = "strata-facts/1";

/// The directory of the list, from the root of the repository. One file holds the facts of one
/// part of the book.
pub const DIRECTORY: &str = "xtask/facts";

/// What a fact states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A request that strata sends: the fact names the request and what strata reads from the
    /// answer.
    Request,
    /// What Palette does after a change or for a given input, with no request of its own.
    Behavior,
    /// A check that strata makes because no route of Palette answers it: the fact names the
    /// error code that Palette gives at the create.
    Gap,
}

/// One fact of the list.
///
/// A script reads the same fields from the JSON that `xtask facts` prints.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    /// The name of the fact: lower case words with dashes, and one in the whole list.
    pub id: String,
    /// What the fact states.
    pub kind: Kind,
    /// The method and the path of the request, such as `GET /v1/clusterprofiles/{uid}`. Only a
    /// fact of the kind `request` has one.
    pub request: Option<String>,
    /// What strata reads from the answer or depends on.
    pub reads: Reads,
    /// The routes of the stand-in that imitate the request, as paths with `{name}` for a segment
    /// that varies. The list is empty when the stand-in serves no document for the fact.
    #[serde(default)]
    pub stand_in: Vec<String>,
    /// The live checks that prove the fact, each as `script.sh:check`.
    #[serde(default)]
    pub live: Vec<String>,
    /// Why no live check proves the fact. The reason is in the list when `live` is empty, and
    /// not otherwise.
    pub unchecked: Option<String>,
    /// The pages of the book that state the fact, each as a path under `docs/src/` and an
    /// optional `#anchor`.
    pub pages: Vec<String>,
    /// The files of strata that use the fact, from the root of the repository.
    #[serde(default)]
    pub code: Vec<String>,
    /// The rules of strata-preflight that use the fact.
    #[serde(default)]
    pub rules: Vec<String>,
}

/// What strata reads from the answer of Palette, or depends on: the fields, the error codes, and
/// the values of a closed set. A fact states at least one of the three.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reads {
    /// A field of the answer, as the path from the root of the document, with `[]` for an array:
    /// `spec.clusterProfileTemplates[].packs`.
    #[serde(default)]
    pub fields: Vec<String>,
    /// An error code that Palette gives, such as `ClusterProfilePackNotAllowed`. The code is on a
    /// page of the fact.
    #[serde(default)]
    pub codes: Vec<String>,
    /// The values of a closed set that strata names, by the field that holds them:
    /// `status.state` and the states that strata tells apart.
    #[serde(default)]
    pub values: BTreeMap<String, Vec<String>>,
}

impl Reads {
    /// True when the fact states no field, no code, and no value.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.codes.is_empty() && self.values.is_empty()
    }
}

/// A file of the list.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    schema: String,
    facts: Vec<Fact>,
}

/// Each fact of the files in `dir`, in the order of the file names and then of the file, and a
/// message for each file that cannot be read. The facts of the other files stay, so one fault in
/// one file does not hide the faults of the others.
pub fn load(dir: &Path) -> (Vec<Fact>, Vec<String>) {
    let mut facts = Vec::new();
    let mut bad = Vec::new();
    let files = files_with_ext(dir, "yaml");
    if files.is_empty() {
        bad.push(format!("{}: no file of facts", dir.display()));
    }
    for file in files {
        let name = file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let part = std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_yaml::from_str::<Part>(&text).map_err(|e| e.to_string()));
        match part {
            Ok(part) if part.schema == SCHEMA => facts.extend(part.facts),
            Ok(part) => bad.push(format!(
                "{name}: the schema is `{}`, and it must be `{SCHEMA}`",
                part.schema
            )),
            Err(e) => bad.push(format!("{name}: {e}")),
        }
    }
    (facts, bad)
}
