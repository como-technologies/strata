//! The hint at the end of the note of an unknown value: `strata report` shows what the user can
//! send.
//!
//! A [`Hint`] shows in a terminal only, and one time in a day at most. A file in the state folder
//! holds the time of the last hint, as a time in RFC 3339. The hint writes no other state.

use std::path::PathBuf;

use chrono::{DateTime, TimeDelta, Utc};

use crate::config::state_dir;
use crate::file::{read_file, write_file};

/// The name of the file that holds the time of the last hint, in the state folder.
pub const FILE: &str = "strata-unknown.hint";

/// The words of the hint. They follow the note of an unknown value.
pub(crate) const TEXT: &str = "`strata report` shows what you can send";

/// The time that must pass between two hints.
const EVERY: TimeDelta = TimeDelta::hours(24);

/// The file of the hint: [`FILE`] in the state folder. `None` when strata knows no state folder.
pub fn path() -> Option<PathBuf> {
    Some(state_dir()?.join(FILE))
}

/// When and where the hint can show.
#[derive(Debug, Clone)]
pub struct Hint {
    file: PathBuf,
    now: DateTime<Utc>,
    terminal: bool,
}

impl Hint {
    /// A hint that records its time in `file`. `now` is the time of the run. `terminal` is true
    /// when standard error is a terminal: a pipe or a file gets no hint.
    pub fn new(file: PathBuf, now: DateTime<Utc>, terminal: bool) -> Hint {
        Hint {
            file,
            now,
            terminal,
        }
    }

    /// The words of the hint when it is time to show it, or `None`. A call that returns the words
    /// has recorded the time, so the next call returns `None` for a day.
    ///
    /// Strata shows no hint when it cannot record the time. Else the hint shows on each run.
    pub(super) fn show(&self) -> Option<&'static str> {
        if !self.terminal || !self.due() {
            return None;
        }
        // strata shows no hint when it cannot record the time, because the hint then shows on each run
        write_file(&self.file, &format!("{}\n", self.now.to_rfc3339())).ok()?;
        Some(TEXT)
    }

    /// True when the file holds no time, holds a time that is not valid, or holds a time in the
    /// future. Also true when the file holds a time that is a day old or older.
    fn due(&self) -> bool {
        // a file that cannot be read has no time that counts: the hint shows, and the file
        // is written again
        let last = read_file(&self.file)
            .ok()
            .and_then(|text| DateTime::parse_from_rfc3339(text.trim()).ok());
        last.is_none_or(|last| {
            let age = self.now.signed_duration_since(last);
            age < TimeDelta::zero() || age >= EVERY
        })
    }
}
