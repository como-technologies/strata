//! The fleet: the clusters that a query of labels selects, from a search of Palette or a saved tag
//! filter, and a record of each.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi, SEARCH_TAGS, search_condition, search_filter, search_group};
use crate::closed_set::ClosedSet;
use crate::conjunction::Conjunction;
use crate::error::{Error, Result};
use crate::reference::{Ref, one_named};
use crate::snapshot::{Snapshot, Tool};
use crate::term::progress::Progress;

use super::list::{ListedCluster, ProjectListings, TENANT_SCOPE, list_clusters, name_projects_of};
use crate::json::{arr, first_nonempty, flag, items, str_at, strings, text};

use super::scope_of;

/// Schema id of the document that `snapshot` writes for a set of clusters.
pub const SET_SCHEMA: &str = "strata-set/1";

/// The value that the provider gives the label of a tag with no value, such as `prod`.
pub const TAG_WITHOUT_VALUE: &str = "spectro__tag";

/// One condition on the labels of a cluster, as a saved tag filter holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TagCondition {
    /// The label key.
    pub key: String,
    /// The values the label may have; empty when the key alone must be there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// True when the condition is the opposite: the label is not there, or has none of the
    /// values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub negation: bool,
}

impl TagCondition {
    /// `env=prod` or `env`, as `--tag` and `tag:` take it.
    pub fn parse(text: &str) -> TagCondition {
        match text.split_once('=') {
            Some((key, value)) => TagCondition {
                key: key.to_string(),
                values: vec![value.to_string()],
                negation: false,
            },
            None => TagCondition {
                key: text.to_string(),
                values: Vec::new(),
                negation: false,
            },
        }
    }

    /// True when the labels meet the condition, as Palette evaluates a tag filter.
    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        let hit = match labels.get(&self.key) {
            None => false,
            Some(_) if self.values.is_empty() => true,
            Some(value) => self.values.iter().any(|v| v == value),
        };
        hit != self.negation
    }

    /// The condition of the cluster search: the property `tags`, a string that the search reads
    /// as `key:value`, or as `key` for a label that must be there.
    fn search_item(&self) -> Value {
        let values: Vec<String> = if self.values.is_empty() {
            vec![self.key.clone()]
        } else {
            self.values
                .iter()
                .map(|v| format!("{}:{v}", self.key))
                .collect()
        };
        search_condition(SEARCH_TAGS, &values, self.negation)
    }
}

impl fmt::Display for TagCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let not = if self.negation { "not " } else { "" };
        // a tag with no value is the label with the value the provider gives it: the key alone
        let values: Vec<String> = self
            .values
            .iter()
            .filter(|v| v.as_str() != TAG_WITHOUT_VALUE)
            .cloned()
            .collect();
        match values.as_slice() {
            [] => write!(f, "{not}{}", self.key),
            [one] => write!(f, "{not}{}={one}", self.key),
            many => write!(f, "{not}{} in ({})", self.key, many.join(", ")),
        }
    }
}

/// A saved tag filter of Palette.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilter {
    /// The filter name.
    pub name: String,
    /// Its uid.
    pub uid: String,
    /// How the conditions join: see [`Conjunction`].
    pub conjunction: Conjunction,
    /// Its conditions.
    pub conditions: Vec<TagCondition>,
}

impl SavedFilter {
    /// The `filter:<name>` ref that selects the clusters of the filter.
    pub(crate) fn reference(&self) -> String {
        Ref::ClusterSet {
            tags: Vec::new(),
            filter: Some(self.name.clone()),
        }
        .to_string()
    }

    /// `env=prod and team in (a, b)`.
    pub fn expression(&self) -> String {
        self.conditions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(&format!(" {} ", self.conjunction))
    }
}

/// What selects the clusters: labels, a saved tag filter, or both, which must all hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterQuery {
    /// Each label that must be there: `env=prod`, or `env`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// A saved tag filter, by name or uid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

impl ClusterQuery {
    /// True when the query selects nothing, so no search is due.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.filter.is_none()
    }
}

/// How the clusters of a query were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Resolved {
    /// The cluster search of Palette, at tenant scope.
    Search,
    /// The cluster listing, with the labels matched by strata: Palette has no search, or its
    /// search takes no tags.
    Listing,
}

