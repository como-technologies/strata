//! `consumers`: the clusters, edge hosts, and cluster templates that use a profile version, and
//! the app deployments of an app profile.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::consumer_list::consumer_uids;
use super::fleet::Resolved;
use super::list::{ProjectListings, TENANT_SCOPE, name_projects_of};
use super::{client_for, scope_of};
use crate::api::{Palette, PaletteApi};
use crate::error::{Error, ErrorDocument, Result};
use crate::json::{arr, first_nonempty, is_zero, str_at, text};
use crate::model::cluster_template::ClusterTemplateRef;
use crate::model::profile_type::ProfileType;
use crate::model::scope::ProfileScope;
use crate::reference::Ref;
use crate::snapshot::{AppliedProfile, Snapshot, SubjectKind, Tool};
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;
use reads::{edge_hosts_bound, second_reading};

mod app;
mod reads;

use app::app_consumers;

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
/// in the projects the key can read, and gives an empty list on an error too. The second reading
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
}

impl ConsumersReport {
    /// True when a reading failed, so a consumer can be missing from the answer: see
    /// [`ClustersRead::incomplete`] and [`EdgeHostsRead::incomplete`].
    pub fn incomplete(&self) -> bool {
        self.clusters_read.incomplete() || self.edge_hosts_read.incomplete()
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
/// it. The profile comes from a uid, `<name>@<version>`, or a profile record (whose uid and
/// project are used; the list is always fetched live). Names and projects come from the profile's
/// `status.inUseClusters`; each cluster is then read in its own project for its state, with one
/// client per project from `connect`. A cluster that cannot be read is listed with state
/// `unknown` and its error; it still counts. Then each cluster the key can list is read for the
/// profiles it applies: see [`consumers_with`] to read the list alone.
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
/// request per consumer, where the second reading costs one per 50 clusters that the cluster
/// search names and one per cluster that the list omits (one per cluster of the fleet when the
/// search takes no profile), but it does not find a consumer that the list omits.
///
/// An app profile, by ref or by record, gives its app deployments.
///
/// # Errors
///
/// [`Error::ProfileRefRequired`] for a ref that names no profile, and for a record of a cluster
/// or of a pack. [`Error::NoUid`] for a record with no uid. The error of `connect` for the scope
/// of the profile, and the error of the read of the profile: [`Error::ProfileNotFound`] and
/// [`Error::ProfileNameAmbiguous`] for a name. A cluster that cannot be read stays in the report
/// with its error. A tenant client that `connect` cannot give is no error: the second reading then
/// uses the client of the profile.
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
    let uid = uid_of_ref(reference, uid_hint, &home)?;
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
    let clusters_read = if options.list_only {
        ClustersRead {
            skipped: true,
            ..ClustersRead::default()
        }
    } else {
        let (name, project) = search_scope(&doc);
        second_reading(
            &mut list,
            &uid,
            &name,
            &project,
            wide,
            &home,
            &mut clients,
            &mut connect,
            progress,
        )
    };
    // the consumer list names no edge host, and the search of the edge hosts is one request a
    // page, so it is done also with --list-only
    let edge_hosts_read = edge_hosts_bound(&mut list, &uid, wide, &home, progress);
    link_and_name(&mut list, &doc, &home);

    Ok(report(
        &doc,
        None,
        list,
        clusters_read,
        edge_hosts_read,
        generated_at,
    ))
}

/// The uid of the profile that `reference` names. `hint` is the uid that the ref holds, when it
/// holds one. The other refs name the profile by name and version, which `home` resolves.
fn uid_of_ref<A: PaletteApi>(
    reference: &Ref,
    hint: Option<String>,
    home: &Palette<A>,
) -> Result<String> {
    match (hint, reference) {
        (Some(uid), _) => Ok(uid),
        (None, Ref::NameVersion { name, version }) => home.resolve(name, version),
        (None, Ref::Draft { id, version }) => home.profile_uid(id, version.as_deref()),
        (None, _) => unreachable!("every other ref carries a uid"),
    }
}

/// Mark the consumers that link a cluster template, and give each one the name of its project.
fn link_and_name<A: PaletteApi>(list: &mut Vec<Consumer>, doc: &Value, home: &Palette<A>) {
    // Palette refuses the delete of a version that a cluster template links, in any project, and
    // the profile document names each such template, so they cost no request (#147)
    linked_templates(list, doc);
    // a uid names no project to a reader, so give each row the project's name as well
    name_consumer_projects(list, home);
}

/// Give each consumer the name of its project, from the project listing of `home`: see
/// [`name_projects_of`].
pub(super) fn name_consumer_projects<A: PaletteApi>(list: &mut [Consumer], home: &Palette<A>) {
    name_projects_of(
        list,
        |c| &c.project_uid,
        |c| &mut c.project_name,
        home,
        &mut ProjectListings::default(),
    );
}

/// What a ref names for `consumers`.
enum Target {
    /// A profile version. `scope` is the scope of its record, when the ref is a record. `uid` is
    /// its uid, when the ref holds one.
    Profile {
        scope: Option<String>,
        uid: Option<String>,
    },
    /// An app profile, in the scope of its record when the ref is a record.
    App {
        scope: Option<String>,
        id: String,
        version: Option<String>,
    },
}

/// What `reference` names, and the scope to read it in.
///
/// # Errors
///
/// [`Error::ProfileRefRequired`] for a ref that names no profile, and for a record of a cluster
/// or of a pack. [`Error::NoUid`] for a record with no uid. Each error of the read of a record.
fn target_of(reference: &Ref) -> Result<Target> {
    let refused = || Error::ProfileRefRequired {
        reference: reference.to_string(),
    };
    match reference {
        Ref::Cluster { .. }
        | Ref::EdgeHost { .. }
        | Ref::ClusterGroup { .. }
        | Ref::ClusterSet { .. }
        | Ref::Pack { .. } => Err(refused()),
        Ref::AppProfile { id, version } => Ok(Target::App {
            scope: None,
            id: id.clone(),
            version: version.clone(),
        }),
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            // a record of a cluster or of a pack has no consumers. Refuse it here: the NoUid below
            // names `verify`, a command that does not run. An app profile has its app deployments
            if !(recorded.source.is_profile_version() || recorded.source.is_app()) {
                return Err(refused());
            }
            if recorded.source.uid.is_empty() {
                return Err(Error::NoUid);
            }
            let scope = scope_of(&recorded.source.project_uid).map(str::to_string);
            if recorded.source.is_app() {
                return Ok(Target::App {
                    scope,
                    id: recorded.source.uid,
                    version: None,
                });
            }
            Ok(Target::Profile {
                scope,
                uid: Some(recorded.source.uid),
            })
        }
        Ref::Uid(uid) => Ok(Target::Profile {
            scope: None,
            uid: Some(uid.clone()),
        }),
        // a draft has the consumers of its version: a cluster runs a published template
        Ref::NameVersion { .. } | Ref::Draft { .. } => Ok(Target::Profile {
            scope: None,
            uid: None,
        }),
    }
}

