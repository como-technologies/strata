//! `consumers`: the clusters that run a profile version.

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

/// One cluster that uses the profile version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Consumer {
    /// Cluster uid.
    pub uid: String,
    /// Cluster name; empty when neither the profile nor the cluster could name it.
    pub name: String,
    /// The project the cluster is in; empty for tenant scope or when unknown.
    pub project_uid: String,
    /// That project's name. A name is a convenience, so it is empty when the project listing
    /// could not be read, and the reports fall back to the uid.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project_name: String,
    /// The cluster's `status.state`, for example `Running`, or `unknown` when it could not be read.
    pub state: String,
    /// The version the cluster's own copy of the profile names, from the cluster document; empty
    /// when the cluster could not be read. It differs from `profile.version` when the version the
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
    /// The number of clusters that use it. Exit status 0 when zero, 1 otherwise.
    pub count: usize,
    /// The consumers whose copy names another version than `profile.version`: the version they run
    /// was overwritten in place.
    #[serde(default)]
    pub overwritten: usize,
    /// One entry per cluster: those Palette lists, in its order, then those the second reading
    /// found.
    pub consumers: Vec<Consumer>,
    /// The second reading: the clusters read for the profiles they apply.
    #[serde(default)]
    pub clusters_read: ClustersRead,
}

impl ConsumersReport {
    /// The exit status: 0 when nothing uses the profile version, 1 when something does, so a
    /// delete step can branch on it.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.count > 0)
    }
}

/// List the clusters that use a profile version, with each cluster's name, project, and state.
/// The profile comes from a uid, `<name>@<version>`, or a profile record (whose uid and project
/// are used; the list is always fetched live). Names and projects come from the profile's
/// `status.inUseClusters`; each cluster is then read in its own project for its state, with one
/// client per project from `connect`. A cluster that cannot be read is listed with state
/// `unknown` and its error; it still counts.
pub fn consumers<A, F>(
    reference: &Ref,
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
        Ref::Cluster { .. } | Ref::Pack { .. } => return Err(refused()),
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            // a pack record is not a profile version either, and NoUid below would name `verify`,
            // a command that is not running
            if recorded.source.cluster.is_some() || recorded.source.pack.is_some() {
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
    let clusters_read =
        second_reading(&mut list, &uid, &home, &mut clients, &mut connect, progress);
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
    })
}

/// The second reading of `consumers`. The listing is read at tenant scope, so a cluster in any
/// project the key can read is in it; a key that cannot list at tenant scope lists its own scope.
/// A cluster the consumer list named is not read again. A cluster that applies `uid` joins
/// `list`, marked `unlisted`, with what its document and the listing say of it.
fn second_reading<A, F>(
    list: &mut Vec<Consumer>,
    uid: &str,
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
    let listing = connect(Some(TENANT_SCOPE))
        .and_then(|tenant| tenant.cluster_listing())
        .or_else(|_| home.cluster_listing());
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
