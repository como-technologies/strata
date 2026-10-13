//! The reads beside the consumer list: the edge hosts that bind the version, the search for the
//! clusters that run it, and the members of each cluster group.

use std::collections::{HashMap, HashSet};

use super::{ClustersRead, Consumer, ConsumerKind, EdgeHostsRead};
use crate::api::{
    Palette, PaletteApi, SEARCH_CLUSTER_PROFILES, search_condition, search_filter, search_group,
};
use crate::conjunction::Conjunction;
use crate::error::{Result, served};
use crate::json::{items, text};
use crate::ops::client_for;
use crate::ops::fleet::Resolved;
use crate::ops::list::{ListedCluster, list_clusters};
use crate::snapshot::{AppliedProfile, SubjectKind};
use crate::term::progress::Progress;

/// Add each edge host that the profile `uid` is bound to. The search is read through `wide`, and
/// through `home` when `wide` cannot read it. A Palette without the search has no edge hosts.
pub(super) fn edge_hosts_bound<A: PaletteApi>(
    list: &mut Vec<Consumer>,
    uid: &str,
    wide: &Palette<A>,
    home: &Palette<A>,
    progress: &Progress,
) -> EdgeHostsRead {
    progress.note("reading each edge host for the profiles bound to it");
    let hosts = match served(wide.edge_hosts().or_else(|_| home.edge_hosts())) {
        Ok(hosts) => hosts.unwrap_or_default(),
        Err(e) => {
            return EdgeHostsRead {
                error: Some(e.to_json()),
                ..EdgeHostsRead::default()
            };
        }
    };
    for host in &hosts {
        if let Some(bound) = AppliedProfile::template_of(host, uid) {
            let mut c = Consumer::unread(
                ConsumerKind::EdgeHost,
                text(host, "/metadata/uid"),
                String::new(),
                String::new(),
            );
            c.fill_from_document(host);
            // the search gives a summary of each template. The summary names the profile
            // version `version`, where the document of a cluster names it `profileVersion`
            c.version = text(bound, "/version");
            list.push(c);
        }
    }
    EdgeHostsRead {
        read: hosts.len(),
        ..EdgeHostsRead::default()
    }
}

/// The clusters that run a version of the profile `name`, from the cluster search of `palette`:
/// one search with `clusterProfiles eq <name>`, and one with `environment eq nested` too, since the
/// search leaves out each virtual cluster unless a condition asks for it. Palette takes the name of
/// a profile there, and not its uid: it changes each name into the uids of every profile with that
/// name, with no scope, and a uid matches no name (#181). The search of the clusters then holds to
/// the scope of the request. So the answer holds each cluster in that scope that runs a version of
/// the name, and the caller keeps the clusters that run the uid.
/// None when the search of Palette takes no `clusterProfiles`, or when the profile has no name.
/// The second search goes out only when the search takes `environment`: see
/// [`Palette::cluster_search_with_virtual`].
fn searched_consumers<A: PaletteApi>(
    palette: &Palette<A>,
    name: &str,
) -> Result<Option<Vec<ListedCluster>>> {
    if name.is_empty() {
        return Ok(None);
    }
    let properties = palette.cluster_search_properties()?;
    if !properties.contains(SEARCH_CLUSTER_PROFILES) {
        return Ok(None);
    }
    let body = search_filter(vec![search_group(
        Conjunction::And,
        vec![search_condition(
            SEARCH_CLUSTER_PROFILES,
            &[name.to_string()],
            false,
        )],
    )]);
    let found = palette.cluster_search_with_virtual(&properties, &body, || {})?;
    Ok(Some(list_clusters(&found.items)))
}

