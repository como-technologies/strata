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

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::sighting::Sighting;
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
pub const FIELDS: [&str; 5] = ["schema", "strata", "set", "value", "route"];

/// One line of the log: a value of a closed set that Palette gave and this release does not know.
///
/// The fields of the [`Sighting`] are in the object of the line, beside `schema`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    /// Always [`SCHEMA`].
    pub schema: String,
    /// The value, and the version of strata and the route that read it.
    #[serde(flatten)]
    pub sighting: Sighting,
}

impl Line {
    /// The line for the value `value` of the set `set`, which the route `route` gave.
    pub(crate) fn new(set: &str, value: &str, route: &str) -> Line {
        Line {
            schema: SCHEMA.to_string(),
            sighting: Sighting::new(set, value, route),
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
        self.sighting.fault()
    }
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
