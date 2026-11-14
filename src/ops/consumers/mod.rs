//! `consumers`: the clusters, edge hosts, and cluster templates that use a profile version, and
//! the app deployments of an app profile.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client_for;
use super::consumer_list::consumer_uids;
use super::fleet::Resolved;
use super::list::{ProjectListings, TENANT_SCOPE, name_projects_of};
use crate::api::{Palette, PaletteApi};
use crate::error::{ErrorDocument, Result};
use crate::json::{arr, first_nonempty, is_zero, str_at, text};
use crate::model::cluster_template::ClusterTemplateRef;
use crate::model::profile_type::ProfileType;
use crate::model::scope::ProfileScope;
use crate::reference::Ref;
use crate::snapshot::{AppliedProfile, SubjectKind, Tool};
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;
use reach::{joined_hosts, search_each_project};
use reads::{candidate_clusters, edge_host_search, edge_hosts_bound, second_reading};

mod app;
mod reach;
mod reads;
mod target;

use app::app_consumers;
pub use reach::{ProjectsRead, UnreadProject};
use target::{Target, target_of, uid_of_ref};

/// Schema id of the document `consumers` writes.
pub const CONSUMERS_SCHEMA: &str = "strata-consumers/1";

/// The state a consumer shows when its cluster could not be read.
pub const UNKNOWN_STATE: &str = "unknown";

/// The state of an edge host that a cluster uses, as Palette gives it in `status.state`.
pub(crate) const IN_USE_STATE: &str = "in-use";

/// The state of a cluster group that Palette marks active, as strata names it.
pub(crate) const ACTIVE_STATE: &str = "active";

/// The state of a cluster group that Palette does not mark active, as strata names it.
pub(crate) const INACTIVE_STATE: &str = "inactive";

/// The kind of a consumer of a profile version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConsumerKind {
    /// `cluster`: a cluster that runs the version.
    #[default]
    Cluster,
    /// `edgehost`: an edge host that the profile is bound to.
    EdgeHost,
    /// `template`: a cluster template that links the version.
    Template,
    /// `app`: an app deployment of an app profile.
    App,
}

impl ConsumerKind {
    /// The noun that names the kind in a report.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Cluster => "cluster",
            Self::EdgeHost => "edge host",
            Self::Template => "cluster template",
            Self::App => "app deployment",
        }
    }
}

/// Why an app profile has no second reading: Palette lists app deployments by name, cluster, and
/// tag, and not by profile.
pub const APP_NO_SECOND_READING: &str =
    "an app profile: the app deployment listing filters no profile";

/// Why an app profile has no edge hosts: Palette binds a cluster profile to an edge host, and
/// never an app profile.
pub const APP_NO_EDGE_HOSTS: &str = "an app profile: no edge host binds one";

/// The profile version whose consumers are listed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumedProfile {
    /// Profile uid.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// `app` for an app profile; absent for a cluster profile.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ProfileType>,
    /// `project`, `tenant`, or `system`, from `metadata.annotations.scope` of the profile. Only
    /// the clusters of its project can use a profile of a project. Each project can use a profile
    /// of the tenant or of the system. Absent when the document does not give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ProfileScope>,
    /// The project of a profile of a project. Empty for each other scope.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_uid: String,
    /// That project's name, when strata could read the project listing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
}

/// One cluster, edge host, cluster template, or app deployment that uses the profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Consumer {
    /// `cluster`, `edgehost` for an edge host that the profile is bound to, `template` for a
    /// cluster template that links the version, or `app` for an app deployment of an app profile.
    #[serde(default)]
    pub kind: ConsumerKind,
    /// Cluster uid, edge host uid, cluster template uid, or app deployment uid.
    pub uid: String,
    /// Cluster name; empty when neither the profile nor the cluster could name it.
    pub name: String,
    /// The project the consumer is in; empty for tenant scope or when unknown.
    pub project_uid: String,
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The cluster's `status.state`, as Palette gives it in `v1ClusterState`, or `unknown` when
    /// strata could not read it.
    /// Empty for a cluster template: a template has no state.
    pub state: String,
    /// The version the cluster's own copy of the profile names, from the cluster document; empty
    /// when the cluster could not be read, and for a cluster template, which keeps no copy. For an
    /// app deployment, `spec.profile.metadata.version` of the deployment. It differs from
    /// `profile.version` when the version the cluster runs was overwritten in place, by a legacy version bump or a rename.
    #[serde(default)]
    pub version: String,
    /// Why the cluster could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
    /// True when the consumer list of the profile did not name this cluster, and the cluster
    /// document did: the second reading found it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unlisted: bool,
}

