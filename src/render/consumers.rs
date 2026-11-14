//! The lines of `consumers`: the heading, the counts, and what strata searched.

use serde_json::Value;

use super::{count_of, entry_error};
use crate::closed_set::impl_texts;
use crate::diff::Diff;
use crate::model::profile_type::ProfileType;
use crate::ops::{ClustersRead, Consumer, ConsumerKind, ConsumersReport, Resolved, UnreadProject};
use crate::reference::name_version;
use crate::snapshot::Source;
use crate::text::{and_list, dash, name_or_uid, plural};

/// What a row says about a consumer that the second reading found, and that the consumer list of
/// the profile does not name.
pub(crate) const NOT_LISTED: &str = "not in the consumer list of the profile";

/// `runs aws-stack@1.1.1, which uid <uid> no longer names`: the version that the copy of a
/// consumer names, when the uid of the profile names another version now.
///
/// None when the copy names the version of the report, or names none. `code` puts the version
/// and the uid in the form of the report.
pub(crate) fn consumer_runs(
    c: &Consumer,
    r: &ConsumersReport,
    code: fn(&str) -> String,
) -> Option<String> {
    (!c.version.is_empty() && c.version != r.profile.version).then(|| {
        format!(
            "runs {}, which uid {} no longer names",
            code(&name_version(&r.profile.name, &c.version)),
            code(&r.profile.uid)
        )
    })
}

/// The project of a consumer, as a row names it: its name, or its uid with no project listing.
/// `-` when the consumer names no project.
pub(crate) fn consumer_project(c: &Consumer) -> &str {
    dash(name_or_uid(&c.project_name, &c.project_uid))
}

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
    let has = format!(
        "{} has {count}",
        name_version(&profile.name, &profile.version)
    );
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

/// The message of `e`, an error of a read.
///
/// Each error that strata writes has a message. The fallback is for an error value that a
/// program builds with no message.
fn error_message(e: &Value) -> &str {
    entry_error(Some(e)).unwrap_or("an error")
}

/// A reading of `consumers`, as a row of the block `searched:` names it.
///
/// Each row names the kind of consumer that the reading gives. The consumer list gives the
/// clusters and the cluster templates. The search and the listing give clusters, the cluster
/// groups give virtual clusters, and the edge host search gives edge hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// The consumer list that Palette computes for the version: `status.inUseClusters` and
    /// `status.inUseClusterTemplates`, or `status.inUseApps` of an app profile.
    ConsumerList,
    /// The cluster search by the name of the profile.
    ClusterSearch,
    /// Each cluster of the cluster listing, when the search of Palette takes no profile.
    ClusterListing,
    /// The cluster groups that pin the version, and their virtual clusters.
    ClusterGroups,
    /// The search of the edge hosts.
    EdgeHosts,
    /// For an app profile: a search of the app deployments beside the consumer list.
    AppDeployments,
}

impl_texts!(pub Reading {
    ConsumerList => "consumer list",
    ClusterSearch => "cluster search",
    ClusterListing => "cluster listing",
    ClusterGroups => "cluster groups",
    EdgeHosts => "edge hosts",
    AppDeployments => "app deployments",
});

/// The rows of the block `searched:` of `consumers`: each reading, and what it found.
///
/// A row gives counts, and never the name of a consumer, so no row reads as a consumer. The
/// lines above the block name each consumer. A row holds no code span and no other mark of
/// markdown of strata, so the markdown escapes each row as one quoted text.
pub(crate) fn searched_rows(r: &ConsumersReport) -> Vec<(Reading, String)> {
    let listed = |kind: ConsumerKind| {
        let n = r
            .consumers
            .iter()
            .filter(|c| c.kind == kind && !c.unlisted)
            .count();
        count_of(n, kind.noun())
    };
    if r.profile.kind == Some(ProfileType::App) {
        return vec![
            (Reading::ConsumerList, listed(ConsumerKind::App)),
            (
                Reading::AppDeployments,
                format!("not searched ({})", r.clusters_read.reason),
            ),
            (Reading::EdgeHosts, edge_hosts_found(r)),
        ];
    }
    let found = r
        .consumers
        .iter()
        .filter(|c| c.kind == ConsumerKind::Cluster && c.unlisted)
        .count();
    vec![
        (
            Reading::ConsumerList,
            format!(
                "{}, {}",
                listed(ConsumerKind::Cluster),
                listed(ConsumerKind::Template)
            ),
        ),
        clusters_found(&r.clusters_read, found),
        (Reading::ClusterGroups, groups_found(&r.clusters_read)),
        (Reading::EdgeHosts, edge_hosts_found(r)),
    ]
}

/// The row of the search or of the listing: the other clusters that it names, and how many of
/// them run the version.
///
/// `found` counts the consumers that the consumer list does not name. The search, the listing,
/// or a cluster group gave them. The search takes the name of the profile, so each cluster that
/// it names runs a version of that name. The listing names each cluster of the scope.
fn clusters_found(read: &ClustersRead, found: usize) -> (Reading, String) {
    let reading = cluster_reading(read);
    if read.skipped {
        return (reading, "not done (--list-only)".to_string());
    }
    if let Some(e) = &read.error {
        return (reading, format!("not done ({})", error_message(e)));
    }
    // the virtual clusters of the groups are counted apart, read and not read
    let named = (read.read + read.unread).saturating_sub(read.members);
    let unread = read.unread.saturating_sub(read.members_unread);
    let on_this = found.saturating_sub(read.members_found);
    let other = named.saturating_sub(unread).saturating_sub(on_this);
    let not_read = not_read(unread);
    let clusters = plural(named, "cluster", "clusters");
    let text = match reading {
        Reading::ClusterSearch if named == 0 => "no other cluster with the profile".to_string(),
        _ if named == 0 => "no other cluster".to_string(),
        Reading::ClusterSearch => format!(
            "{named} other {clusters} with the profile: {on_this} on this version, \
             {other} on other versions{not_read}"
        ),
        _ => format!("{named} other {clusters}: {on_this} on this version{not_read}"),
    };
    (reading, text)
}

