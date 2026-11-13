//! Renderers for the diff document, one per output format, behind one trait.

mod behind;
pub(super) mod changes;
mod consumers;
mod json;
mod list;
mod markdown;
pub(crate) mod notes;
mod report;
mod same_release;
mod terminal;
mod unified;
mod verify;

use clap::ValueEnum;
use serde_json::Value;

use crate::closed_set::ClosedSet;
use crate::closed_set::impl_texts;
use crate::closed_set::unknown::report::Report;
use crate::diff::Diff;
use crate::error::{MESSAGE_AT, Result};
use crate::json::opt_str;
use crate::model::cluster_state::ClusterState;
use crate::ops::{
    Chain, ConsumersReport, Listing, PackListing, SetReport, UNKNOWN_STATE, VerifySummary,
};
use crate::snapshot::Snapshot;
use crate::term::color::Paint;
use crate::text::{cut, plural};

pub use behind::RowState;
pub use json::JsonRenderer;
pub use list::{invalid_pack_warnings, secret_like_notes, set_counts_line};
pub use markdown::MarkdownRenderer;
pub use terminal::TerminalRenderer;

/// The output formats. On the command line, each is its name in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Colored text for a terminal, plain for a pipe.
    Terminal,
    /// A pull request comment.
    Markdown,
    /// The `strata-diff/1` document.
    Json,
}

impl_texts!(pub Format {
    Terminal => "terminal",
    Markdown => "markdown",
    Json => "json",
});

/// Turns a diff into text.
pub trait Renderer {
    /// Render the diff. `full` adds unified diffs of raw values and changed manifests where the
    /// format supports them.
    ///
    /// # Errors
    ///
    /// [`Error::Serialize`](crate::Error::Serialize) when the JSON renderer cannot write the
    /// document. The terminal renderer and the markdown renderer give no error.
    fn render(&self, d: &Diff, a: &Snapshot, b: &Snapshot, full: bool) -> Result<String>;

    /// One line that states the outcome of `verify`.
    ///
    /// `None` for a diff with no verify, and for the formats whose document carries the outcome.
    fn verdict(&self, _d: &Diff) -> Option<String> {
        None
    }

    /// Render the outcome of `verify` over several records.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_verify_summary(&self, s: &VerifySummary) -> Result<String>;

    /// Render the outcome of `same-release`.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_chain(&self, c: &Chain) -> Result<String>;

    /// Render the clusters that use a profile version.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_consumers(&self, r: &ConsumersReport) -> Result<String>;

    /// Render the profiles, clusters, and projects `list` found.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_list(&self, l: &Listing) -> Result<String>;

    /// The versions a registry offers for a pack.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_packs(&self, l: &PackListing) -> Result<String>;

    /// The pack versions and the profile versions that records hold, against their newer versions.
    /// `behind_only` omits each pin and each profile version that is current, where the
    /// format shows rows. The JSON document holds each one.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_behind(&self, r: &crate::ops::BehindReport, behind_only: bool) -> Result<String>;

    /// The records that `snapshot` wrote for a set of clusters, and the files it removed.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_set(&self, r: &SetReport) -> Result<String>;

    /// The values of closed sets that Palette gave and this release of strata does not know.
    ///
    /// # Errors
    ///
    /// The errors of [`Renderer::render`].
    fn render_report(&self, r: &Report) -> Result<String>;
}

/// The parts of a packs note that are there, joined with commas. None when no part is there.
fn joined_note<const N: usize>(parts: [Option<&str>; N]) -> Option<String> {
    let parts: Vec<&str> = parts.into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// `2 profiles`, `1 profile`, or `no profiles`.
///
/// Each count and its noun in a report comes from here, so zero reads in one way.
pub fn count_of(n: usize, singular: &str) -> String {
    count_of_noun(n, singular, &format!("{singular}s"))
}

/// `2 registries`, `1 registry`, or `no registries`: [`count_of`] for a noun whose plural is
/// `many`, and not the singular `one` with an `s`.
pub(crate) fn count_of_noun(n: usize, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        n => format!("{n} {}", plural(n, one, many)),
    }
}