impl Consumer {
    /// A consumer whose document strata did not read yet: its state is `unknown`, and it names no
    /// version.
    fn unread(kind: ConsumerKind, uid: String, name: String, project_uid: String) -> Consumer {
        Consumer {
            kind,
            uid,
            name,
            project_uid,
            project_name: String::new(),
            state: UNKNOWN_STATE.to_string(),
            version: String::new(),
            error: None,
            unlisted: false,
        }
    }

    /// Take the name, the project, and the state from the document of the consumer. A field
    /// that the document does not give keeps its value.
    fn fill_from_document(&mut self, doc: &Value) {
        self.name = first_nonempty(&[str_at(doc, "/metadata/name"), &self.name]);
        self.project_uid = first_nonempty(&[
            str_at(doc, "/metadata/annotations/projectUid"),
            &self.project_uid,
        ]);
        self.state = first_nonempty(&[str_at(doc, "/status/state"), &self.state]);
    }

    /// Read the document of the consumer in its own project, and take from it the name, the
    /// project, the state, and the version.
    ///
    /// `document` reads the document at the uid of the consumer. `version` reads the version of
    /// the profile in that document. `error` holds an error of the read, and the consumer keeps
    /// what the consumer list gave it.
    fn read<A, F>(
        &mut self,
        clients: &mut HashMap<Option<String>, Palette<A>>,
        home: &Palette<A>,
        connect: &mut F,
        document: impl FnOnce(&Palette<A>, &str) -> Result<Value>,
        version: impl FnOnce(&Value) -> String,
    ) where
        A: PaletteApi,
        F: FnMut(Option<&str>) -> Result<Palette<A>>,
    {
        let doc = client_for(clients, home, &self.project_uid, connect)
            .and_then(|palette| document(palette, &self.uid));
        match doc {
            Ok(d) => {
                self.fill_from_document(&d);
                self.version = version(&d);
            }
            Err(e) => self.error = Some(e.to_json()),
        }
    }
}

/// The second reading of `consumers`: the clusters that run the version.
///
/// They come from the cluster search of Palette, or from each cluster the key can list, each read
/// for the profiles it applies. Palette computes the consumer list of a profile when it is asked,
/// in the projects the key can read. It gives an empty list on an error too. The second reading
/// finds a consumer that the list omits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClustersRead {
    /// How strata found the clusters to read: `search`, the cluster search with the profile name,
    /// which holds the virtual clusters too; or `listing`, each cluster of the cluster listing,
    /// when the search of Palette takes no `clusterProfiles`. Absent when the second reading was
    /// not done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolved>,
    /// Cluster documents read: each cluster that the search or the listing names and the consumer
    /// list did not.
    pub read: usize,
    /// Clusters that the search or the listing names that could not be read. Each one can be a
    /// consumer.
    pub unread: usize,
    /// Why the search or the listing could not be read, when it could not. Then `read` is 0, and
    /// the consumer list of the profile stands alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
    /// True when the second reading was not done, as `--list-only` asks, or for an app profile.
    /// Then the consumer list of the profile stands alone, and an empty list can be an error of
    /// Palette.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
    /// Why the second reading was not done, when `--list-only` did not ask: for an app profile.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// The cluster groups that pin the version (#184). The cluster search does not name a virtual
    /// cluster that runs the version through its group, so strata reads the virtual clusters of
    /// each such group.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub groups: usize,
    /// The virtual clusters of those groups that strata read: each one that the consumer list and
    /// the search did not name. Each one also counts in `read` or `unread`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub members: usize,
    /// The virtual clusters of `members` that strata could not read. They also count in `unread`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub members_unread: usize,
    /// The virtual clusters of `members` that run the version. Each one is in `consumers`, marked
    /// `unlisted`. The other consumers marked `unlisted` come from the search or the listing.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub members_found: usize,
    /// Why strata could not read the cluster groups, or one of them, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub groups_error: Option<Value>,
}