/// The reading that named the clusters beside the consumer list: the search, or the listing when
/// the search of Palette takes no profile.
fn cluster_reading(read: &ClustersRead) -> Reading {
    match read.resolved {
        Some(Resolved::Listing) => Reading::ClusterListing,
        _ => Reading::ClusterSearch,
    }
}

/// `, 2 not read`, or nothing when strata read each one.
fn not_read(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!(", {n} not read")
    }
}

/// The row of the cluster groups: the groups that pin the version, and their virtual clusters
/// (#184). The cluster search does not name a virtual cluster that runs the version through its
/// group.
fn groups_found(read: &ClustersRead) -> String {
    if read.skipped {
        return "not read (--list-only)".to_string();
    }
    if read.error.is_some() {
        // the second reading stops at a failed search, before it lists the groups
        return format!("not read (the {} failed)", cluster_reading(read).as_str());
    }
    let failed = read
        .groups_error
        .as_ref()
        .map(|e| format!("not read ({})", error_message(e)));
    match (read.groups, failed) {
        (0, None) => "none pins the version".to_string(),
        (0, Some(failed)) => failed,
        (g, failed) => {
            let pins = format!("{g} {} the version", plural(g, "pins", "pin"));
            let members = match read.members {
                0 => format!("{pins}: no other virtual cluster"),
                m => format!(
                    "{pins}: {m} other virtual {}: {} on this version{}",
                    plural(m, "cluster", "clusters"),
                    read.members_found,
                    not_read(read.members_unread)
                ),
            };
            match failed {
                Some(failed) => format!("{members}; a group {failed}"),
                None => members,
            }
        }
    }
}

/// The row of the edge hosts: how many the search names, and how many of them the version binds.
/// Or why strata could not read them.
fn edge_hosts_found(r: &ConsumersReport) -> String {
    let read = &r.edge_hosts_read;
    if read.skipped {
        return format!("not read ({})", read.reason);
    }
    if let Some(msg) = entry_error(read.error.as_ref()) {
        return format!("not read ({msg})");
    }
    let bound = r
        .consumers
        .iter()
        .filter(|c| c.kind == ConsumerKind::EdgeHost)
        .count();
    match read.read {
        0 => "none found".to_string(),
        n => format!("{bound} of {n} bound to the version"),
    }
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
        "{} (uid {}): {count}",
        name_version(&r.profile.name, &r.profile.version),
        dash(&r.profile.uid)
    )
}

/// `scope: project profile, so only project dev can use it`. A profile of the tenant or of the
/// system gives `scope: tenant profile, usable in every project; 3 of 3 projects searched`.
///
/// The count is there when strata searched each project of the tenant listing. None when the
/// profile document names no scope.
pub(crate) fn scope_line(r: &ConsumersReport) -> Option<String> {
    let scope = r.profile.scope?;
    if !scope.is_shared() {
        let project = match name_or_uid(&r.profile.project_name, &r.profile.project_uid) {
            "" => "its own project".to_string(),
            project => format!("project {project}"),
        };
        return Some(format!(
            "scope: {scope} profile, so only {project} can use it"
        ));
    }
    let line = format!("scope: {scope} profile, usable in every project");
    Some(match &r.projects_read {
        Some(p) => format!(
            "{line}; {} of {} {} searched",
            p.read.saturating_sub(p.unread.len()),
            p.read,
            plural(p.read, "project", "projects")
        ),
        None => line,
    })
}

/// `NOT CHECKED: qa and staging (this key cannot read them)`. It names each project that can use
/// the version and that strata could not search. None when strata searched each project that it
/// knows of, or searched no project one by one.
///
/// A project that Palette refused with a denial is one that the key cannot read. A project whose
/// search failed in another way gives the message of the error. A key that cannot search at
/// tenant scope gets only the projects of its roles in the listing. So the line then also names
/// each project outside the listing.
pub(crate) fn not_checked_line(r: &ConsumersReport) -> Option<String> {
    let read = r.projects_read.as_ref()?;
    let (denied, failed): (Vec<&UnreadProject>, Vec<&UnreadProject>) =
        read.unread.iter().partition(|p| p.denied);
    let mut parts = Vec::new();
    if !denied.is_empty() {
        let names: Vec<&str> = denied
            .iter()
            .map(|p| name_or_uid(&p.name, &p.uid))
            .collect();
        parts.push(format!(
            "{} (this key cannot read {})",
            and_list(&names),
            plural(names.len(), "it", "them")
        ));
    }
    for p in failed {
        let name = name_or_uid(&p.name, &p.uid);
        parts.push(format!("{name} ({})", error_message(&p.error)));
    }
    if read.tenant_error.is_some() {
        parts.push("each project that this key cannot list".to_string());
    }
    if let Some(e) = &read.error {
        parts.push(format!("each project of the tenant ({})", error_message(e)));
    }
    (!parts.is_empty()).then(|| format!("NOT CHECKED: {}", parts.join("; ")))
}
