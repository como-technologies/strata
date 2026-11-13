//! The lines of `list` and `packs`, and of a set of records.

use super::notes::marks_invalid;
use super::verify::record_label;
use super::{count_of, count_of_noun, entry_error, joined_note};
use crate::closed_set::{ClosedSet, Open};
use crate::diff::invalid_packs;
use crate::ops::{Listing, PackListing, Resolved, SetRecord, SetReport};
use crate::snapshot::Snapshot;
use crate::text::{and_list, dash, name_or_uid};

/// The profile type of a listed profile as a cell of a report: `cluster` or `add-on`, and `-`
/// when the listing has no type.
pub(crate) fn profile_type_cell(pr: &crate::ops::ListedProfile) -> &str {
    dash(pr.profile_type.as_ref().map_or("", Open::as_str))
}

/// `env=prod, team`: the labels of a cluster, with a tag that has no value by its key alone.
pub(crate) fn labels_text(labels: &std::collections::BTreeMap<String, String>) -> String {
    labels
        .iter()
        .map(|(k, v)| {
            if v == crate::ops::TAG_WITHOUT_VALUE {
                k.clone()
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `the list holds no virtual cluster: Palette has no search, or its search takes no
/// environment`, for a list of clusters whose Palette has no search for the virtual clusters.
pub(crate) fn virtual_line(l: &Listing) -> Option<String> {
    (l.includes_virtual == Some(false)).then(|| {
        "the list holds no virtual cluster: Palette has no search, or its search takes no \
         environment"
            .to_string()
    })
}

/// `group dev-group`: the cluster group of a virtual cluster, by name, or by uid when Palette
/// gives no name. None when Palette names no group.
pub(crate) fn virtual_group(v: &crate::ops::VirtualCluster) -> Option<String> {
    let group = v.cluster_group.as_ref()?;
    Some(format!("group {}", name_or_uid(&group.name, &group.uid)))
}

/// `virtual (group dev-group)`, or `virtual` when Palette names no group: what the row of a
/// virtual cluster ends with. None for each other cluster.
pub(crate) fn virtual_note(c: &crate::ops::ListedCluster) -> Option<String> {
    let v = c.virtual_cluster.as_ref()?;
    Some(match virtual_group(v) {
        Some(group) => format!("virtual ({group})"),
        None => "virtual".to_string(),
    })
}

/// `the clusters with env=prod and the saved filter prod (region=us), from the search of
/// Palette`, for a listing with a query.
pub(crate) fn query_line(l: &Listing) -> Option<String> {
    let q = l.query.as_ref()?;
    let mut parts: Vec<String> = q.tags.clone();
    if let Some(name) = &q.filter {
        let expr = l
            .query_filter
            .as_ref()
            .map(|f| format!(" ({})", f.expression()))
            .unwrap_or_default();
        parts.push(format!("the saved filter {name}{expr}"));
    }
    let how = match l.resolved {
        Some(r @ Resolved::Search) => format!("from the {}, at tenant scope", r.noun()),
        Some(r @ Resolved::Listing) => {
            format!("from the {}, labels matched by strata", r.noun())
        }
        None => String::new(),
    };
    Some(format!("the clusters with {}, {how}", and_list(&parts)))
}

/// The words of a report for the scope of the tenant, which has no project.
const TENANT_SCOPE_TEXT: &str = "tenant scope";

/// `project lab (uid p1)`, `project p1` when the name is unknown, or `tenant scope`.
pub(crate) fn list_scope(l: &Listing) -> String {
    if l.project_uid.is_empty() {
        TENANT_SCOPE_TEXT.to_string()
    } else {
        format!("project {}", project_label(&l.project_uid, &l.project_name))
    }
}

/// The projects of two sides, by uid, when the two differ, such as the two records of a
/// comparison. A side with no project is `tenant scope`.
///
/// A record names its project by uid alone, so a comparison of two files names no project by
/// name. `strata list projects` names each uid.
pub(crate) fn side_projects<'a>(a: &'a str, b: &'a str) -> Option<(&'a str, &'a str)> {
    let shown = |uid: &'a str| {
        if uid.is_empty() {
            TENANT_SCOPE_TEXT
        } else {
            uid
        }
    };
    (a != b).then(|| (shown(a), shown(b)))
}

/// A cluster's project, in the shape the scope line uses: `dev (uid 64f2...)`, or the uid alone
/// when the name could not be read. A uid names no project to a reader.
pub(crate) fn project_label(uid: &str, name: &str) -> String {
    match (uid, name) {
        ("", _) => dash("").to_string(),
        (uid, "") => uid.to_string(),
        (uid, name) => format!("{name} (uid {uid})"),
    }
}

/// What a packs heading adds for a registry. It names the state that all its versions share.
/// It says `no tags` when Palette answered 404 for the tags, and `no digests` when no version
/// has a digest.
fn registry_note(r: &crate::ops::PackRegistry) -> Option<String> {
    let tags = match (&r.state, r.tags_unavailable) {
        (_, false) => None,
        (Some(_), true) => Some("no tags"),
        (None, true) => Some("no tags: the registry answered 404"),
    };
    joined_note([
        r.state.as_ref().map(Open::as_str),
        tags,
        r.no_digests.then_some("no digests"),
    ])
}

/// `csi-aws-ebs in Public Repo (2 of 12 versions, deprecated)`: the heading of the versions that
/// one registry offers.
///
/// The heading states what the registry holds. A flag that cuts the list says so, so that a
/// reader never takes the count for the inventory of the registry. `strong` puts the name of the
/// pack and of the registry in the form of the report.
pub(crate) fn registry_heading(
    l: &PackListing,
    r: &crate::ops::PackRegistry,
    strong: impl Fn(&str) -> String,
) -> String {
    let offered = count_of(r.versions_offered, "version");
    let count = if r.versions.len() == r.versions_offered {
        offered
    } else {
        format!("{} of {offered}", r.versions.len())
    };
    let note = registry_note(r)
        .map(|n| format!(", {n}"))
        .unwrap_or_default();
    format!(
        "{} in {} ({count}{note})",
        strong(&l.name),
        strong(&r.registry)
    )
}

/// What a packs row adds for a version. It names the state, and says `no digest` when the
/// registry reports none. It says `latest in Palette` when Palette calls the version the latest
/// and strata does not. A version that both call the latest needs no mark.
pub(crate) fn version_note(v: &crate::ops::PackBuild) -> Option<String> {
    joined_note([
        v.state.as_ref().map(Open::as_str),
        v.digest.is_empty().then_some("no digest"),
        (v.palette_latest && !v.latest).then_some("latest in Palette"),
    ])
}

/// The one line of a listing that keeps no registry, or None when a registry has the pack.
///
/// `no version of csi-aws-ebs supports edge-native` when `--cloud` left the listing empty.
/// `Palette serves no version of csi-aws-ebs` with no `--cloud`: the registry gives no version a
/// tag, so strata shows none.
pub(crate) fn no_version_sentence(l: &PackListing) -> Option<String> {
    if !l.registries.is_empty() {
        return None;
    }
    Some(match &l.cloud {
        Some(cloud) => format!("no version of {} supports {cloud}", l.name),
        None => format!("Palette serves no version of {}", l.name),
    })
}

/// `2 profiles`, or, with the `--pack` filter, `2 profiles with a pack matching 'cilium'`.
pub(crate) fn list_count(n: usize, singular: &str, pack: Option<&str>) -> String {
    let count = count_of(n, singular);
    match pack {
        Some(text) => format!("{count} with a pack matching '{text}'"),
        None => count,
    }
}

/// `2 registries`, `1 registry`, or `no registries`: the heading of the registries of a listing.
pub(crate) fn registry_count(n: usize) -> String {
    count_of_noun(n, "registry", "registries")
}

/// The kind of a listed registry as a cell of a report: `pack`, `helm`, or `oci`, and `-` when
/// the listing names no kind.
pub(crate) fn registry_kind_cell(r: &crate::ops::ListedRegistry) -> &str {
    dash(r.kind.as_ref().map_or("", Open::as_str))
}

/// `2 clusters`, or, with the `--cluster` filter, `2 clusters matching 'payments'`.
pub(crate) fn cluster_count(n: usize, text: Option<&str>) -> String {
    let count = count_of(n, "cluster");
    match text {
        Some(text) => format!("{count} matching '{text}'"),
        None => count,
    }
}

/// A listed pack's version, with the tag when the profile pins another name for it:
/// `24.04 (tag LTS__24.x)`.
pub(crate) fn pack_version(pk: &crate::ops::ListedPack) -> String {
    if pk.tag.is_empty() || pk.tag == pk.version {
        pk.version.clone()
    } else {
        format!("{} (tag {})", pk.version, pk.tag)
    }
}

/// The registry that a profile takes a listed pack from: its name, or its uid when the key cannot
/// see the registry. `None` for a pack with no registry.
pub(crate) fn pack_registry(pk: &crate::ops::ListedPack) -> Option<&str> {
    Some(name_or_uid(&pk.registry, &pk.registry_uid)).filter(|r| !r.is_empty())
}

/// `2 of 3 clusters recorded, 1 removed; found by the search of Palette`: the counts of a set.
pub fn set_counts_line(r: &SetReport) -> String {
    let c = &r.summary;
    format!(
        "{} of {} clusters recorded, {} removed; found by the {}",
        c.recorded,
        c.clusters,
        c.removed,
        r.resolved.noun()
    )
}

/// One warning for each pack of `snap` that Palette marks invalid, with the reason when Palette
/// gives one. The root does not cover the flag, so the record alone does not show it to a person.
pub fn invalid_pack_warnings(snap: &Snapshot) -> Vec<String> {
    invalid_packs(snap)
        .iter()
        .map(|p| {
            format!(
                "{}. The record keeps the flag, outside the root",
                marks_invalid(p, "")
            )
        })
        .collect()
}

/// Why strata could not record a cluster of a set: the message of its error.
pub(crate) fn set_error(r: &SetRecord) -> &str {
    entry_error(r.error.as_ref()).unwrap_or("see the error")
}

/// The mark of a listed version that nobody published: `draft`, or nothing.
fn draft_mark(p: &crate::ops::ListedProfile) -> Option<&'static str> {
    p.is_draft().then_some("draft")
}

/// The note of a cluster of `list --applied` that has no profile, when the listing says why.
///
/// A person imported the cluster, and Palette applies no add-on profile to an imported cluster
/// in read-only mode. `None` for a cluster with a profile, one that strata did not read, or one
/// that Palette built.
pub(crate) fn imported_note(c: &crate::ops::ListedCluster) -> Option<&'static str> {
    let no_profile = c.profiles.as_ref().is_some_and(Vec::is_empty);
    (no_profile && c.is_imported())
        .then_some("imported: Palette applies no add-on profile in read-only mode")
}

/// The marks of a listed version, joined with commas: its scope when it is not the scope of the
/// listing, for example `tenant` in a project, and `draft` for a version that nobody published.
/// None when the row needs no mark.
pub(crate) fn profile_marks(
    p: &crate::ops::ListedProfile,
    l: &crate::ops::Listing,
) -> Option<String> {
    let scope = p
        .scope
        .filter(|scope| *scope != l.scope())
        .map(crate::model::scope::ProfileScope::as_str);
    joined_note([scope, draft_mark(p)])
}

/// `<name>@<version>` of a profile a cluster applies, as `list` and the reports print it.
pub(crate) fn applied_ref(a: &crate::snapshot::AppliedProfile) -> String {
    record_label(&a.name, &a.version)
}