impl ClustersRead {
    /// True when a reading failed, so a consumer can be missing from the answer. The reading
    /// that failed is the search or the listing, a cluster group or its virtual clusters, or a
    /// cluster that one of them names. A second reading that was not done, as `--list-only`
    /// asks or for an app profile, is not a failed reading.
    pub fn incomplete(&self) -> bool {
        self.error.is_some() || self.groups_error.is_some() || self.unread > 0
    }
}

/// The edge hosts that `consumers` read. The consumer list of a profile names clusters only, so
/// strata reads each edge host the key can list for the profiles bound to it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EdgeHostsRead {
    /// Edge hosts listed.
    pub read: usize,
    /// Why the edge hosts could not be listed, when they could not. Then no edge host counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
    /// True when strata did not search the edge hosts: for an app profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
    /// Why strata did not search the edge hosts, when it did not.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

impl EdgeHostsRead {
    /// True when the edge hosts could not be listed, so an edge host that the version is bound
    /// to can be missing from the answer. A search that was not done, for an app profile, is not
    /// a failed reading.
    pub fn incomplete(&self) -> bool {
        self.error.is_some()
    }
}

/// What `consumers` reads.
#[derive(Debug, Clone, Default)]
pub struct ConsumersOptions {
    /// Read the consumer list that Palette computes, and not each cluster of the fleet.
    pub list_only: bool,
}

/// The document `consumers` writes: `strata-consumers/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConsumersReport {
    /// The schema id of this document
    #[schemars(extend("const" = CONSUMERS_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    #[schemars(with = "Rfc3339")]
    pub generated_at: String,
    /// The profile version.
    pub profile: ConsumedProfile,
    /// The number of clusters, edge hosts, and cluster templates that use it. Exit status 1 when
    /// it is not zero. Exit status 0 when it is zero and each reading succeeded, and 2 when it
    /// is zero and a reading failed: see [`ConsumersReport::exit_code`].
    pub count: usize,
    /// The consumers whose copy names another version than `profile.version`: the version they run
    /// was overwritten in place.
    #[serde(default)]
    pub overwritten: usize,
    /// One entry per consumer: the clusters Palette lists, in its order, then those the second
    /// reading found, then the edge hosts, then the cluster templates.
    pub consumers: Vec<Consumer>,
    /// The second reading: the clusters read for the profiles they apply.
    #[serde(default)]
    pub clusters_read: ClustersRead,
    /// The edge hosts read for the profiles bound to them.
    #[serde(default)]
    pub edge_hosts_read: EdgeHostsRead,
    /// The projects that strata searched one by one, and those that it could not. Strata searches
    /// them for a profile of the tenant or of the system, which each project can use. Absent for
    /// a profile of a project, for an app profile, and with `--list-only`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects_read: Option<ProjectsRead>,
}

impl ConsumersReport {
    /// True when a reading failed, so a consumer can be missing from the answer: see
    /// [`ClustersRead::incomplete`], [`EdgeHostsRead::incomplete`], and
    /// [`ProjectsRead::incomplete`].
    pub fn incomplete(&self) -> bool {
        self.clusters_read.incomplete()
            || self.edge_hosts_read.incomplete()
            || self
                .projects_read
                .as_ref()
                .is_some_and(ProjectsRead::incomplete)
    }

    /// The exit status, so that a delete step can branch on it. 1 when something uses the
    /// profile version, whatever else failed. 0 when nothing does and each reading succeeded. 2
    /// when nothing was found and a reading failed: a consumer can be missing from the answer.
    /// Palette gives an empty consumer list on an error too, so a failed reading with no
    /// consumer found is not "nothing uses it".
    pub fn exit_code(&self) -> u8 {
        if self.count > 0 {
            1
        } else if self.incomplete() {
            2
        } else {
            0
        }
    }
}