/// The error message of a listed entry, when it has one.
pub(crate) fn entry_error(error: Option<&Value>) -> Option<&str> {
    error.and_then(|e| opt_str(e, MESSAGE_AT))
}

/// The text with its first letter in upper case.
pub(crate) fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The width of the key column of the terminal report: the longest key, `consumers`, and two
/// spaces.
const KEY_WIDTH: usize = 11;

/// One line of the terminal report: `key` padded to the key column, and then `text`, as in
/// `note       the content uses macro region`.
///
/// Each key of the report has this one width, so the texts of the lines start in one column.
pub(crate) fn key_line(key: &str, text: impl std::fmt::Display) -> String {
    format!("{key:<KEY_WIDTH$}{text}")
}

/// One line of the answer of Palette, as a report prints it: `palette    accepts`.
pub fn palette_line(line: &str) -> String {
    key_line("palette", line)
}

/// `state` in the color of a cluster state.
///
/// Green is a cluster that runs. Yellow is a cluster that Palette builds, or a state that strata
/// could not read. Red is a cluster in error or one that goes. A state that strata does not know
/// has no color.
pub fn paint_state(paint: &Paint, state: &str) -> String {
    match ClusterState::parse(state) {
        Some(ClusterState::Running) => paint.green(state),
        Some(ClusterState::Provisioning | ClusterState::Pending | ClusterState::Importing) => {
            paint.yellow(state)
        }
        Some(ClusterState::Error | ClusterState::Deleting | ClusterState::Deleted) => {
            paint.red(state)
        }
        None if state == UNKNOWN_STATE => paint.yellow(state),
        None => state.to_string(),
    }
}

/// The renderer for a format.
pub fn renderer(format: Format, paint: Paint, compact: bool) -> Box<dyn Renderer> {
    match format {
        Format::Terminal => Box::new(TerminalRenderer { paint }),
        Format::Markdown => Box::new(MarkdownRenderer),
        Format::Json => Box::new(JsonRenderer { compact }),
    }
}

/// A leaf value as the reports show it, or `null` for a side that has none.
fn shown(v: Option<&Value>) -> String {
    v.map(show).unwrap_or_else(|| "null".to_string())
}

/// The first twelve characters of a hash, or `-` for an empty one.
///
/// It cuts at a `char` boundary, so a short root in a record file does not stop strata.
pub fn short(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        let end = s.char_indices().nth(12).map(|(i, _)| i).unwrap_or(s.len());
        &s[..end]
    }
}

/// A digest as the reports show it: the name of its algorithm, and the first twelve characters of
/// the hash, as [`short`] cuts a root. `-` for an empty digest.
pub(crate) fn short_digest(digest: &str) -> String {
    match digest.split_once(':') {
        Some((algorithm, hash)) => format!("{algorithm}:{}", short(hash)),
        None => short(digest).to_string(),
    }
}

/// JSON rendering of a leaf value, truncated for the text reports; the JSON document carries all of it.
pub(crate) fn show(v: &Value) -> String {
    let t = serde_json::to_string(v).unwrap_or_default();
    let n = t.chars().count();
    if n > 120 {
        format!("{} ({n} chars, see --full)", cut(&t, 101))
    } else {
        t
    }
}