impl Resolved {
    /// The noun that names how strata found the clusters, in a report.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Search => "search of Palette",
            Self::Listing => "cluster listing",
        }
    }
}

/// The clusters that a query selects, and how strata found them.
#[derive(Debug, Clone)]
pub struct Selection {
    /// The clusters, by name.
    pub clusters: Vec<ListedCluster>,
    /// How strata found them.
    pub resolved: Resolved,
    /// Whether `clusters` holds the virtual clusters that the query selects: the search was
    /// asked for them. False when strata read the listing, or the search takes no
    /// `environment`.
    pub includes_virtual: bool,
    /// The saved filter, when the query names one.
    pub filter: Option<SavedFilter>,
}

/// Every saved tag filter of the tenant, with its conditions: one request for the list, and one
/// for each filter.
pub fn saved_filters<A: PaletteApi>(tenant: &Palette<A>) -> Result<Vec<SavedFilter>> {
    let listing = tenant.tag_filters()?;
    let entries: Vec<(String, String)> = items(&listing)
        .map(|it| {
            (
                first_nonempty(&[str_at(it, "/metadata/name"), str_at(it, "/name")]),
                first_nonempty(&[str_at(it, "/metadata/uid"), str_at(it, "/uid")]),
            )
        })
        .filter(|(_, uid)| !uid.is_empty())
        .collect();
    let mut filters = Vec::with_capacity(entries.len());
    for (name, uid) in entries {
        filters.push(filter_of(&tenant.tag_filter(&uid)?, &name, &uid));
    }
    filters.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(filters)
}

/// A saved tag filter from its document.
fn filter_of(doc: &Value, name: &str, uid: &str) -> SavedFilter {
    let conditions = arr(doc, "/spec/filterGroup/filters")
        .iter()
        .map(|f| TagCondition {
            key: text(f, "/key"),
            values: strings(f, "/values"),
            negation: flag(f, "/negation"),
        })
        .collect();
    SavedFilter {
        name: first_nonempty(&[str_at(doc, "/metadata/name"), name]),
        uid: uid.to_string(),
        // a filter that names no conjunction joins with `and`, as Palette does
        conjunction: Conjunction::parse(str_at(doc, "/spec/filterGroup/conjunction"))
            .unwrap_or_default(),
        conditions,
    }
}

/// The saved filter of that name or uid.
fn find_filter<A: PaletteApi>(tenant: &Palette<A>, wanted: &str) -> Result<SavedFilter> {
    let filters = saved_filters(tenant)?;
    let hits: Vec<&SavedFilter> = filters
        .iter()
        .filter(|f| f.name == wanted || f.uid == wanted)
        .collect();
    one_named(
        hits,
        |f| f.uid.as_str(),
        || Error::FilterNotFound {
            filter: wanted.to_string(),
            available: filters.iter().map(|f| f.name.clone()).collect(),
        },
        |uids| Error::FilterNameAmbiguous {
            name: wanted.to_string(),
            uids,
        },
    )
    .cloned()
}

/// The groups of the query: the labels of `--tag`, which must all hold, and the conditions of
/// the saved filter, joined as the filter joins them.
fn groups(
    query: &ClusterQuery,
    filter: Option<&SavedFilter>,
) -> Vec<(Conjunction, Vec<TagCondition>)> {
    let mut out = Vec::new();
    if !query.tags.is_empty() {
        out.push((
            Conjunction::And,
            query.tags.iter().map(|t| TagCondition::parse(t)).collect(),
        ));
    }
    if let Some(f) = filter {
        out.push((f.conjunction, f.conditions.clone()));
    }
    out
}

/// True when the labels meet each group.
fn meets(groups: &[(Conjunction, Vec<TagCondition>)], labels: &BTreeMap<String, String>) -> bool {
    groups
        .iter()
        .all(|(conjunction, conditions)| match conjunction {
            Conjunction::Or => conditions.iter().any(|c| c.matches(labels)),
            Conjunction::And => conditions.iter().all(|c| c.matches(labels)),
        })
}