/// List the clusters that use a profile version, with each cluster's name, project, and state.
///
/// The report also holds the edge hosts bound to the version and the cluster templates that link
/// it. The profile comes from a uid, `<name>@<version>`, or a profile record (strata uses its uid
/// and project, and always fetches the list live). Names and projects come from the profile's
/// `status.inUseClusters`. Strata then reads each cluster in its own project for its state, with
/// one client per project from `connect`. The report lists a cluster that strata cannot read with
/// state `unknown` and its error, and the cluster still counts. Then strata reads each cluster the
/// key can list for the profiles it applies: see [`consumers_with`] to read the list alone.
///
/// `connect` gives a client for a project uid, with `None` for the configured scope.
/// `generated_at` is the time of the report, in RFC 3339.
///
/// # Errors
///
/// The errors of [`consumers_with`].
pub fn consumers<A, F>(
    reference: &Ref,
    connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<ConsumersReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    consumers_with(
        reference,
        &ConsumersOptions::default(),
        connect,
        generated_at,
        progress,
    )
}

/// [`consumers`] with options.
///
/// `list_only` reads the consumer list that Palette computes, and no other cluster. It costs one
/// request per consumer. The second reading costs one request per 50 clusters that the cluster
/// search names, and one per cluster that the list omits. When the search takes no profile, the
/// second reading costs one request per cluster of the fleet. But `list_only` does not find a
/// consumer that the list omits.
///
/// Each project can use a profile of the tenant or of the system. So for such a profile, strata
/// also searches each project of the tenant listing, through `connect` with the uid of the project.
/// [`ConsumersReport::projects_read`] names each project that it could not search.
///
/// An app profile, by ref or by record, gives its app deployments.
///
/// # Errors
///
/// [`ProfileRefRequired`](crate::Error::ProfileRefRequired) for a ref that names no profile, and for
/// a record of a cluster or of a pack. [`NoUid`](crate::Error::NoUid) for a record with no uid.
/// The error of `connect` for the scope of the profile, and the error of the read of the profile:
/// [`ProfileNotFound`](crate::Error::ProfileNotFound) and
/// [`ProfileNameAmbiguous`](crate::Error::ProfileNameAmbiguous) for a name. A cluster that cannot be
/// read stays in the report with its error. A tenant client that `connect` cannot give is no error:
/// the second reading then uses the client of the profile.
pub fn consumers_with<A, F>(
    reference: &Ref,
    options: &ConsumersOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<ConsumersReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let (profile_scope, uid_hint) = match target_of(reference)? {
        Target::App { scope, id, version } => {
            return app_consumers(
                scope.as_deref(),
                &id,
                version.as_deref(),
                connect,
                generated_at,
                progress,
            );
        }
        Target::Profile { scope, uid } => (scope, uid),
    };

    let mut clients: HashMap<Option<String>, Palette<A>> = HashMap::new();
    let home = connect(profile_scope.as_deref())?;
    let uid = uid_of_ref(reference, uid_hint, &home, progress)?;
    progress.note(&format!("listing consumers of profile {uid}"));
    let doc = home.profile_document(&uid)?;

    let mut list = listed_consumers(&doc, &uid, &home, &mut clients, &mut connect);
    // The list above can omit a cluster: see `second_reading`, which adds it.
    // The second reading and the edge hosts read at tenant scope. So they find a cluster or a
    // host in each project that the key can read. A key that cannot read the tenant reads
    // `home`, the scope of the profile, and not the configured scope as `tenant_of` does. A
    // cluster that runs a profile of a project is in that project
    let tenant = connect(Some(TENANT_SCOPE)).ok();
    let wide = tenant.as_ref().unwrap_or(&home);
    let mut listings = ProjectListings::default();
    let readings = readings(
        &mut list,
        &doc,
        &uid,
        options,
        Scopes { wide, home: &home },
        &mut clients,
        &mut connect,
        &mut listings,
        progress,
    );
    link_and_name(&mut list, &doc, wide, &home, &mut listings);
    let mut r = report(&doc, None, list, readings, generated_at);
    name_profile_project(&mut r.profile, wide, &home, &mut listings);
    Ok(r)
}

/// The client of the tenant, `wide`, and the client of the scope of the profile, `home`. `wide`
/// is `home` when strata cannot connect to the tenant.
struct Scopes<'a, A: PaletteApi> {
    wide: &'a Palette<A>,
    home: &'a Palette<A>,
}