/// The clusters that the profile document `doc` names as users of the profile `uid`, each one
/// read in its own project for its state and the version that it runs.
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

/// The document of `consumers` for the profile `doc`, with the consumers `consumers`.
///
/// `kind` is the type of an app profile. The document holds the count, how many copies name another
/// version than the profile, and the two readings.
fn report(
    doc: &Value,
    kind: Option<ProfileType>,
    consumers: Vec<Consumer>,
    clusters_read: ClustersRead,
    edge_hosts_read: EdgeHostsRead,
    generated_at: &str,
) -> ConsumersReport {
    let version = text(doc, "/spec/version");
    let overwritten = consumers
        .iter()
        .filter(|c| !c.version.is_empty() && c.version != version)
        .count();
    ConsumersReport {
        schema: CONSUMERS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        profile: ConsumedProfile {
            uid: text(doc, "/metadata/uid"),
            name: text(doc, "/metadata/name"),
            version,
            kind,
        },
        count: consumers.len(),
        overwritten,
        consumers,
        clusters_read,
        edge_hosts_read,
    }
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
mod tests {
    use super::*;

    #[test]
    fn a_consumer_kind_is_named_in_the_documents_and_in_the_reports() {
        for (kind, name, noun) in [
            (ConsumerKind::Cluster, "cluster", "cluster"),
            (ConsumerKind::EdgeHost, "edgehost", "edge host"),
            (ConsumerKind::Template, "template", "cluster template"),
            (ConsumerKind::App, "app", "app deployment"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), Value::from(name));
            assert_eq!(kind.noun(), noun);
        }
        // a consumer with no kind is a cluster, as each older document names one
        let c: Consumer = serde_json::from_value(serde_json::json!({
            "uid": "c1", "name": "lab", "projectUid": "p1", "state": "Running"
        }))
        .unwrap();
        assert_eq!(c.kind, ConsumerKind::Cluster);
    }

    /// Read the consumer `c` in the home scope. `read` gives its document.
    fn read_with(c: &mut Consumer, read: Result<Value>) {
        let home = Palette::new(crate::api::MemoryApi::new());
        let mut clients = HashMap::new();
        let mut connect = |_: Option<&str>| Ok(Palette::new(crate::api::MemoryApi::new()));
        c.read(
            &mut clients,
            &home,
            &mut connect,
            |_, _| read,
            |d| text(d, "/spec/profile/metadata/version"),
        );
    }

    #[test]
    fn a_consumer_that_is_read_takes_the_fields_of_its_document_and_keeps_those_it_lacks() {
        let mut c = Consumer::unread(ConsumerKind::App, "a1".into(), "shop".into(), "p1".into());
        read_with(
            &mut c,
            Ok(serde_json::json!({
                "status": {"state": "Running"},
                "spec": {"profile": {"metadata": {"version": "1.2.0"}}},
            })),
        );
        assert_eq!(
            (c.name.as_str(), c.project_uid.as_str(), c.state.as_str()),
            ("shop", "p1", "Running"),
            "a field that the document does not give keeps the value of the list"
        );
        assert_eq!(c.version, "1.2.0");
        assert!(c.error.is_none());
    }

    #[test]
    fn a_consumer_whose_document_cannot_be_read_keeps_the_list_and_records_the_error() {
        let mut c = Consumer::unread(ConsumerKind::Cluster, "c1".into(), "lab".into(), "".into());
        read_with(
            &mut c,
            Err(Error::Usage {
                message: "no such cluster".to_string(),
            }),
        );
        assert_eq!(c.state, UNKNOWN_STATE);
        assert_eq!(c.version, "");
        assert_eq!(c.name, "lab");
        let message = c.error.as_ref().and_then(|e| e.get("message")?.as_str());
        assert_eq!(message, Some("no such cluster"));
    }
}