/// The end of the verdict of a pack record, and of a pin of `behind` that is current. It says
/// when the last complete sync of the registry started, that a sync runs now, or that the last
/// sync failed. It also names a recovery of a sync that stopped. Palette holds a copy of each
/// registry, and the answer states that copy. Nothing when Palette gives no sync.
fn registry_sync_suffix(sync: Option<&crate::diff::RegistrySync>) -> String {
    let Some(s) = sync else {
        return String::new();
    };
    let last_complete = (!s.last_synced_time.is_empty()).then(|| {
        format!(
            "the last complete sync of the registry started {}",
            s.last_synced_time
        )
    });
    let mut out = if s.failed() {
        let failed = match s.message.as_str() {
            "" => "the last sync of the registry failed".to_string(),
            message => format!("the last sync of the registry failed: {message}"),
        };
        match last_complete {
            Some(complete) => format!("; {complete}; {failed}"),
            None => format!("; {failed}"),
        }
    } else if s.in_progress() {
        let since = match s.last_run_time.as_str() {
            "" => String::new(),
            run => format!(" since {run}"),
        };
        let complete = last_complete
            .map(|c| format!(", and {c}"))
            .unwrap_or_default();
        format!("; a sync of the registry is in progress{since}{complete}")
    } else {
        last_complete.map(|c| format!("; {c}")).unwrap_or_default()
    };
    if let Some(r) = &s.sync_recovery {
        out.push_str(&format!(
            "; the recovery of the sync is {}",
            r.sync_recovery_state
        ));
        if !r.message.is_empty() {
            out.push_str(&format!(": {}", r.message));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cluster_state_has_one_color_in_every_report() {
        use crate::term::color::{ColorMode, Stream};
        let on = Paint::resolve(ColorMode::Always, Stream::Stdout);
        for (state, painted) in [
            ("Running", on.green("Running")),
            ("Provisioning", on.yellow("Provisioning")),
            ("Pending", on.yellow("Pending")),
            ("Importing", on.yellow("Importing")),
            ("Error", on.red("Error")),
            ("Deleting", on.red("Deleting")),
            ("Deleted", on.red("Deleted")),
            (UNKNOWN_STATE, on.yellow(UNKNOWN_STATE)),
            ("Upgrading", "Upgrading".to_string()),
            ("-", "-".to_string()),
        ] {
            assert_eq!(paint_state(&on, state), painted, "{state}");
            assert_eq!(paint_state(&Paint::off(), state), state, "{state}");
        }
    }

    #[test]
    fn each_format_has_the_name_that_the_command_line_gives_it() {
        let named: Vec<String> = Format::value_variants()
            .iter()
            .map(|f| f.to_possible_value().unwrap().get_name().to_string())
            .collect();
        let texts: Vec<&str> = Format::ALL.iter().map(|f| f.as_str()).collect();
        assert_eq!(texts, named);
        assert_eq!(Format::from_text("markdown"), Some(Format::Markdown));
    }

    #[test]
    fn a_long_value_shows_its_start_with_a_mark_and_its_length() {
        let long = Value::from("x".repeat(130));
        let shown = show(&long);
        assert!(
            shown.starts_with(&format!("\"{}…", "x".repeat(99))),
            "{shown}"
        );
        assert!(shown.ends_with(" (132 chars, see --full)"), "{shown}");
        assert_eq!(show(&Value::from("short")), "\"short\"");
    }

    #[test]
    fn a_digest_keeps_the_name_of_its_algorithm_and_twelve_characters_of_its_hash() {
        let hash = "d7aee32a7be434bba2a2d1ca8b6c559053844cc2da2d7cde82c306baba4394d0";
        assert_eq!(
            short_digest(&format!("sha256:{hash}")),
            "sha256:d7aee32a7be4"
        );
        // the root has no algorithm, and a digest with none is cut as the root is
        assert_eq!(short_digest(hash), short(hash));
        assert_eq!(short_digest("sha256:cni-a"), "sha256:cni-a");
        assert_eq!(short_digest(""), "-");
    }

    #[test]
    fn the_message_of_a_listed_entry_is_the_message_that_the_error_document_writes() {
        let document = crate::error::Error::NoUid.to_json();
        assert_eq!(
            entry_error(Some(&document)),
            Some("snapshot has no source.uid to verify against")
        );
        assert_eq!(entry_error(Some(&serde_json::json!({"kind": "x"}))), None);
        assert_eq!(entry_error(None), None);
    }

    #[test]
    fn a_line_of_the_answer_of_palette_starts_with_the_word_palette_in_the_key_column() {
        assert_eq!(palette_line("accepts"), "palette    accepts");
    }

    #[test]
    fn a_key_line_pads_each_key_to_the_width_of_the_longest_key() {
        assert_eq!(key_line("root", "a -> b"), "root       a -> b");
        assert_eq!(key_line("consumers", 2), "consumers  2");
        assert_eq!(key_line("palette", "x"), palette_line("x"));
    }
}