/// The second reading of `consumers`. Strata asks the cluster search for the clusters that run a
/// version of the profile `name`, and reads the cluster listing when the search takes no profile.
/// The search goes to the scope of the profile: its project `project`, where each cluster that can
/// run it is, or `wide` for a tenant profile. Then `home` when that
/// scope cannot answer. The listing is read through `wide`, and through `home` when `wide` cannot
/// read it. A cluster the consumer list named is not read again. A cluster that applies `uid`
/// joins `list`, marked `unlisted`, with what its document and the search or the listing say of it.
#[allow(clippy::too_many_arguments)]
pub(super) fn second_reading<A, F>(
    list: &mut Vec<Consumer>,
    uid: &str,
    name: &str,
    project: &str,
    wide: &Palette<A>,
    home: &Palette<A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
    progress: &Progress,
) -> ClustersRead
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    progress.note("searching the clusters that run the version");
    let searched = {
        let own = if project.is_empty() {
            Ok(wide)
        } else {
            client_for(clients, home, project, connect)
        };
        match own {
            Ok(own) => searched_consumers(own, name),
            Err(e) => Err(e),
        }
        .or_else(|_| searched_consumers(home, name))
    };
    let (candidates, resolved) = match searched {
        Ok(Some(found)) => (Ok(found), Resolved::Search),
        Ok(None) => {
            progress.note("reading each cluster for the profiles it applies");
            let listing = wide.cluster_listing().or_else(|_| home.cluster_listing());
            (listing.map(|l| list_clusters(items(&l))), Resolved::Listing)
        }
        Err(e) => (Err(e), Resolved::Search),
    };
    let candidates = match candidates {
        Ok(candidates) => candidates,
        Err(e) => {
            return ClustersRead {
                resolved: Some(resolved),
                error: Some(e.to_json()),
                ..ClustersRead::default()
            };
        }
    };
    let mut clusters_read = ClustersRead {
        resolved: Some(resolved),
        ..ClustersRead::default()
    };
    // the virtual clusters of each cluster group that pins the version (#184)
    progress.note("reading the virtual clusters of the cluster groups that pin the version");
    let members = group_members(wide, home, uid, &mut clusters_read);
    let mut seen: HashSet<String> = HashSet::new();
    let tagged = candidates
        .into_iter()
        .map(|c| (c, false))
        .chain(members.into_iter().map(|c| (c, true)));
    for (listed, of_group) in tagged {
        if list.iter().any(|c| c.uid == listed.uid) || !seen.insert(listed.uid.clone()) {
            continue;
        }
        if of_group {
            clusters_read.members += 1;
        }
        let doc = client_for(clients, home, &listed.project_uid, connect)
            .and_then(|p| p.subject_document(SubjectKind::Cluster, &listed.uid));
        let doc = match doc {
            Ok(doc) => doc,
            Err(_) => {
                clusters_read.unread += 1;
                if of_group {
                    clusters_read.members_unread += 1;
                }
                continue;
            }
        };
        clusters_read.read += 1;
        if let Some(copy) = AppliedProfile::applied(&doc, uid) {
            let mut c = Consumer {
                state: listed.state,
                unlisted: true,
                ..Consumer::unread(
                    ConsumerKind::Cluster,
                    listed.uid,
                    listed.name,
                    listed.project_uid,
                )
            };
            c.fill_from_document(&doc);
            c.version = copy.version;
            list.push(c);
        }
    }
    clusters_read
}

/// The virtual clusters of each cluster group that pins `uid`, for the second reading (#184). The
/// cluster search does not name a virtual cluster that runs the version through its group: the
/// search reads a summary of each cluster, and the summary of such a virtual cluster does not
/// name the profiles of its group. So strata lists the cluster groups of the tenant (`wide`) and
/// of the scope of the key (`home`), keeps each group whose `spec.clusterProfileTemplates` holds
/// `uid`, and lists its virtual clusters, in the scope that listed the group, and in the scope of
/// the key when that scope cannot read the group. A Palette that has
/// no cluster groups answers the listing with `404` or `405`: no group. The count of the groups
/// and an error go into `read`.
fn group_members<A: PaletteApi>(
    wide: &Palette<A>,
    home: &Palette<A>,
    uid: &str,
    read: &mut ClustersRead,
) -> Vec<ListedCluster> {
    let mut groups: Vec<(&Palette<A>, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut failed = None;
    let mut answered = false;
    for scope in [wide, home] {
        match served(scope.cluster_groups()) {
            Ok(listed) => {
                answered = true;
                for (_, group) in listed.into_iter().flatten() {
                    if seen.insert(group.clone()) {
                        groups.push((scope, group));
                    }
                }
            }
            Err(e) => failed = Some(e.to_json()),
        }
    }
    if !answered {
        read.groups_error = failed;
        return Vec::new();
    }
    let mut members = Vec::new();
    for (scope, group) in groups {
        // a group that the tenant lists can be one that only the scope of the key can read
        let doc = match scope
            .subject_document(SubjectKind::ClusterGroup, &group)
            .or_else(|_| home.subject_document(SubjectKind::ClusterGroup, &group))
        {
            Ok(doc) => doc,
            Err(e) => {
                read.groups_error.get_or_insert(e.to_json());
                continue;
            }
        };
        if AppliedProfile::template_of(&doc, uid).is_none() {
            continue;
        }
        read.groups += 1;
        match scope
            .cluster_group_members(&group)
            .or_else(|_| home.cluster_group_members(&group))
        {
            Ok(found) => members.extend(found),
            Err(e) => {
                read.groups_error.get_or_insert(e.to_json());
            }
        }
    }
    list_clusters(&members)
}
