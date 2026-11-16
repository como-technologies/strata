//! The search of each project, for a version that each project can use: a profile of the tenant
//! or of the system.
//!
//! The cluster search and the edge host search at tenant scope give only the projects where a
//! role of the key gives the listing. In a project where no role gives it, Palette refuses the
//! search of that project with a denial, such as `ResourceAccessDenied`. So strata also searches
//! each project of the tenant listing, and names each project that it could not search.

use std::collections::{BTreeSet, HashMap};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Scopes;
use super::reads::{Candidates, listed_hosts, search_or_listing};
use crate::api::{Palette, PaletteApi};
use crate::error::{Error, ErrorDocument, Result};
use crate::json::text;
use crate::ops::client_for;
use crate::ops::list::{ListedCluster, ListedProject, ProjectListings};
use crate::term::progress::Progress;

/// A project that can use the version, where strata could not search.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnreadProject {
    /// The project uid.
    pub uid: String,
    /// The project name, from the project listing of the tenant.
    pub name: String,
    /// True when Palette answered the search with a denial, such as `ResourceAccessDenied`: no
    /// role of the key gives the listing in the project. False when the search failed in another
    /// way.
    pub denied: bool,
    /// What Palette answered to the search, or why the search did not complete.
    #[schemars(with = "ErrorDocument")]
    pub error: Value,
}

/// The projects that strata searched one by one, for a version of the tenant or of the system.
///
/// Each project of the tenant can use such a version. The searches at tenant scope give only the
/// projects where a role of the key gives the listing.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsRead {
    /// The projects of the tenant listing that strata searched for the clusters, the virtual
    /// clusters, and the edge hosts, or could not search. For an app profile, strata listed their
    /// app deployments.
    pub read: usize,
    /// The projects of `read` where a search failed. A consumer in one of them can be missing.
    /// When the cluster search at tenant scope failed with an error that is not a denial, strata
    /// searched no project. Then each project of `read` is here with that error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<UnreadProject>,
    /// Why Palette refused the cluster search at tenant scope, when it did. Palette lists every
    /// project of the tenant only to a key whose role gives `project.list` in the tenant, and that
    /// search needs the same permission. So a project outside the listing is not checked. For an
    /// app profile, strata sends one page of the search only for this answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub tenant_error: Option<Value>,
    /// Why strata could not read the project listing of the tenant. Then strata searched no
    /// project one by one. With no client of the tenant, it is the error of that client: in a
    /// project, Palette lists that project alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
}

impl ProjectsRead {
    /// True when strata could not search a project that can use the version. Then a consumer can
    /// be missing from the answer. The project is in `unread`, or the key cannot list it, or
    /// strata could not read the project listing.
    pub fn incomplete(&self) -> bool {
        !self.unread.is_empty() || self.tenant_error.is_some() || self.error.is_some()
    }
}

/// What the searches of each project found: the clusters to read, the edge hosts, and the
/// projects that strata searched.
pub(super) struct EachProject {
    /// The clusters for the second reading: those of the search at tenant scope, and those of the
    /// search of each project.
    pub(super) candidates: Candidates,
    /// The edge hosts of the projects, each one time.
    pub(super) hosts: Vec<Value>,
    /// The projects that strata searched, and those that it could not.
    pub(super) projects: ProjectsRead,
}

/// Search the tenant and each project of the tenant listing for the clusters that can run a
/// version of the profile `name`. Search each project for its edge hosts too.
///
/// A search of a project that fails puts the project in [`ProjectsRead::unread`]. A denial of the
/// cluster search at tenant scope goes in [`ProjectsRead::tenant_error`], and strata still
/// searches each project that the key can list. Each other error of that search stops the second
/// reading, as it does for a profile of a project. Then each project of the listing is in
/// [`ProjectsRead::unread`] with that error.
pub(super) fn search_each_project<A, F>(
    name: &str,
    scopes: &Scopes<'_, A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
    listings: &mut ProjectListings,
    progress: &Progress,
) -> EachProject
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let Scopes { wide, home, .. } = *scopes;
    progress.note("searching the clusters that run the version");
    let mut projects = ProjectsRead::default();
    let mut found = Found::default();
    let Candidates { clusters, resolved } = search_or_listing(wide, name, progress);
    let stopped = match tenant_search(clusters, &mut projects) {
        Ok(clusters) => {
            found.clusters.extend(clusters);
            None
        }
        Err(e) => Some(e),
    };
    if let Some(listed) = tenant_listing(scopes, listings, &mut projects) {
        if stopped.is_none() {
            progress.note("searching each project of the tenant");
        }
        search_listed(listed, stopped.as_ref(), &mut projects, |project| {
            client_for(clients, home, &project.uid, connect)
                .and_then(|p| found.search(p, name, progress))
        });
    }
    EachProject {
        candidates: Candidates {
            clusters: stopped.map_or(Ok(found.clusters), Err),
            resolved,
        },
        hosts: found.hosts,
        projects,
    }
}

