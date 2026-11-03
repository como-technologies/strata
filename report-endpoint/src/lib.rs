//! The endpoint of the report: a small service that takes the report of the values that strata
//! does not know. It merges the reports into one issue for each set and value.
//!
//! A user of strata chooses the send, and strata posts the document `strata-report/1` here: see
//! `strata::closed_set::unknown::send`. The endpoint does these things for each request, in this
//! order:
//!
//! 1. It checks the request, with [`accept::parse`]. It takes only the schema of the report and
//!    its fixed list of fields, and refuses each other field.
//! 2. It limits the rate of the sender, with [`limit::Limiter`]. It keeps no address, and it counts
//!    an IPv6 address by its /64 network.
//! 3. It omits a report that it took in the last day, with [`digest::Counted`], so a second
//!    try of a send does not count twice.
//! 4. It reads the issues of the tracker, and limits the new issues and the writes, so that a flood
//!    of made-up values opens few issues. The values that the limits hold back wait.
//! 5. It answers 202, and then merges the report into the issues of the tracker, with
//!    [`merge::apply`]. A set and a value have one issue. The issue holds a count, and no request
//!    opens an issue for itself.
//!
//! The tracker is a trait, [`tracker::Tracker`]. The service writes to GitHub with
//! [`tracker::GitHub`], and the tests write to a stand-in.

pub mod accept;
pub mod digest;
pub mod limit;
pub mod merge;
pub mod server;
pub mod settings;
pub mod tracker;