/// What strata reads beside the consumer list: the second reading, the edge hosts, and the
/// projects that it searches one by one.
///
/// Each project can use a version of the tenant or of the system. So strata searches each project
/// of the tenant listing for it: see [`search_each_project`]. `--list-only` asks for none of it
/// but the edge hosts.
#[allow(clippy::too_many_arguments)]
fn readings<A, F>(
    list: &mut Vec<Consumer>,
    doc: &Value,
    uid: &str,
    options: &ConsumersOptions,
    scopes: Scopes<'_, A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
    listings: &mut ProjectListings,
    progress: &Progress,
) -> Readings
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let Scopes { wide, home } = scopes;
    let (name, project) = search_scope(doc);
    let shared = ProfileScope::of_document(doc).is_some_and(ProfileScope::is_shared);
    let (clusters, more_hosts, projects) = if options.list_only {
        let skipped = ClustersRead {
            skipped: true,
            ..ClustersRead::default()
        };
        (skipped, Vec::new(), None)
    } else if shared {
        let each = search_each_project(&name, wide, home, clients, connect, listings, progress);
        let read = second_reading(
            list,
            uid,
            each.candidates,
            wide,
            home,
            clients,
            connect,
            progress,
        );
        (read, each.hosts, Some(each.projects))
    } else {
        let candidates =
            candidate_clusters(&name, &project, wide, home, clients, connect, progress);
        let read = second_reading(
            list, uid, candidates, wide, home, clients, connect, progress,
        );
        (read, Vec::new(), None)
    };
    // the consumer list names no edge host, and the search of the edge hosts is one request a
    // page, so it is done also with --list-only
    let hosts = joined_hosts(edge_host_search(wide, home, progress), more_hosts);
    Readings {
        clusters,
        edge_hosts: edge_hosts_bound(list, uid, hosts),
        projects,
    }
}

/// Mark the consumers that link a cluster template, and give each one the name of its project.
///
/// `wide` and `home` give the project listings: see [`name_consumer_projects`].
fn link_and_name<A: PaletteApi>(
    list: &mut Vec<Consumer>,
    doc: &Value,
    wide: &Palette<A>,
    home: &Palette<A>,
    listings: &mut ProjectListings,
) {
    // Palette refuses the delete of a version that a cluster template links, in any project. The
    // profile document names each such template, so they cost no request (#147)
    linked_templates(list, doc);
    // a uid names no project to a reader, so give each row the project's name as well
    name_consumer_projects(list, wide, home, listings);
}

/// Give each consumer the name of its project: see [`name_wide_then_home`].
pub(super) fn name_consumer_projects<A: PaletteApi>(
    list: &mut [Consumer],
    wide: &Palette<A>,
    home: &Palette<A>,
    listings: &mut ProjectListings,
) {
    name_wide_then_home(
        list,
        |c| &c.project_uid,
        |c| &mut c.project_name,
        [wide, home],
        listings,
    );
}

/// Give each row the name of its project, from the project listing of each scope of `scopes`.
///
/// The scopes are the tenant, and then the scope of the profile. See [`name_projects_of`].
///
/// In a project, Palette lists that project alone, so a row in another project gets its name only
/// from the listing of the tenant. A row that still has no name takes it from the next listing.
/// Strata reads no more listings when each row that names a project has a name.
fn name_wide_then_home<T, A: PaletteApi>(
    rows: &mut [T],
    uid_of: impl Fn(&T) -> &String,
    name_of: impl Fn(&mut T) -> &mut String,
    scopes: [&Palette<A>; 2],
    listings: &mut ProjectListings,
) {
    for palette in scopes {
        if rows
            .iter_mut()
            .all(|r| uid_of(r).is_empty() || !name_of(r).is_empty())
        {
            return;
        }
        name_projects_of(rows, &uid_of, &name_of, palette, listings);
    }
}

