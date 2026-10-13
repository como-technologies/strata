//! The lines of `consumers`: the heading, the counts, the second reading, and the edge hosts.

use crate::diff::Diff;
use crate::ops::{ClustersRead, ConsumersReport};
use crate::snapshot::Source;

use super::{count_of, dash, entry_error};

/// `1 consumer`, `2 consumers`, or `no consumers`.
pub(crate) fn consumer_count(n: usize) -> String {
    count_of(n, "consumer")
}

/// True when Palette lists the clusters that use a side: a published profile version has
/// consumers, but a side that records a cluster, or one pack from a registry, has none. A
/// candidate has none either, and for another reason: Palette does not hold it yet, so no request
/// asked about it. `no clusters` would state a count that nothing counted.
pub(crate) fn lists_consumers(s: &Source) -> bool {
    s.consumers.is_some()
}

/// The two counts, when a request gave one for each side. None when either side has none, so a
/// renderer cannot print a count that nothing counted.
pub(crate) fn consumer_counts(d: &Diff) -> Option<(usize, usize)> {
    Some((d.a.consumers.as_ref()?.count, d.b.consumers.as_ref()?.count))
}

/// What the consumers of the profile side say, when the other side is a cluster or a pack:
/// `aws-stack@1.1.0 has 1 consumer; payments is one of them`. The count of the other side is 0
/// because it has no consumers, so the reports state the profile side alone. None when both sides
/// are profile versions, or when neither is.
pub(crate) fn consumers_sentence(d: &Diff) -> Option<String> {
    let (profile, other) = match (lists_consumers(&d.a), lists_consumers(&d.b)) {
        (true, false) => (&d.a, &d.b),
        (false, true) => (&d.b, &d.a),
        _ => return None,
    };
    let consumers = profile.consumers.as_ref()?;
    let count = consumer_count(consumers.count);
    let has = format!("{}@{} has {count}", profile.name, profile.version);
    match &other.cluster {
        // the consumer list of Palette names clusters, and never an edge host
        Some(c) if !c.kind.is_cluster() => Some(has),
        Some(c) if consumers.count > 0 => Some(format!(
            "{has}; {} is {}one of them",
            c.name,
            if consumers.clusters.contains(&c.uid) {
                ""
            } else {
                "not "
            }
        )),
        _ => Some(has),
    }
}

/// What the second reading read, after `second reading: `: `3 of 3 clusters read` from the
/// listing, or, from the cluster search, `the cluster search names 2 other clusters with the
/// profile, 2 read`. The search takes the name of the profile, so it names each cluster that runs
/// a version of it; strata keeps the clusters that run this version. Only the clusters that the
/// consumer list did not name are read.
fn second_reading_counts(read: &crate::ops::ClustersRead) -> String {
    let counts = second_reading_main(read);
    // the virtual clusters of the cluster groups that pin the version (#184)
    match (read.groups, &read.groups_error) {
        (0, None) => counts,
        (0, Some(e)) => format!(
            "{counts}; cluster groups not read ({})",
            entry_error(Some(e)).unwrap_or("an error")
        ),
        (g, _) => format!(
            "{counts}; {g} cluster {} the version, {} other virtual {}, {} read",
            if g == 1 { "group pins" } else { "groups pin" },
            read.members,
            if read.members == 1 {
                "cluster"
            } else {
                "clusters"
            },
            read.members.saturating_sub(read.members_unread)
        ),
    }
}

fn second_reading_main(read: &crate::ops::ClustersRead) -> String {
    // the virtual clusters of the groups are counted apart, read and not read
    let n = (read.read + read.unread).saturating_sub(read.members);
    let read_n = read
        .read
        .saturating_sub(read.members.saturating_sub(read.members_unread));
    match read.resolved {
        Some(crate::ops::Resolved::Search) if n == 0 => {
            "the cluster search names no other cluster with the profile".to_string()
        }
        Some(crate::ops::Resolved::Search) => format!(
            "the cluster search names {} other {} with the profile, {} read",
            n,
            if n == 1 { "cluster" } else { "clusters" },
            read_n
        ),
        _ => format!("{read_n} of {n} clusters read"),
    }
}

/// `edge hosts: 3 read`, or why they could not be read.
pub(crate) fn edge_hosts_line(r: &ConsumersReport) -> String {
    let read = &r.edge_hosts_read;
    if read.skipped {
        return format!("edge hosts: not read ({})", read.reason);
    }
    match entry_error(read.error.as_ref()) {
        Some(msg) => format!("edge hosts: not read ({msg})"),
        None => format!("edge hosts: {} read", read.read),
    }
}

/// `second reading: 3 of 3 clusters read`, or why the second reading was not done.
///
/// `--list-only` asked for none, an app profile has none, or the search or the listing failed.
/// `code` puts the flag in the form of the report.
pub(crate) fn second_reading_line(read: &ClustersRead, code: fn(&str) -> String) -> String {
    // The consumer list of a profile is empty on an error too, so the second reading says how
    // far it got.
    let what = if read.skipped && !read.reason.is_empty() {
        format!("not done ({})", read.reason)
    } else if read.skipped {
        format!("not done ({})", code("--list-only"))
    } else if let Some(e) = &read.error {
        format!("not done ({})", entry_error(Some(e)).unwrap_or("an error"))
    } else {
        second_reading_counts(read)
    };
    format!("second reading: {what}")
}

/// `lab-profile@1.3.0 (uid <uid>): 2 consumers`, or `...: no consumers`. When no consumer was
/// found and a reading failed, the heading says so: `...: no consumer found, and a reading
/// failed`. The lines after it name the reading.
pub(crate) fn consumers_heading(r: &ConsumersReport) -> String {
    let count = if r.count == 0 && r.incomplete() {
        "no consumer found, and a reading failed".to_string()
    } else {
        consumer_count(r.count)
    };
    format!(
        "{}@{} (uid {}): {count}",
        r.profile.name,
        r.profile.version,
        dash(&r.profile.uid)
    )
}
