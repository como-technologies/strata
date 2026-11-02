//! The log of the unknown values: one line in `strata-unknown.jsonl` for each value that strata
//! noted.
//!
//! The log is a file of the user, in the state folder: see [`path`]. It never leaves the machine.
//! Later stages of the self-report, `strata report` and the opt-in send, read only this file.
//!
//! # The line
//!
//! Each line is one JSON object, the document `strata-unknown/1`. A [`Line`] has only the fields
//! of [`FIELDS`]. The set is a closed set, so its values are short words and hold no secret. A
//! line holds no name, no uid, no host, no free text, and no value of a pack. The route is a
//! template, such as `/v1/clusterprofiles/{uid}`, and never the path of a real request.
//!
//! `status` and `errorCode` state a refusal of Palette. A value of a document that Palette served
//! comes from an answer of status 2xx with no error code, so a line of such a value has neither.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::TOOL_VERSION;
use crate::config::state_dir;
use crate::error::Result;
use crate::file::append_file;

/// The schema id that each line names.
pub const SCHEMA: &str = "strata-unknown/1";

/// The name of the log, in the state folder.
pub const FILE: &str = "strata-unknown.jsonl";

/// The fields of a line, as the JSON names them. A reader refuses a line with another field.
pub const FIELDS: [&str; 8] = [
    "schema",
    "strata",
    "palette",
    "set",
    "value",
    "route",
    "status",
    "errorCode",
];

/// One line of the log: a value of a closed set that Palette gave and this release does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    /// Always [`SCHEMA`].
    pub schema: String,
    /// The version of strata that read the value.
    pub strata: String,
    /// The Palette release, when strata knows it. A key cannot read the release, so a line of the
    /// command line has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    /// The name of the closed set, as `strata schema` names its definition.
    pub set: String,
    /// The text of the value.
    pub value: String,
    /// The template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`.
    pub route: String,
    /// The status of a refusal of Palette. A value of a document has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The error code of a refusal of Palette. A value of a document has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl Line {
    /// The line for the value `value` of the set `set`, which the route `route` gave. `palette` is
    /// the Palette release, when strata knows it.
    pub(crate) fn new(set: &str, value: &str, route: &str, palette: Option<&str>) -> Line {
        Line {
            schema: SCHEMA.to_string(),
            strata: TOOL_VERSION.to_string(),
            palette: palette.map(str::to_string),
            set: set.to_string(),
            value: value.to_string(),
            route: route.to_string(),
            status: None,
            error_code: None,
        }
    }

    /// Append the line to the file `file`. The call makes the folder and the file when they are
    /// missing, and writes the line with one write, so two runs do not mix their lines.
    ///
    /// # Errors
    ///
    /// `Io`, with the file: for example, a folder that the user cannot write.
    pub(crate) fn append(&self, file: &Path) -> Result<()> {
        let mut text = serde_json::to_string(self)?;
        text.push('\n');
        append_file(file, &text)
    }
}

/// The log of the user: [`FILE`] in the state folder. `None` when strata knows no state folder.
pub fn path() -> Option<PathBuf> {
    Some(state_dir()?.join(FILE))
}