/// The clusters that the profile document `doc` names as users of the profile `uid`. Strata reads
/// each one in its own project for its state and the version that it runs.
fn listed_consumers<A, F>(
    doc: &Value,
    uid: &str,
    home: &Palette<A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
) -> Vec<Consumer>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let named = arr(doc, "/status/inUseClusters");
    let uids = consumer_uids(doc);
    let mut list = Vec::with_capacity(uids.len());
    for cluster in uids {
        let known = named.iter().find(|n| text(n, "/uid") == cluster);
        // a uid that `inUseClusters` does not name has no name and no project here: the read of
        // its document gives them
        let mut c = Consumer::unread(
            ConsumerKind::Cluster,
            cluster,
            known.map(|n| text(n, "/name")).unwrap_or_default(),
            known.map(|n| text(n, "/projectUid")).unwrap_or_default(),
        );
        c.read(
            clients,
            home,
            connect,
            |p, cluster| p.subject_document(SubjectKind::Cluster, cluster),
            // a cluster that does not run the profile now has no version of it
            |d| {
                AppliedProfile::applied(d, uid)
                    .map(|p| p.version)
                    .unwrap_or_default()
            },
        );
        list.push(c);
    }
    list
}

/// The name of the profile in `doc`, and the project to search for the clusters that run it: the
/// scope of the profile (#181). The project is empty for a profile of the tenant.
fn search_scope(doc: &Value) -> (String, String) {
    let project = if ProfileScope::of_document(doc) == Some(ProfileScope::Tenant) {
        String::new()
    } else {
        text(doc, "/metadata/annotations/projectUid")
    };
    (text(doc, "/metadata/name"), project)
}

/// What strata read beside the consumer list, for [`report`].
pub(super) struct Readings {
    /// The second reading.
    pub(super) clusters: ClustersRead,
    /// The edge hosts.
    pub(super) edge_hosts: EdgeHostsRead,
    /// The projects that strata searched one by one, for a version that each project can use.
    pub(super) projects: Option<ProjectsRead>,
}

/// The document of `consumers` for the profile `doc`, with the consumers `consumers`.
///
/// `kind` is the type of an app profile. The document holds the count, how many copies name another
/// version than the profile, and the readings `readings`. A cluster profile has the scope that
/// `doc` names, and the project of a profile of a project, with no name yet.
fn report(
    doc: &Value,
    kind: Option<ProfileType>,
    consumers: Vec<Consumer>,
    readings: Readings,
    generated_at: &str,
) -> ConsumersReport {
    let version = text(doc, "/spec/version");
    let overwritten = consumers
        .iter()
        .filter(|c| !c.version.is_empty() && c.version != version)
        .count();
    // strata reads the scope of a cluster profile only: no fact of Palette gives the scope of an
    // app profile
    let scope = kind
        .is_none()
        .then(|| ProfileScope::of_document(doc))
        .flatten();
    let project_uid = if scope == Some(ProfileScope::Project) {
        text(doc, "/metadata/annotations/projectUid")
    } else {
        String::new()
    };
    ConsumersReport {
        schema: CONSUMERS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        profile: ConsumedProfile {
            uid: text(doc, "/metadata/uid"),
            name: text(doc, "/metadata/name"),
            version,
            kind,
            scope,
            project_uid,
            project_name: String::new(),
        },
        count: consumers.len(),
        overwritten,
        consumers,
        clusters_read: readings.clusters,
        edge_hosts_read: readings.edge_hosts,
        projects_read: readings.projects,
    }
}

/// Give the profile of a project the name of its project, as [`name_consumer_projects`] names
/// the project of a consumer.
pub(super) fn name_profile_project<A: PaletteApi>(
    profile: &mut ConsumedProfile,
    wide: &Palette<A>,
    home: &Palette<A>,
    listings: &mut ProjectListings,
) {
    name_wide_then_home(
        std::slice::from_mut(profile),
        |p| &p.project_uid,
        |p| &mut p.project_name,
        [wide, home],
        listings,
    );
}

/// Add each cluster template that `status.inUseClusterTemplates` of the profile document names. A
/// template keeps no copy of the version and has no state, so both stay empty.
fn linked_templates(list: &mut Vec<Consumer>, doc: &Value) {
    for t in ClusterTemplateRef::linked(doc) {
        list.push(Consumer {
            state: String::new(),
            ..Consumer::unread(ConsumerKind::Template, t.uid, t.name, t.project_uid)
        });
    }
}

#[cfg(test)]
mod tests;
