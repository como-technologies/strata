//! `list`: the profiles, clusters, and projects in a scope.

use std::collections::{BTreeMap, HashMap};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::Result;
use crate::progress::Progress;
use crate::snapshot::{AppliedProfile, Tool};
use crate::version;

use super::{client_for, consumer_uids, items, text};

/// Schema id of the document `list` writes.
pub const LIST_SCHEMA: &str = "strata-list/1";
/// The scope `list` passes to `connect` for tenant scope: no project, whatever the environment
/// sets.
pub const TENANT_SCOPE: &str = "";

/// A kind of thing `list` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// Profile versions, each a `<name>@<version>` ref.
    Profiles,
    /// Clusters, each a `cluster:<uid>` ref.
    Clusters,
    /// Projects, each a uid for `PALETTE_PROJECT_UID` or `--project`.
    Projects,
    /// The saved tag filters of the tenant, each a `filter:<name>` ref.
    Filters,
}

/// What `list` reads, and in which scope.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// The kinds to list. Empty is profiles and clusters, or profiles alone when `pack` is set.
    pub kinds: Vec<ListKind>,
    /// The project to list profiles and clusters in, in place of the configured scope.
    pub project: Option<String>,
    /// Read each profile document for its type, consumer count, and packs: one request per
    /// profile version.
    pub packs: bool,
    /// Keep only the profiles with a pack whose name contains this text, ignoring case. Implies
    /// `packs`.
    pub pack: Option<String>,
    /// Read each cluster document for the profiles the cluster applies: one request per cluster.
    pub applied: bool,
    /// Keep only the clusters whose name or uid contains this text, ignoring case. Implies
    /// `applied`.
    pub cluster: Option<String>,
    /// Keep only the clusters whose labels meet this query: a search of Palette at tenant scope,
    /// in each project the key can read.
    pub query: super::ClusterQuery,
}

impl ListOptions {
    fn wants(&self, kind: ListKind) -> bool {
        if !self.kinds.is_empty() {
            return self.kinds.contains(&kind);
        }
        // a query of labels is a question about clusters, so it lists them alone
        match kind {
            ListKind::Profiles => self.cluster.is_none() && self.query.is_empty(),
            ListKind::Clusters => self.pack.is_none(),
            ListKind::Projects | ListKind::Filters => false,
        }
    }

    /// True when the clusters are read for the profiles they apply: `--applied`, or `--cluster`,
    /// which names one cluster to answer what it runs.
    fn reads_clusters(&self) -> bool {
        self.applied || self.cluster.is_some()
    }
}

/// One pack of a listed profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListedPack {
    /// Palette's layer name, for example `cni`.
    pub layer: String,
    /// Pack name, for example `cni-cilium-oss`.
    pub name: String,
    /// The tag the profile pins.
    pub tag: String,
    /// Pack version.
    pub version: String,
}

/// One profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedProfile {
    /// `<name>@<version>`, for `snapshot`, `compare`, and `consumers`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
    /// Profile uid.
    pub uid: String,
    /// Cloud type, for example `aws`.
    pub cloud_type: String,
    /// With packs: the profile type, for example `cluster` or `add-on`.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub profile_type: Option<String>,
    /// With packs: the number of clusters that use the version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumers: Option<usize>,
    /// With packs: the packs in the order the profile holds them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<Vec<ListedPack>>,
    /// With packs: why the profile document could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// One cluster.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedCluster {
    /// `cluster:<uid>`, for `snapshot` and `compare`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Cluster name.
    pub name: String,
    /// Cluster uid.
    pub uid: String,
    /// The cluster's `status.state`, for example `Running`.
    pub state: String,
    /// Cloud type, for example `aws`.
    pub cloud_type: String,
    /// The project the cluster is in; empty for a tenant cluster. The ref works only in this
    /// project, which a tenant-scope listing can differ from.
    pub project_uid: String,
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The labels of the cluster, as Palette lists them. A tag of the provider is a label:
    /// `env:prod` is `env` with the value `prod`, and `prod` is `prod` with the value
    /// `spectro__tag`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// With applied: the profile versions the cluster applies, in the order the cluster document
    /// lists them. That order is no install order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<AppliedProfile>>,
    /// With applied: why the cluster document could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// With applied: whether Palette says that an update waits for the cluster, as
    /// `source.cluster.updatesPending` of a record. Absent when the notifications were not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates_pending: Option<bool>,
    /// With applied: each change that waits, as a record gives it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_updates: Vec<crate::snapshot::PendingUpdate>,
}

/// One project.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListedProject {
    /// Project name.
    pub name: String,
    /// Project uid.
    pub uid: String,
}