/// The answer `searched` of the cluster search at tenant scope, for the projects of `read`.
///
/// Palette refuses that search with a denial, such as `AccessDenied`, to a key whose role does not
/// give `project.list` in the tenant. The denial goes in [`ProjectsRead::tenant_error`], and the
/// answer is then empty: strata still searches each project that the key can list. Each other
/// error stays the answer, and it stops the search of each project.
pub(super) fn tenant_search<T: Default>(searched: Result<T>, read: &mut ProjectsRead) -> Result<T> {
    match searched {
        Err(e) if e.denial().is_some() => {
            read.tenant_error = Some(e.to_json());
            Ok(T::default())
        }
        searched => searched,
    }
}

/// Search each project of `listed`, the projects of the tenant listing, with `search`.
///
/// A project whose search fails goes in [`ProjectsRead::unread`] of `read`. When `stopped` holds
/// the error of the cluster search at tenant scope, strata searches no project. Then each project
/// of `listed` is in `unread` with that error.
pub(super) fn search_listed(
    listed: &[ListedProject],
    stopped: Option<&Error>,
    read: &mut ProjectsRead,
    mut search: impl FnMut(&ListedProject) -> Result<()>,
) {
    read.read = listed.len();
    for project in listed {
        let failed = match stopped {
            Some(e) => Some(unread_project(project, e)),
            None => search(project).err().map(|e| unread_project(project, &e)),
        };
        read.unread.extend(failed);
    }
}

/// The project listing of the tenant, through the client of the tenant. None when strata cannot
/// read it, and [`ProjectsRead::error`] of `read` then says why.
///
/// The error is that of the client of the tenant when strata has none and the client of the
/// profile is in a project. In a project, Palette lists that project alone.
pub(super) fn tenant_listing<'l, A: PaletteApi>(
    scopes: &Scopes<'_, A>,
    listings: &'l mut ProjectListings,
    read: &mut ProjectsRead,
) -> Option<&'l [ListedProject]> {
    let error = match scopes.no_tenant {
        Some(e) if scopes.wide.project_uid().is_some() => e.to_json(),
        _ => match listings.of(scopes.wide) {
            Ok(listed) => return Some(listed),
            Err(e) => e.to_json(),
        },
    };
    read.error = Some(error);
    None
}

/// The edge hosts of `hosts`, the answer of the edge host search, and the other hosts of `more`.
/// Each host is there one time.
///
/// # Errors
///
/// The error of `hosts`.
pub(super) fn joined_hosts(hosts: Result<Vec<Value>>, more: Vec<Value>) -> Result<Vec<Value>> {
    let mut hosts = hosts?;
    let mut seen: BTreeSet<String> = hosts.iter().map(|h| text(h, "/metadata/uid")).collect();
    hosts.extend(
        more.into_iter()
            .filter(|h| seen.insert(text(h, "/metadata/uid"))),
    );
    Ok(hosts)
}

/// The clusters and the edge hosts that the searches of the projects found.
#[derive(Default)]
struct Found {
    clusters: Vec<ListedCluster>,
    hosts: Vec<Value>,
}

impl Found {
    /// Search the scope of `palette`, one project, for the clusters that can run a version of the
    /// profile `name`, and for its edge hosts. A Palette without the edge host search has no
    /// edge hosts. A cluster or a host that two searches give is one time in the second reading.
    ///
    /// # Errors
    ///
    /// The error of the cluster search, of the listing, or of the edge host search. Then the
    /// project gives nothing.
    fn search<A: PaletteApi>(
        &mut self,
        palette: &Palette<A>,
        name: &str,
        progress: &Progress,
    ) -> Result<()> {
        let clusters = search_or_listing(palette, name, progress).clusters?;
        let hosts = listed_hosts(palette)?;
        self.clusters.extend(clusters);
        self.hosts = joined_hosts(Ok(std::mem::take(&mut self.hosts)), hosts)?;
        Ok(())
    }
}

/// The project `project`, which strata could not search, with the error `e`.
pub(super) fn unread_project(project: &ListedProject, e: &Error) -> UnreadProject {
    UnreadProject {
        uid: project.uid.clone(),
        name: project.name.clone(),
        denied: e.denial().is_some(),
        error: e.to_json(),
    }
}
