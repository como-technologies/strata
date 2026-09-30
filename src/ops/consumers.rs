//! `consumers`: the clusters, edge hosts, and cluster templates that use a profile version.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{Snapshot, Tool};

use super::list::{TENANT_SCOPE, list_clusters, name_projects_of};
use super::{client_for, consumer_uids, text};

/// Schema id of the document `consumers` writes.
pub const CONSUMERS_SCHEMA: &str = "strata-consumers/1";

/// The state a consumer shows when its cluster could not be read.
pub const UNKNOWN_STATE: &str = "unknown";

/// The kind of a consumer that is a cluster.
pub const CLUSTER_KIND: &str = "cluster";

/// The kind of a consumer that is an edge host.
pub const EDGE_HOST_KIND: &str = "edgehost";

/// The kind of a consumer that is a cluster template.
pub const TEMPLATE_KIND: &str = "template";

fn cluster_kind() -> String {
    CLUSTER_KIND.to_string()
}

/// The profile version whose consumers are listed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ConsumedProfile {
    /// Profile uid.
    pub uid: String,
    /// Profile name.
    pub name: String,
    /// Profile version.
    pub version: String,
}

/// One cluster, edge host, or cluster template that uses the profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Consumer {
    /// `cluster`, `edgehost` for an edge host that the profile is bound to, or `template` for a
    /// cluster template that links the version.
    #[serde(default = "cluster_kind")]
    pub kind: String,
    /// Cluster uid, edge host uid, or cluster template uid.
    pub uid: String,
    /// Cluster name; empty when neither the profile nor the cluster could name it.
    pub name: String,
    /// The project the consumer is in; empty for tenant scope or when unknown.
    pub project_uid: String,
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The cluster's `status.state`, for example `Running`, or `unknown` when it could not be read.
    /// Empty for a cluster template: a template has no state.
    pub state: String,
    /// The version the cluster's own copy of the profile names, from the cluster document; empty
    /// when the cluster could not be read, and for a cluster template, which keeps no copy. It differs from `profile.version` when the version the
    /// cluster runs was overwritten in place, by a legacy version bump or a rename.
    #[serde(default)]
    pub version: String,
    /// Why the cluster could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// True when the consumer list of the profile did not name this cluster, and the cluster
    /// document did: the second reading found it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unlisted: bool,
}

/// The second reading of `consumers`: the clusters the key can list, each read for the profiles
/// it applies. Palette computes the consumer list of a profile when it is asked, in the projects
/// the key can read, and gives an empty list on an error too. The second reading finds a consumer
/// that the list omits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClustersRead {
    /// Cluster documents read: each cluster the listing names and the consumer list did not.
    pub read: usize,
    /// Clusters the listing names that could not be read. Each one can be a consumer.
    pub unread: usize,
    /// Why the cluster listing could not be read, when it could not. Then `read` is 0, and the
    /// consumer list of the profile stands alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    /// True when the second reading was not done, as `--list-only` asks. Then the consumer list
    /// of the profile stands alone, and an empty list can be an error of Palette.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skipped: bool,
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
    pub error: Option<Value>,
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
    pub generated_at: String,
    /// The profile version.
    pub profile: ConsumedProfile,
    /// The number of clusters, edge hosts, and cluster templates that use it. Exit status 0 when
    /// zero, 1 otherwise.
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
    /// The exit status: 0 when nothing uses the profile version, 1 when something does, so a
    /// delete step can branch on it.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.count > 0)
    }
}

/// List the clusters that use a profile version, with each cluster's name, project, and state,
/// then the edge hosts bound to it and the cluster templates that link it.
/// The profile comes from a uid, `<name>@<version>`, or a profile record (whose uid and project
/// are used; the list is always fetched live). Names and projects come from the profile's
/// `status.inUseClusters`; each cluster is then read in its own project for its state, with one
/// client per project from `connect`. A cluster that cannot be read is listed with state
/// `unknown` and its error; it still counts. Then each cluster the key can list is read for the
/// profiles it applies: see [`consumers_with`] to read the list alone.
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