/// The document `list` writes: `strata-list/1`. A kind that was not listed is absent; a kind
/// that was listed and holds nothing is empty.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    /// The schema id of this document
    #[schemars(extend("const" = LIST_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The project the profiles and clusters were listed in; empty for tenant scope.
    pub project_uid: String,
    /// That project's name; empty for tenant scope or when it could not be read.
    pub project_name: String,
    /// With `--pack`: the text the pack names were matched against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    /// With `--cluster`: the text the cluster names and uids were matched against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster: Option<String>,
    /// Profile versions, by name and then natural version order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<Vec<ListedProfile>>,
    /// Clusters, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clusters: Option<Vec<ListedCluster>>,
    /// Every project the key can see, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ListedProject>>,
    /// With `--tag` or `--filter`: the query of the clusters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<super::ClusterQuery>,
    /// With a query: how strata found the clusters, `search` or `listing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<super::Resolved>,
    /// With `--filter`: the saved tag filter, with its conditions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_filter: Option<super::SavedFilter>,
    /// The saved tag filters of the tenant, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<super::SavedFilter>>,
}

/// List what a ref can name: profile versions and clusters in one scope, and the projects the
/// key can see. Profiles and clusters are read through `connect(options.project)`, where `None`
/// is the configured scope. Projects are read through `connect(Some(TENANT_SCOPE))`, because in a
/// project Palette lists only that project. With `packs` or `pack`, each profile document is read
/// as well; a profile that cannot be read stays in the list with its error, and a listing that
/// cannot be read is an error.
pub fn list<A, F>(
    options: &ListOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<Listing>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let home = connect(options.project.as_deref())?;
    // a query of labels searches every project the key can read, so the listing is at tenant
    // scope, and each cluster names its project
    let project_uid = if options.query.is_empty() {
        home.project_uid().unwrap_or("").to_string()
    } else {
        String::new()
    };

    let profiles = if options.wants(ListKind::Profiles) {
        progress.note("listing profiles");
        Some(list_profiles(&home, options, progress)?)
    } else {
        None
    };
    let mut resolved = None;
    let mut query_filter = None;
    let clusters = if options.wants(ListKind::Clusters) {
        progress.note("listing clusters");
        let mut clusters = if options.query.is_empty() {
            list_clusters(&home.cluster_listing()?)
        } else {
            let tenant = super::tenant_of(&mut connect)?;
            let selection = super::select(&options.query, &tenant, progress)?;
            resolved = Some(selection.resolved);
            query_filter = selection.filter;
            selection.clusters
        };
        if let Some(text) = &options.cluster {
            let text = text.to_lowercase();
            clusters.retain(|c| {
                c.name.to_lowercase().contains(&text) || c.uid.to_lowercase().contains(&text)
            });
        }
        if options.reads_clusters() {
            read_applied(&mut clusters, &home, &mut connect, progress);
        }
        // a uid names no project to a reader, so give each line the project's name as well
        name_projects_of(
            &mut clusters,
            |c| &c.project_uid,
            |c| &mut c.project_name,
            &home,
        );
        Some(clusters)
    } else {
        None
    };
    let filters = if options.wants(ListKind::Filters) {
        progress.note("listing saved tag filters");
        Some(super::saved_filters(&super::tenant_of(&mut connect)?)?)
    } else {
        None
    };
    let projects = if options.wants(ListKind::Projects) {
        progress.note("listing projects");
        let tenant = connect(Some(TENANT_SCOPE))?;
        Some(list_projects(&tenant.project_listing()?))
    } else {
        None
    };

    // The scope's name: from the project listing when it was read, else from the scope itself,
    // where Palette lists that one project. A name is a convenience, so a failure leaves it empty.
    let project_name = if project_uid.is_empty() {
        String::new()
    } else {
        let named = match &projects {
            Some(p) => p.clone(),
            None => home
                .project_listing()
                .map(|doc| list_projects(&doc))
                .unwrap_or_default(),
        };
        named
            .into_iter()
            .find(|p| p.uid == project_uid)
            .map(|p| p.name)
            .unwrap_or_default()
    };

    Ok(Listing {
        schema: LIST_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        project_uid,
        project_name,
        pack: options.pack.clone(),
        cluster: options.cluster.clone(),
        profiles,
        clusters,
        projects,
        query: Some(options.query.clone()).filter(|q| !q.is_empty()),
        resolved,
        query_filter,
        filters,
    })
}

