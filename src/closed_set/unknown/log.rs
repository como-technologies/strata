//! The log of the unknown values: one line in `strata-unknown.jsonl` for each value that strata
//! noted.
//!
//! The log is a file of the user, in the state folder: see [`path`]. The file never leaves the
//! machine. `strata report` and the opt-in send read only this file, and they send the report of its
//! lines, never the file. A reader refuses a line that holds more than the fields below.
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

use super::LONGEST_VALUE;
use crate::TOOL_VERSION;
use crate::config::state_dir;
use crate::error::{Error, Result};
use crate::file::{append_file, read_file, write_file};

/// The schema id that each line names.
pub const SCHEMA: &str = "strata-unknown/1";

/// The name of the log, in the state folder.
pub const FILE: &str = "strata-unknown.jsonl";

/// The name of the file beside the log that counts the lines which the user marked as sent.
pub const SENT_FILE: &str = "strata-unknown.sent";

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

    /// What is wrong with a line that has the right fields, or `None`. The type refuses a field
    /// that is not in [`FIELDS`]. This check refuses a value that no closed set has, and a route
    /// that is not a template of a route, such as the path of a real request. Strata never writes
    /// such a line.
    fn fault(&self) -> Option<String> {
        if self.schema != SCHEMA {
            return Some(format!("the schema is not {SCHEMA}"));
        }
        value_fault(&self.value).or_else(|| route_fault(&self.route))
    }
}

/// What is wrong with `value` as the value of a closed set, or `None`. A value is a short word:
/// it has [`LONGEST_VALUE`] characters or fewer, and no control character.
pub(super) fn value_fault(value: &str) -> Option<String> {
    if value.is_empty()
        || value.chars().count() > LONGEST_VALUE
        || value.chars().any(char::is_control)
    {
        return Some(format!(
            "the value is not a short word of {LONGEST_VALUE} characters or fewer"
        ));
    }
    None
}

/// What is wrong with `route` as the template of a route, or `None`. A template starts with
/// [`ROUTE_PREFIX`], holds only the characters of [`is_route_char`], and none of its words is a uid.
pub(super) fn route_fault(route: &str) -> Option<String> {
    if !route.starts_with(ROUTE_PREFIX)
        || !route.chars().all(is_route_char)
        || route.split('/').any(looks_like_uid)
    {
        return Some("the route is not a template of a route".to_string());
    }
    None
}

/// The start of each route that strata reads.
const ROUTE_PREFIX: &str = "/v1/";

/// True for a character of a route template: a letter or a digit of ASCII, or one of `/`, `_`, `-`,
/// `.`, `{`, and `}`. So a template holds no space, no quote, no mark of markdown, and no line
/// break.
fn is_route_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '{' | '}')
}

/// The fewest characters of a uid. A uid of Palette has 24 hexadecimal digits, and a UUID has more.
const UID_LENGTH: usize = 24;

/// True when `segment` is as long as a uid and holds only hexadecimal digits and hyphens. A word
/// of a route holds other letters, and a placeholder such as `{uid}` holds braces.
fn looks_like_uid(segment: &str) -> bool {
    segment.len() >= UID_LENGTH && segment.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// The log of the user: [`FILE`] in the state folder. `None` when strata knows no state folder.
pub fn path() -> Option<PathBuf> {
    Some(state_dir()?.join(FILE))
}

/// The lines of the log `file`, in order. A file that is not there is an empty log. A blank line
/// is no line.
///
/// # Errors
///
/// `LogInvalid`, with the number of the first line that is not a line of [`SCHEMA`]: a field that
/// [`FIELDS`] does not list, a field of the wrong type, or a value that strata does not write. The
/// message names the fault. `Io` when strata cannot read the file.
pub(crate) fn read(file: &Path) -> Result<Vec<Line>> {
    let text = match read_file(file) {
        Ok(text) => text,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(e) => return Err(e),
    };
    let invalid = |number: usize, message: String| Error::LogInvalid {
        path: file.to_path_buf(),
        line: number,
        message,
    };
    let mut lines = Vec::new();
    for (index, text) in text.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let line: Line =
            serde_json::from_str(text).map_err(|e| invalid(index + 1, e.to_string()))?;
        if let Some(fault) = line.fault() {
            return Err(invalid(index + 1, fault));
        }
        lines.push(line);
    }
    Ok(lines)
}

/// The file that counts the lines of the log that the user marked as sent: [`SENT_FILE`], beside
/// the log `log`.
pub(crate) fn sent_path(log: &Path) -> PathBuf {
    log.with_file_name(SENT_FILE)
}

/// The number of lines of the log that the file `sent` marks as sent.
///
/// A file that is not there, or that holds no number, marks none: the next report then holds
/// more lines, and never fewer.
pub(crate) fn sent_count(sent: &Path) -> usize {
    // an error and a missing file are one answer here: no mark, so the report holds more lines
    read_file(sent)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// Mark the first `count` lines of the log as sent, in the file `sent`.
///
/// # Errors
///
/// `Io`, with the file: for example, a folder that the user cannot write.
pub(crate) fn mark_sent(sent: &Path, count: usize) -> Result<()> {
    write_file(sent, &format!("{count}\n"))
}