/// [`consumers`] with options: `list_only` reads the consumer list that Palette computes, and
/// no other cluster. It costs one request per consumer, where the second reading costs one per
/// cluster of the fleet, but it does not find a consumer that the list omits.
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
    let refused = || Error::ProfileRefRequired {
        reference: reference.to_string(),
    };
    let (profile_scope, uid_hint) = match reference {
        Ref::Cluster { .. }
        | Ref::EdgeHost { .. }
        | Ref::ClusterGroup { .. }
        | Ref::AppProfile { .. }
        | Ref::ClusterSet { .. }
        | Ref::Pack { .. } => {
            return Err(refused());
        }
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            // a pack record is not a profile version either, and NoUid below would name `verify`,
            // a command that is not running
            if recorded.source.cluster.is_some()
                || recorded.source.pack.is_some()
                || recorded.source.kind == crate::snapshot::APP_KIND
            {
                return Err(refused());
            }
            if recorded.source.uid.is_empty() {
                return Err(Error::NoUid);
            }
            let scope = Some(recorded.source.project_uid.clone()).filter(|p| !p.is_empty());
            (scope, Some(recorded.source.uid))
        }
        Ref::Uid(uid) => (None, Some(uid.clone())),
        Ref::NameVersion { .. } => (None, None),
    };

    let mut clients: HashMap<String, Palette<A>> = HashMap::new();
    let home = connect(profile_scope.as_deref())?;
    let uid = match (uid_hint, reference) {
        (Some(uid), _) => uid,
        (None, Ref::NameVersion { name, version }) => home.resolve(name, version)?,
        (None, _) => unreachable!("every other ref carries a uid"),
    };
    progress.note(&format!("listing consumers of profile {uid}"));
    let doc = home.profile_document(&uid)?;

    let named: Vec<Value> = doc
        .pointer("/status/inUseClusters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let uids = consumer_uids(&doc);

    let mut list = Vec::with_capacity(uids.len());
    for cluster in uids {
        let known = named.iter().find(|n| text(n, "/uid") == cluster);
        let mut c = Consumer {
            kind: cluster_kind(),
            name: known.map(|n| text(n, "/name")).unwrap_or_default(),
            project_uid: known.map(|n| text(n, "/projectUid")).unwrap_or_default(),
            project_name: String::new(),
            state: UNKNOWN_STATE.to_string(),
            version: String::new(),
            error: None,
            unlisted: false,
            uid: cluster,
        };
        let doc = client_for(&mut clients, &home, &c.project_uid, &mut connect)
            .and_then(|p| p.cluster_document(&c.uid));
        match doc {
            Ok(d) => {
                let or_keep = |current: String, found: String| {
                    if found.is_empty() { current } else { found }
                };
                c.name = or_keep(c.name, text(&d, "/metadata/name"));
                c.project_uid =
                    or_keep(c.project_uid, text(&d, "/metadata/annotations/projectUid"));
                c.state = or_keep(c.state, text(&d, "/status/state"));
                c.version = d
                    .pointer("/spec/clusterProfileTemplates")
                    .and_then(Value::as_array)
                    .and_then(|ts| ts.iter().find(|t| text(t, "/uid") == uid))
                    .map(|t| text(t, "/profileVersion"))
                    .unwrap_or_default();
            }
            Err(e) => c.error = Some(e.to_json()),
        }
        list.push(c);
    }
    // Palette computes the list above when it is asked, in the projects the key can read, and
    // gives an empty list on an error too. So each cluster the key can list is read for the
    // profiles it applies, and a cluster that applies the version joins the list (#66).
    // the second reading and the edge hosts are read at tenant scope, so a cluster or a host in
    // any project the key can read is in them; a key that cannot read the tenant reads its own
    // scope
    let tenant = connect(Some(TENANT_SCOPE)).ok();
    let wide = tenant.as_ref().unwrap_or(&home);
    let clusters_read = if options.list_only {
        ClustersRead {
            skipped: true,
            ..ClustersRead::default()
        }
    } else {
        second_reading(
            &mut list,
            &uid,
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
    // Palette refuses the delete of a version that a cluster template links, in any project, and
    // the profile document names each such template, so they cost no request (#147)
    linked_templates(&mut list, &doc);
    // a uid names no project to a reader, so give each row the project's name as well
    name_projects_of(
        &mut list,
        |c| &c.project_uid,
        |c| &mut c.project_name,
        &home,
    );

    let version = text(&doc, "/spec/version");
    let overwritten = list
        .iter()
        .filter(|c| !c.version.is_empty() && c.version != version)
        .count();
    Ok(ConsumersReport {
        schema: CONSUMERS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        profile: ConsumedProfile {
            uid: text(&doc, "/metadata/uid"),
            name: text(&doc, "/metadata/name"),
            version,
        },
        count: list.len(),
        overwritten,
        consumers: list,
        clusters_read,
        edge_hosts_read,
    })
}

/// Add each cluster template that `status.inUseClusterTemplates` of the profile document names. A
/// template keeps no copy of the version and has no state, so both stay empty.
fn linked_templates(list: &mut Vec<Consumer>, doc: &Value) {
    let templates = doc
        .pointer("/status/inUseClusterTemplates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for t in templates {
        let uid = text(t, "/uid");
        if uid.is_empty() || list.iter().any(|c| c.kind == TEMPLATE_KIND && c.uid == uid) {
            continue;
        }
        list.push(Consumer {
            kind: TEMPLATE_KIND.to_string(),
            name: text(t, "/name"),
            project_uid: text(t, "/projectUid"),
            project_name: String::new(),
            state: String::new(),
            version: String::new(),
            error: None,
            unlisted: false,
            uid,
        });
    }
}

/// Add each edge host that the profile `uid` is bound to. The search is read through `wide`, and
/// through `home` when `wide` cannot read it. A Palette without the search has no edge hosts.
fn edge_hosts_bound<A: PaletteApi>(
    list: &mut Vec<Consumer>,
    uid: &str,
    wide: &Palette<A>,
    home: &Palette<A>,
    progress: &Progress,
) -> EdgeHostsRead {
    progress.note("reading each edge host for the profiles bound to it");
    let hosts = wide.edge_hosts().or_else(|_| home.edge_hosts());
    let hosts = match hosts {
        Ok(hosts) => hosts,
        Err(Error::Api { status: 404, .. }) => Vec::new(),
        Err(e) => {
            return EdgeHostsRead {
                read: 0,
                error: Some(e.to_json()),
            };
        }
    };
    for host in &hosts {
        let bound = host
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .and_then(|ts| ts.iter().find(|t| text(t, "/uid") == uid));
        if let Some(bound) = bound {
            list.push(Consumer {
                kind: EDGE_HOST_KIND.to_string(),
                uid: text(host, "/metadata/uid"),
                name: text(host, "/metadata/name"),
                project_uid: text(host, "/metadata/annotations/projectUid"),
                project_name: String::new(),
                state: Some(text(host, "/status/state"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| UNKNOWN_STATE.to_string()),
                version: text(bound, "/version"),
                error: None,
                unlisted: false,
            });
        }
    }
    EdgeHostsRead {
        read: hosts.len(),
        error: None,
    }
}

/// The second reading of `consumers`. The listing is read through `wide`, and through `home` when
/// `wide` cannot list. A cluster the consumer list named is not read again. A cluster that
/// applies `uid` joins `list`, marked `unlisted`, with what its document and the listing say of
/// it.
fn second_reading<A, F>(
    list: &mut Vec<Consumer>,
    uid: &str,
    wide: &Palette<A>,
    home: &Palette<A>,
    clients: &mut HashMap<String, Palette<A>>,
    connect: &mut F,
    progress: &Progress,
) -> ClustersRead
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    progress.note("reading each cluster for the profiles it applies");
    let listing = wide.cluster_listing().or_else(|_| home.cluster_listing());
    let listing = match listing {
        Ok(listing) => listing,
        Err(e) => {
            return ClustersRead {
                error: Some(e.to_json()),
                ..ClustersRead::default()
            };
        }
    };
    let mut clusters_read = ClustersRead::default();
    for listed in list_clusters(&listing) {
        if list.iter().any(|c| c.uid == listed.uid) {
            continue;
        }
        let doc = client_for(clients, home, &listed.project_uid, connect)
            .and_then(|p| p.cluster_document(&listed.uid));
        let doc = match doc {
            Ok(doc) => doc,
            Err(_) => {
                clusters_read.unread += 1;
                continue;
            }
        };
        clusters_read.read += 1;
        let copy = doc
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .and_then(|ts| ts.iter().find(|t| text(t, "/uid") == uid));
        if let Some(copy) = copy {
            let or_listed = |found: String, listed: &str| {
                if found.is_empty() {
                    listed.to_string()
                } else {
                    found
                }
            };
            list.push(Consumer {
                kind: cluster_kind(),
                uid: listed.uid.clone(),
                name: or_listed(text(&doc, "/metadata/name"), &listed.name),
                project_uid: or_listed(
                    text(&doc, "/metadata/annotations/projectUid"),
                    &listed.project_uid,
                ),
                project_name: String::new(),
                state: or_listed(text(&doc, "/status/state"), &listed.state),
                version: text(copy, "/profileVersion"),
                error: None,
                unlisted: true,
            });
        }
    }
    clusters_read
}