/// The clusters that `query` selects. Strata reads the search schema of the tenant: when the
/// search takes the property `tags`, the search of Palette selects the clusters, at tenant scope.
/// The search leaves out each virtual cluster unless a condition asks for them. So strata sends
/// it a second time with that condition, when the search takes `environment`. Else strata reads
/// the cluster listing and matches the labels itself. `tenant` reads at tenant scope: see
/// [`tenant_of`].
pub fn select<A: PaletteApi>(
    query: &ClusterQuery,
    tenant: &Palette<A>,
    progress: &Progress,
) -> Result<Selection> {
    select_in(query, tenant, &mut ProjectListings::default(), progress)
}

/// [`select`] with the project listings of the command, so that a later check of the same command
/// reads them again from memory.
pub(super) fn select_in<A: PaletteApi>(
    query: &ClusterQuery,
    tenant: &Palette<A>,
    listings: &mut ProjectListings,
    progress: &Progress,
) -> Result<Selection> {
    let filter = match &query.filter {
        Some(wanted) => Some(find_filter(tenant, wanted)?),
        None => None,
    };
    let groups = groups(query, filter.as_ref());
    let properties = tenant.cluster_search_properties()?;
    let (mut clusters, resolved, includes_virtual) = if properties.contains(SEARCH_TAGS) {
        progress.note("searching the clusters of the tenant");
        let body = search_filter(
            groups
                .iter()
                .map(|(conjunction, conditions)| {
                    search_group(
                        *conjunction,
                        conditions.iter().map(TagCondition::search_item).collect(),
                    )
                })
                .collect(),
        );
        let found = tenant.cluster_search_with_virtual(&properties, &body, || {
            progress.note("searching the virtual clusters of the tenant")
        })?;
        (
            list_clusters(&found.items),
            Resolved::Search,
            found.includes_virtual,
        )
    } else {
        progress.note("listing the clusters, and matching their labels");
        let listing = tenant.cluster_listing()?;
        let mut all = list_clusters(items(&listing));
        all.retain(|c| meets(&groups, &c.labels));
        (all, Resolved::Listing, false)
    };
    // the search gives each project by name too; the listing does not
    name_projects_of(
        &mut clusters,
        |c| &c.project_uid,
        |c| &mut c.project_name,
        tenant,
        listings,
    );
    Ok(Selection {
        clusters,
        resolved,
        includes_virtual,
        filter,
    })
}

/// A tenant client, and the configured scope when the key cannot read the tenant.
///
/// `consumers` falls back to the scope of its profile, and not to the configured scope. A
/// cluster that runs a profile of a project is in that project. A fleet has no such scope.
pub fn tenant_of<A, F>(connect: &mut F) -> Result<Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    connect(Some(TENANT_SCOPE)).or_else(|_| connect(None))
}

/// One record that `snapshot_set` wrote, or could not write.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetRecord {
    /// The cluster.
    pub cluster: ListedCluster,
    /// The file.
    pub path: PathBuf,
    /// The root of the record, when strata wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// Why strata could not record the cluster, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// The counts of a set: the clusters of the query, the records written, the clusters that strata
/// could not record, and the files removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SetCounts {
    /// The clusters that the query selects.
    pub clusters: usize,
    /// The records that strata wrote.
    pub recorded: usize,
    /// The clusters that strata could not record.
    pub failed: usize,
    /// The records that `prune` removed.
    pub removed: usize,
}

/// The document that `snapshot` writes for a set of clusters: `strata-set/1`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetReport {
    /// The schema id of this document
    #[schemars(extend("const" = SET_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The query of the clusters: the labels, the saved tag filter, or both.
    pub query: ClusterQuery,
    /// How strata found the clusters.
    pub resolved: Resolved,
    /// The counts.
    pub summary: SetCounts,
    /// One entry for each cluster that the query selects.
    pub records: Vec<SetRecord>,
    /// With `prune`: each record of the directory whose cluster the query no longer selects,
    /// removed.
    pub removed: Vec<PathBuf>,
}

impl SetReport {
    /// 2 when a cluster could not be recorded, else 0.
    pub fn exit_code(&self) -> u8 {
        if self.summary.failed > 0 { 2 } else { 0 }
    }
}

