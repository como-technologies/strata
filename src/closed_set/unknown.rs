//! What strata does when Palette gives a value of a closed set that this release does not know.
//!
//! A field that holds a value of a closed set is an [`Open`](super::Open) value. So this release
//! reads a value that a later Palette release adds with no error, and nothing told a person. A
//! [`Watch`] tells the person, and keeps a local record.
//!
//! # The note and the log
//!
//! A [`Watch`] sees each value that a read of a document of Palette gives and that the set does
//! not name. For each set and value, one time in a run, it does two things:
//!
//! - It writes one note on standard error: `strata: Palette gave the <set> value '<text>', which
//!   this release of strata does not know`. `--quiet` hides the note.
//! - It appends one line to the log: see [`log`]. `--quiet` does not stop the log.
//!
//! After the first note of a day, in a terminal, the watch adds a hint that `strata report` shows
//! what the user can send: see [`hint`]. The report is made from the log: see [`report`]. A user
//! who chose the send gets the report posted at the end of the run: see [`send`].
//!
//! The set is the name that `strata schema` gives to the definition of the set. The JSON
//! documents do not change.
//!
//! A value that strata reads from a record or from a report is not in this scope. A later release
//! of strata wrote it, and Palette did not give it.
//!
//! # Where a watch works
//!
//! The reads of the documents of Palette are in many modules, and none of them holds the progress
//! or the log. So [`Watch::start`] puts the watch on the current thread, and the guard that it
//! returns removes the watch when it drops. A program that starts no watch sees no note and writes
//! no log. A user of the library stays quiet, as before. Strata runs on one thread, and a watch
//! of one thread does not see the reads of another.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;

use super::ClosedSet;
use crate::term::progress::Progress;
use crate::text::cut;

pub mod hint;
pub mod log;
pub mod report;
pub mod send;
#[cfg(test)]
mod tests;

pub use hint::Hint;
pub use log::Line;
pub use report::Report;

/// The most characters of a value that the note and the log show. A value of a closed set is a
/// short word, so a longer text is not a value of a set.
const LONGEST_VALUE: usize = 64;

/// What strata does with each unknown value of a run: the note, and the line of the log.
#[derive(Debug)]
pub struct Watch {
    progress: Progress,
    log: Option<PathBuf>,
    hint: Option<Hint>,
    release: Option<String>,
    /// How many lines this watch wrote to the log.
    written: usize,
    /// The set and the value of each note so far, so that a run notes a value one time.
    told: BTreeSet<(String, String)>,
}

impl Watch {
    /// A watch that writes the note through `progress`, and writes no log.
    pub fn new(progress: Progress) -> Watch {
        Watch {
            progress,
            log: None,
            hint: None,
            release: None,
            written: 0,
            told: BTreeSet::new(),
        }
    }

    /// Also append a line to the file `file` for each unknown value: see [`log::path`].
    #[must_use]
    pub fn logging_to(mut self, file: PathBuf) -> Watch {
        self.log = Some(file);
        self
    }

    /// Also show `hint` after the first note that has a line in the log, when it is time: see
    /// [`hint`]. A watch with no log shows no hint, because `strata report` would show nothing.
    #[must_use]
    pub fn hinting(mut self, hint: Hint) -> Watch {
        self.hint = Some(hint);
        self
    }

    /// Name the Palette `release` in each line, when the caller knows it.
    #[must_use]
    pub fn palette_release(mut self, release: &str) -> Watch {
        self.release = Some(release.to_string());
        self
    }

    /// Put the watch on the current thread, in place of the watch that is there. The watch ends
    /// when the guard drops.
    #[must_use = "the watch ends when the guard drops"]
    pub fn start(self) -> Watching {
        WATCH.with(|slot| *slot.borrow_mut() = Some(self));
        Watching(())
    }

    /// Tell about the value `value` of the set `set`, which the route `route` gave. A value that
    /// the watch told about before is not told again.
    fn see(&mut self, set: &str, value: &str, route: &str) {
        let value = shown(value);
        if !self.told.insert((set.to_string(), value.clone())) {
            return;
        }
        self.progress.note(&note(set, &value));
        let Some(file) = &self.log else { return };
        let line = Line::new(set, &value, route, self.release.as_deref());
        if let Err(e) = line.append(file) {
            self.progress
                .warn(&format!("could not write {}: {e}", file.display()));
            // a log that fails once fails again, and one warning is enough
            self.log = None;
            return;
        }
        self.written += 1;
        if let Some(text) = self.hint_text() {
            self.progress.note(text);
        }
    }

    /// The words of the hint, when this run shows it. The hint gets one try in a run. `--quiet`
    /// hides the note, so it hides the hint too, and it does not use up the day.
    fn hint_text(&mut self) -> Option<&'static str> {
        if self.progress.is_quiet() {
            return None;
        }
        self.hint.take()?.show()
    }
}

/// The guard of a started watch: it removes the watch from the thread when it drops.
#[derive(Debug)]
pub struct Watching(());

impl Watching {
    /// How many lines of the log the watch has written so far. A run that wrote none saw no
    /// unknown value, and has nothing to send.
    pub fn lines_written(&self) -> usize {
        WATCH.with(|slot| slot.borrow().as_ref().map_or(0, |watch| watch.written))
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        WATCH.with(|slot| *slot.borrow_mut() = None);
    }
}

thread_local! {
    /// The watch of this thread, when a caller started one.
    static WATCH: RefCell<Option<Watch>> = const { RefCell::new(None) };
}

/// Tell the watch of this thread, when there is one, that a read of a document gave the text
/// `value` for the set `T`, and that `T` does not name it. `route` is the template of the route
/// that gave the document, such as `/v1/clusterprofiles/{uid}`.
pub(crate) fn seen<T: ClosedSet>(value: &str, route: &'static str) {
    WATCH.with(|slot| {
        if let Some(watch) = slot.borrow_mut().as_mut() {
            watch.see(&T::schema_name(), value, route);
        }
    });
}

/// `value` cut to [`LONGEST_VALUE`] characters, with a `?` in place of each control character. So
/// a value cannot make the terminal do more than show text.
fn shown(value: &str) -> String {
    cut(value, LONGEST_VALUE)
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// The note for the value `value` of the set `set`, without the name of the tool.
fn note(set: &str, value: &str) -> String {
    format!("Palette gave the {set} value '{value}', which this release of strata does not know")
}
