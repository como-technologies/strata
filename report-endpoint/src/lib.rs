//! The endpoint of the report: a small service that takes the report of the values that strata
//! does not know, and merges the reports into one issue for each set and value.
//!
//! A user of strata turns the send on, and strata posts the document `strata-report/1` here: see
//! `strata::closed_set::unknown::send`. The endpoint does four things for each request, in this
//! order:
//!
//! 1. It limits the rate of the sender, with [`limit::Limiter`]. It keeps no address.
//! 2. It checks the request, with [`accept::parse`]. It takes only the schema of the report and
//!    its fixed list of fields, and refuses each other field.
//! 3. It limits the new issues, so that a flood of made-up values opens few issues.
//! 4. It merges the report into the issues of the tracker, with [`merge::merge`]. A set and a value
//!    have one issue. The issue holds a count, and no request opens an issue for itself.
//!
//! The tracker is a trait, [`tracker::Tracker`]. The service writes to GitHub with
//! [`tracker::GitHub`], and the tests write to a stand-in.

pub mod accept;
pub mod limit;
pub mod merge;
pub mod server;
pub mod settings;
pub mod tracker;