fn list_profiles<A: PaletteApi>(
    palette: &Palette<A>,
    options: &ListOptions,
    progress: &Progress,
) -> Result<Vec<ListedProfile>> {
    let mut profiles: Vec<ListedProfile> = items(&palette.profile_listing()?)
        .map(|it| {
            let name = text(it, "/metadata/name");
            let version = text(it, "/spec/version");
            ListedProfile {
                reference: format!("{name}@{version}"),
                uid: text(it, "/metadata/uid"),
                cloud_type: text(it, "/spec/cloudType"),
                name,
                version,
                profile_type: None,
                consumers: None,
                packs: None,
                error: None,
            }
        })
        .collect();
    profiles.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| version::compare(&a.version, &b.version))
            .then_with(|| a.uid.cmp(&b.uid))
    });
    if !options.packs && options.pack.is_none() {
        return Ok(profiles);
    }

    for p in &mut profiles {
        progress.note(&format!("reading profile {}", p.reference));
        let doc = match palette.profile_document(&p.uid) {
            Ok(doc) => doc,
            Err(e) => {
                p.error = Some(e.to_json());
                continue;
            }
        };
        // A profile that was never published has only a draft, as a snapshot reads it.
        let template = ["/spec/published", "/spec/draft"]
            .into_iter()
            .filter_map(|ptr| doc.pointer(ptr))
            .find(|t| {
                t.get("packs")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
            })
            .or_else(|| doc.pointer("/spec/published"));
        let packs = template
            .and_then(|t| t.get("packs"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|pk| ListedPack {
                layer: text(pk, "/layer"),
                name: text(pk, "/name"),
                tag: text(pk, "/tag"),
                version: text(pk, "/version"),
            })
            .collect();
        p.profile_type = template.map(|t| text(t, "/type")).filter(|t| !t.is_empty());
        p.consumers = Some(consumer_uids(&doc).len());
        p.packs = Some(packs);
    }
    if let Some(needle) = &options.pack {
        let needle = needle.to_lowercase();
        // A profile that could not be read cannot be ruled out, so it stays with its error.
        profiles.retain(|p| match &p.packs {
            Some(packs) => packs
                .iter()
                .any(|pk| pk.name.to_lowercase().contains(&needle)),
            None => p.error.is_some(),
        });
    }
    Ok(profiles)
}

/// Fill in each row's project name from the project listing. The name is a convenience, so a
/// listing that cannot be read leaves every name empty and the reports print the uid alone. No
/// request is made when no row names a project.
pub(super) fn name_projects_of<T, A: PaletteApi>(
    rows: &mut [T],
    uid_of: impl Fn(&T) -> &String,
    name_of: impl Fn(&mut T) -> &mut String,
    home: &Palette<A>,
) {
    if rows.iter().all(|r| uid_of(r).is_empty()) {
        return;
    }
    let Ok(doc) = home.project_listing() else {
        return;
    };
    let names = list_projects(&doc);
    for r in rows.iter_mut() {
        if let Some(p) = names.iter().find(|p| p.uid == *uid_of(r)).cloned() {
            *name_of(r) = p.name;
        }
    }
}

pub(super) fn list_clusters(doc: &Value) -> Vec<ListedCluster> {
    let mut clusters: Vec<ListedCluster> = items(doc)
        .map(|it| {
            let uid = text(it, "/metadata/uid");
            ListedCluster {
                reference: format!("{}{uid}", crate::reference::CLUSTER_PREFIX),
                name: text(it, "/metadata/name"),
                state: text(it, "/status/state"),
                // the listing gives the cloud in the spec, and the search in a summary, with the
                // name of the project
                cloud_type: Some(text(it, "/spec/cloudType"))
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| text(it, "/specSummary/cloudConfig/cloudType")),
                project_uid: Some(text(it, "/metadata/annotations/projectUid"))
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| text(it, "/specSummary/projectMeta/uid")),
                project_name: text(it, "/specSummary/projectMeta/name"),
                labels: it
                    .pointer("/metadata/labels")
                    .and_then(Value::as_object)
                    .map(|o| {
                        o.iter()
                            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
                uid,
                profiles: None,
                error: None,
                updates_pending: None,
                pending_updates: Vec::new(),
            }
        })
        .collect();
    clusters.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uid.cmp(&b.uid)));
    clusters
}

/// Read each cluster document for the profiles it applies. A cluster is in its own project, so
/// each project gets one client, as `consumers` reads its clusters. A cluster the request cannot
/// read keeps its row and carries the error.
fn read_applied<A, F>(
    clusters: &mut [ListedCluster],
    home: &Palette<A>,
    connect: &mut F,
    progress: &Progress,
) where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut clients: HashMap<String, Palette<A>> = HashMap::new();
    for c in clusters.iter_mut() {
        progress.note(&format!("reading cluster {}", c.uid));
        let palette = match client_for(&mut clients, home, &c.project_uid, connect) {
            Ok(p) => p,
            Err(e) => {
                c.error = Some(e.to_json());
                continue;
            }
        };
        let profiles = match palette.cluster_document(&c.uid) {
            Ok(d) => AppliedProfile::of_cluster(&d),
            Err(e) => {
                c.error = Some(e.to_json());
                continue;
            }
        };
        // the updates that wait are a second statement; a failed read leaves them unknown
        if let Ok(n) = palette.cluster_notifications(&c.uid) {
            c.pending_updates = crate::snapshot::PendingUpdate::read(&Value::Array(n), &profiles);
            c.updates_pending = Some(!c.pending_updates.is_empty());
        }
        c.profiles = Some(profiles);
    }
}

fn list_projects(doc: &Value) -> Vec<ListedProject> {
    let mut projects: Vec<ListedProject> = items(doc)
        .map(|it| ListedProject {
            name: text(it, "/metadata/name"),
            uid: text(it, "/metadata/uid"),
        })
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uid.cmp(&b.uid)));
    projects
}
