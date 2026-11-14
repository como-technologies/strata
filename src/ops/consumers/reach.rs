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

use super::reads::{Candidates, search_or_listing};
use crate::api::{Palette, PaletteApi};
use crate::error::{Error, ErrorDocument, Result, served};
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
    /// clusters, and the edge hosts.
    pub read: usize,
    /// The projects of `read` where a search failed. A consumer in one of them can be missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<UnreadProject>,
    /// Why Palette refused the cluster search at tenant scope, when it did. Palette lists every
    /// project of the tenant only to a key whose role gives `project.list` in the tenant, and that
    /// search needs the same permission. So a project outside the listing is not checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub tenant_error: Option<Value>,
    /// Why strata could not read the project listing of the tenant. Then strata searched no
    /// project one by one.
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
/// `wide` is the client of the tenant, and `home` the client of the scope of the key. A search of
/// a project that fails puts the project in [`ProjectsRead::unread`]. A denial of the cluster
/// search at tenant scope goes in [`ProjectsRead::tenant_error`], and strata still searches each
/// project that the key can list. Each other error of that search stops the second reading, as
/// it does for a profile of a project.
#[allow(clippy::too_many_arguments)]
pub(super) fn search_each_project<A, F>(
    name: &str,
    wide: &Palette<A>,
    home: &Palette<A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
    listings: &mut ProjectListings,
    progress: &Progress,
) -> EachProject
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    progress.note("searching the clusters that run the version");
    let mut projects = ProjectsRead::default();
    let mut found = Found::default();
    let Candidates { clusters, resolved } = search_or_listing(wide, name, progress);
    match clusters {
        Ok(clusters) => found.clusters.extend(clusters),
        Err(e) if e.denial().is_some() => projects.tenant_error = Some(e.to_json()),
        Err(e) => {
            return EachProject {
                candidates: Candidates {
                    clusters: Err(e),
                    resolved,
                },
                hosts: Vec::new(),
                projects,
            };
        }
    }
    match listings.of(wide) {
        Ok(listed) => {
            progress.note("searching each project of the tenant");
            for project in listed {
                projects.read += 1;
                let searched = client_for(clients, home, &project.uid, connect)
                    .and_then(|p| found.search(p, name, progress));
                if let Err(e) = searched {
                    projects.unread.push(unread_project(project, &e));
                }
            }
        }
        Err(e) => projects.error = Some(e.to_json()),
    }
    EachProject {
        candidates: Candidates {
            clusters: Ok(found.clusters),
            resolved,
        },
        hosts: found.hosts,
        projects,
    }
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
        let hosts = served(palette.edge_hosts())?.unwrap_or_default();
        self.clusters.extend(clusters);
        self.hosts = joined_hosts(Ok(std::mem::take(&mut self.hosts)), hosts)?;
        Ok(())
    }
}

/// The project `project`, which strata could not search, with the error `e`.
fn unread_project(project: &ListedProject, e: &Error) -> UnreadProject {
    UnreadProject {
        uid: project.uid.clone(),
        name: project.name.clone(),
        denied: e.denial().is_some(),
        error: e.to_json(),
    }
}