/// The file of each cluster: `<name>.json`, or `<name>.<uid>.json` when two clusters of the set
/// have one name.
fn file_names(clusters: &[ListedCluster]) -> Vec<String> {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for c in clusters {
        *count.entry(c.name.as_str()).or_default() += 1;
    }
    clusters
        .iter()
        .map(|c| {
            if count[c.name.as_str()] > 1 || c.name.is_empty() {
                format!("{}.{}.json", c.name, c.uid)
            } else {
                format!("{}.json", c.name)
            }
        })
        .collect()
}

/// Write a record of each cluster that `query` selects into `dir`, each read in its own project.
/// With `prune`, remove each cluster record of `dir` that is not one of them: the set is the
/// query, and a cluster that no longer meets it leaves the directory. A file that is not a
/// cluster record stays.
pub fn snapshot_set<A, F>(
    query: &ClusterQuery,
    dir: &Path,
    prune: bool,
    mut connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<SetReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let tenant = tenant_of(&mut connect)?;
    let selection = select(query, &tenant, progress)?;
    std::fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let names = file_names(&selection.clusters);
    let mut records = Vec::with_capacity(names.len());
    for (cluster, name) in selection.clusters.into_iter().zip(names) {
        let path = dir.join(name);
        let reference = Ref::Cluster {
            uid: cluster.uid.clone(),
            profile: None,
            pending: false,
        };
        let scope = scope_of(&cluster.project_uid);
        let snap = super::snapshot_for(
            &reference,
            &super::SnapshotOptions::default(),
            || connect(scope),
            fetched_at,
            progress,
        )
        .and_then(|snap| {
            crate::file::write_file(&path, &snap.to_json(false)?)?;
            Ok(snap)
        });
        match snap {
            Ok(snap) => records.push(SetRecord {
                cluster,
                path,
                root: Some(snap.root),
                error: None,
            }),
            Err(e) => records.push(SetRecord {
                cluster,
                path,
                root: None,
                error: Some(e.to_json()),
            }),
        }
    }
    let mut removed = Vec::new();
    if prune {
        let written: BTreeSet<&Path> = records.iter().map(|r| r.path.as_path()).collect();
        let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let mut stale: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| x == "json") && !written.contains(p.as_path())
            })
            .filter(|p| Snapshot::load(p).is_ok_and(|s| s.source.cluster.is_some()))
            .collect();
        stale.sort();
        for path in stale {
            std::fs::remove_file(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            removed.push(path);
        }
    }
    let failed = records.iter().filter(|r| r.error.is_some()).count();
    Ok(SetReport {
        schema: SET_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: fetched_at.to_string(),
        query: query.clone(),
        resolved: selection.resolved,
        summary: SetCounts {
            clusters: records.len(),
            recorded: records.len() - failed,
            failed,
            removed: removed.len(),
        },
        records,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn labels() -> BTreeMap<String, String> {
        [("env", "prod"), ("team", "core")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_group_joins_its_conditions_as_its_conjunction_says() {
        let one_holds = vec![
            TagCondition::parse("env=prod"),
            TagCondition::parse("team=edge"),
        ];
        assert!(meets(&[(Conjunction::Or, one_holds.clone())], &labels()));
        assert!(!meets(&[(Conjunction::And, one_holds.clone())], &labels()));
        // each group must hold
        let both_hold = vec![TagCondition::parse("env=prod"), TagCondition::parse("team")];
        assert!(meets(
            &[
                (Conjunction::And, both_hold.clone()),
                (Conjunction::Or, one_holds.clone())
            ],
            &labels()
        ));
        assert!(!meets(
            &[(Conjunction::And, both_hold), (Conjunction::And, one_holds)],
            &labels()
        ));
        assert!(meets(&[], &labels()));
    }

    #[test]
    fn a_saved_filter_reads_its_conjunction_and_joins_with_and_when_it_names_none() {
        let doc = json!({"metadata": {"name": "edge"}, "spec": {"filterGroup": {
            "conjunction": "or", "filters": [{"key": "env", "values": ["prod"]}]}}});
        let f = filter_of(&doc, "edge", "f1");
        assert_eq!(f.conjunction, Conjunction::Or);
        assert_eq!(f.conditions, [TagCondition::parse("env=prod")]);
        let doc = json!({"metadata": {"name": "edge"}, "spec": {"filterGroup": {"filters": []}}});
        assert_eq!(filter_of(&doc, "edge", "f1").conjunction, Conjunction::And);
    }
}
