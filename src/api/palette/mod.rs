//! The operations over any [`PaletteApi`]: find a profile or a cluster, and read the documents a
//! snapshot is built from.

use serde_json::Value;

use super::PaletteApi;
use crate::cloud::CLOUD_NESTED;
use crate::conjunction::Conjunction;
use crate::error::{Error, Result, unread_reason};
use crate::json::{arr, items, str_at, take_items, text};
use crate::pack_type::PackType;
use crate::snapshot::{SubjectKind, Template, manifest_of};
use crate::term::progress::Progress;

mod clusters;
mod listings;
mod profiles;

pub use clusters::{cluster_events_path, cluster_summary_path, subject_path};

const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";
/// The profile search. It pages by `offset`.
const PROFILE_SEARCH: &str = "/v1/dashboard/clusterprofiles";
/// The most entries that Palette serves on one page of a search of the dashboard. The searches:
/// the profile search, the cluster search, the edge host search, and the virtual clusters of a
/// cluster group.
const SEARCH_PAGE: usize = 50;
/// The most entries that Palette serves on one page of the pack listing, `/v1/packs`.
pub(crate) const PACK_LISTING_PAGE: usize = 100;
const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";
const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";
/// The edge host search. It pages by `continue`.
const EDGE_HOST_SEARCH: &str = "/v1/dashboard/edgehosts/search";
const CLUSTER_GROUP_LISTING: &str = "/v1/clustergroups/hostCluster/metadata";
const APP_PROFILE_LISTING: &str = "/v1/dashboard/appProfiles/metadata";
/// The cluster search. It pages by `continue`.
const CLUSTER_SEARCH: &str = "/v1/dashboard/spectroclusters/search";
const CLUSTER_SEARCH_SCHEMA: &str = "/v1/dashboard/spectroclusters/search/schema";
/// The property of the cluster search that takes a label, as `key:value` or `key`.
pub(crate) const SEARCH_TAGS: &str = "tags";
/// The property of the cluster search that takes the cloud type of a cluster.
pub(crate) const SEARCH_ENVIRONMENT: &str = "environment";
/// The property of the cluster search that takes the name of a profile that a cluster runs.
pub(crate) const SEARCH_CLUSTER_PROFILES: &str = "clusterProfiles";
/// The type of a property of the cluster search that takes text, in the schema of the search and
/// in a condition.
const SEARCH_STRING: &str = "string";
const TAG_FILTER_LISTING: &str = "/v1/filters/metadata?filterType=tag";
/// The most entries that Palette serves on one page of the audit log. A larger `limit` answers
/// `500` with the code `InvalidLimit`. The API reference gives 50, but Palette serves 100.
const AUDIT_PAGE: u32 = 100;
/// The field of a tier that gives its place in the install of the app profile. Palette installs
/// the tiers in this order, and does not give the field for a tier whose order is 0.
const TIER_INSTALL_ORDER: &str = "installOrder";
/// The field of a container tier that gives the registry of its image.
const TIER_CONTAINER_REGISTRY: &str = "containerRegistryUid";
/// The collection of the profile versions. A uid names one: `/v1/clusterprofiles/{uid}`.
const PROFILES: &str = "/v1/clusterprofiles";
/// The collection of the clusters: `/v1/spectroclusters/{uid}`.
const CLUSTERS: &str = "/v1/spectroclusters";
/// The collection of the cluster groups: `/v1/clustergroups/{uid}`.
const CLUSTER_GROUPS: &str = "/v1/clustergroups";
/// The collection of the edge hosts: `/v1/edgehosts/{uid}`.
const EDGE_HOSTS: &str = "/v1/edgehosts";
/// The collection of the app profiles: `/v1/appProfiles/{uid}`.
const APP_PROFILES: &str = "/v1/appProfiles";
/// The collection of the app deployments: `/v1/appDeployments/{uid}`.
const APP_DEPLOYMENTS: &str = "/v1/appDeployments";
/// The collection of the packs: `/v1/packs/{uid}`, and the listing and the tags under it.
const PACKS: &str = "/v1/packs";
/// The collection of the saved tag filters: `/v1/filters/tag/{uid}`.
const TAG_FILTERS: &str = "/v1/filters/tag";
/// The collection of the cloud configs: `/v1/cloudconfigs/{cloudType}/{uid}`.
const CLOUD_CONFIGS: &str = "/v1/cloudconfigs";
/// The registries: the summary listing of a kind is under it.
const REGISTRIES: &str = "/v1/registries";
/// The registries that the key can see.
const REGISTRY_METADATA: &str = "/v1/registries/metadata";
/// The dashboard summary of a cluster: `/v1/dashboard/spectroclusters/{uid}`.
const CLUSTER_SUMMARIES: &str = "/v1/dashboard/spectroclusters";
/// The events of a cluster: `/v1/events/components/spectrocluster/{uid}`.
const CLUSTER_EVENTS: &str = "/v1/events/components/spectrocluster";
/// The most events that strata asks for in one read of the events of a cluster.
const CLUSTER_EVENTS_LIMIT: usize = 50;
/// The cloud accounts: `/v1/cloudaccounts/{cloudType}/{uid}`.
const CLOUD_ACCOUNTS: &str = "/v1/cloudaccounts";
/// The dashboard of a cluster group: its virtual clusters are under it.
const CLUSTER_GROUP_DASHBOARD: &str = "/v1/dashboard/clustergroups";
/// The notifications of a cluster: `/v1/notifications/spectrocluster/{uid}`.
const CLUSTER_NOTIFICATIONS: &str = "/v1/notifications/spectrocluster";
/// The audit log.
const AUDITS: &str = "/v1/audits";

/// A route of Palette: `prefix`, then each segment of `segments`, each escaped by [`query_value`].
///
/// So a `/` or a `?` in a uid or a name stays in its segment. A fixed word of a route, such as
/// `packs`, is a segment too, and the escape leaves it as it is.
fn route(prefix: &str, segments: &[&str]) -> String {
    let mut path = String::from(prefix);
    for segment in segments {
        path.push('/');
        path.push_str(&query_value(segment));
    }
    path
}

/// The collection of the objects of `kind`: the route under which a uid names one.
fn collection(kind: SubjectKind) -> &'static str {
    match kind {
        SubjectKind::Cluster => CLUSTERS,
        SubjectKind::EdgeHost => EDGE_HOSTS,
        SubjectKind::ClusterGroup => CLUSTER_GROUPS,
    }
}

/// The search `route` with the page size of the dashboard, [`SEARCH_PAGE`], as its `limit`.
fn with_limit(route: &str) -> String {
    format!("{route}?limit={SEARCH_PAGE}")
}

/// `path` with `continue={next}` in its query, the page after the token `next`. `path` itself
/// for the first page, when `next` is empty.
fn page_after(path: &str, next: &str) -> String {
    if next.is_empty() {
        return path.to_string();
    }
    let join = if path.contains('?') { '&' } else { '?' };
    format!("{path}{join}continue={}", query_value(next))
}

/// The install order of a tier, when Palette gives one.
fn install_order(tier: &Value) -> Option<&Value> {
    tier.pointer(&format!("/spec/{TIER_INSTALL_ORDER}"))
}

/// The variables document of an object that has no variable: `{"variables": []}`.
fn no_variables() -> Value {
    serde_json::json!({"variables": []})
}

/// The variables of an object, as Palette gives them, and whether the route that gives them is
/// missing.
///
/// "No variables" and "strata could not read the variables" give the same root. Only
/// `route_missing` shows the difference, so a caller that records the answer must record the
/// flag too.
#[derive(Debug, Clone, PartialEq)]
pub struct VariablesRead {
    /// The answer of the route. When the route is missing, the empty answer of the object.
    pub answer: Value,
    /// True when Palette has no variables route, as an older Palette, and `answer` is empty.
    pub route_missing: bool,
}

/// The body of a search that selects each item, with no sort: `{"filter": {}, "sort": []}`.
fn search_all() -> Value {
    serde_json::json!({"filter": {}, "sort": []})
}

/// `(name, uid)` of each entry of a listing: an array, or an object with `items`.
pub(crate) fn named_entries(listing: &Value) -> Vec<(String, String)> {
    items(listing)
        .map(|e| (text(e, "/metadata/name"), text(e, "/metadata/uid")))
        .collect()
}

/// The uid of each entry of `entries`, `(name, uid)` pairs, that is named `name`.
fn uids_named(entries: &[(String, String)], name: &str) -> Vec<String> {
    entries
        .iter()
        .filter(|(n, _)| n == name)
        .map(|(_, uid)| uid.clone())
        .collect()
}

/// One version of a profile, as the profile listing gives it.
pub(crate) struct ProfileVersion {
    /// The profile name.
    pub name: String,
    /// The version.
    pub version: String,
    /// The uid of the version.
    pub uid: String,
    /// The cloud type of the profile, for example `aws`.
    pub cloud_type: String,
}

/// Each version of the profile listing, `items[]`, in the order Palette lists them.
pub(crate) fn profile_versions(listing: &Value) -> impl Iterator<Item = ProfileVersion> + '_ {
    items(listing).map(|it| ProfileVersion {
        name: text(it, "/metadata/name"),
        version: text(it, "/spec/version"),
        uid: text(it, "/metadata/uid"),
        cloud_type: text(it, "/spec/cloudType"),
    })
}

/// `text` as a value in a query string: each byte that is not a letter, a digit, or one of
/// `-._~:` becomes `%XX`, so a time with `+00:00` keeps its `+`.
fn query_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `(version, uid)` of each item of the profile listing named `name`.
///
/// An item with no version or no uid is not a version that a resolve can give, so the resolve
/// skips it. A name with no other item is then [`Error::ProfileNotFound`].
fn listed_versions<'a>(
    listing: &'a Value,
    name: &'a str,
) -> impl Iterator<Item = (String, String)> + 'a {
    profile_versions(listing)
        .filter(move |v| v.name == name && !v.version.is_empty() && !v.uid.is_empty())
        .map(|v| (v.version, v.uid))
}

/// The names of `entries`, sorted.
fn sorted_names(entries: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    names
}

/// A condition of the cluster search: the string property `property` equals one of `values`.
/// With `negation`, it equals none of them.
pub(crate) fn search_condition(property: &str, values: &[String], negation: bool) -> Value {
    serde_json::json!({
        "property": property, "type": SEARCH_STRING,
        "condition": {SEARCH_STRING: {"operator": "eq", "negation": negation, "ignoreCase": false,
            "match": {"conjunction": Conjunction::Or, "values": values}}},
    })
}

/// A group of the filter of the cluster search: `filters`, its conditions, joined by
/// `conjunction`.
pub(crate) fn search_group(conjunction: Conjunction, filters: Vec<Value>) -> Value {
    serde_json::json!({"conjunction": conjunction, "filters": filters})
}

/// The filter of the cluster search: `groups`, each a [`search_group`], joined with `and`.
pub(crate) fn search_filter(groups: Vec<Value>) -> Value {
    serde_json::json!({"conjunction": Conjunction::And, "filterGroups": groups})
}

/// `filter` with one more group, the condition that asks the search for the virtual clusters:
/// `environment` equals [`CLOUD_NESTED`]. `filter` joins its groups with `and`; `Null` has no
/// group, so the result selects each virtual cluster.
fn with_nested(filter: &Value) -> Value {
    let mut groups = arr(filter, "/filterGroups").to_vec();
    groups.push(search_group(
        Conjunction::And,
        vec![search_condition(
            SEARCH_ENVIRONMENT,
            &[CLOUD_NESTED.to_string()],
            false,
        )],
    ));
    search_filter(groups)
}

/// A kind of registry, as the routes of Palette name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RegistryKind {
    /// A pack registry. It holds the packs of the type `spectro`.
    Pack,
    /// A Helm registry.
    Helm,
    /// An OCI registry.
    Oci,
}

impl RegistryKind {
    /// The kind of the registry that holds a pack of the type `pack_type`: a pack registry holds
    /// the type `spectro`. None for a manifest pack, which no registry holds.
    pub(crate) fn of_pack_type(pack_type: PackType) -> Option<Self> {
        match pack_type {
            PackType::Spectro => Some(Self::Pack),
            PackType::Helm => Some(Self::Helm),
            PackType::Oci => Some(Self::Oci),
            PackType::Manifest => None,
        }
    }

    /// The kind of a registry as the metadata listing names it in `kind`: `pack`, `helm`, or
    /// `oci`, the words of the routes. None for another word.
    pub(crate) fn of_kind(kind: &str) -> Option<Self> {
        [Self::Pack, Self::Helm, Self::Oci]
            .into_iter()
            .find(|k| k.route() == kind)
    }

    /// The type of a pack of a registry of the kind: `spectro` for a pack registry, else the
    /// word of the kind.
    pub(crate) fn pack_type(self) -> PackType {
        match self {
            Self::Pack => PackType::Spectro,
            Self::Helm => PackType::Helm,
            Self::Oci => PackType::Oci,
        }
    }

    /// The segment of the kind in a route: `pack`, `helm`, or `oci`.
    fn route(self) -> &'static str {
        match self {
            Self::Pack => "pack",
            Self::Helm => "helm",
            Self::Oci => "oci",
        }
    }
}

/// The clusters that a search found, and whether the virtual clusters are among them.
#[derive(Debug)]
pub struct SearchedClusters {
    /// Each cluster that the filter selects, then each virtual cluster that it selects, when
    /// `includes_virtual`.
    pub items: Vec<Value>,
    /// True when the search takes `environment`, so `items` holds the virtual clusters too. False
    /// on a Palette whose search does not take it: the search then leaves each one out, and no
    /// condition can ask for them.
    pub includes_virtual: bool,
}

/// Profile-level operations over any [`PaletteApi`].
pub struct Palette<A: PaletteApi> {
    api: A,
}

impl<A: PaletteApi> Palette<A> {
    /// Wrap an API.
    pub fn new(api: A) -> Self {
        Palette { api }
    }

    /// The underlying API.
    pub fn api(&self) -> &A {
        &self.api
    }

    /// The project the API is scoped to, if any; snapshots taken through it record this.
    pub fn project_uid(&self) -> Option<&str> {
        self.api.project_uid()
    }

    /// Give each item of a listing that Palette pages by `offset` to `each`, every page. `fetch`
    /// reads the page at an offset. `each` returns false to stop the walk. Palette gives at most
    /// `page` items on a page, and `listmeta.count` the number of items. The walk ends at a page
    /// that is short, or when the offset reaches the count.
    pub(crate) fn each_offset_page(
        &self,
        page: usize,
        fetch: impl Fn(usize) -> Result<Value>,
        mut each: impl FnMut(Value) -> bool,
    ) -> Result<()> {
        let mut offset = 0;
        loop {
            let got = fetch(offset)?;
            let total = got
                .pointer("/listmeta/count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let items = take_items(got);
            let n = items.len();
            for it in items {
                if !each(it) {
                    return Ok(());
                }
            }
            offset += n;
            if n < page || offset >= total {
                return Ok(());
            }
        }
    }

    /// Every item of a listing that Palette pages by `continue`, every page.
    ///
    /// `path` is the first page, with its query when it has one, and `fetch` reads one page at a
    /// path. `listmeta.continue` names the next page, and is empty on the last one. A page with no
    /// items ends the listing too, and `items: null` is such a page.
    fn each_continue_page(
        &self,
        path: &str,
        fetch: impl Fn(&str) -> Result<Value>,
    ) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut next = String::new();
        loop {
            let page = fetch(&page_after(path, &next))?;
            next = text(&page, "/listmeta/continue");
            let got = take_items(page);
            let done = got.is_empty();
            items.extend(got);
            if next.is_empty() || done {
                return Ok(items);
            }
        }
    }

    /// Every item of a search of the dashboard, a POST of `body` to `search`, every page.
    fn searched(&self, search: &str, body: &Value) -> Result<Vec<Value>> {
        self.each_continue_page(search, |path| self.api.post_json(path, body))
    }

    /// Every item of the listing at `path`, a GET, every page.
    fn listed(&self, path: &str) -> Result<Vec<Value>> {
        self.each_continue_page(path, |path| self.api.get_json(path))
    }
}

/// The answer of a read beside the profiles of a cluster, or why Palette did not give it.
///
/// An answer of Palette that is an error does not stop the record. The record holds `HTTP
/// <status>: <body>` in place of the document, and a note names `what`: `notifications of
/// cluster <uid>`.
///
/// # Errors
///
/// Each error that is not an answer of Palette, such as a refused connection.
fn or_unread(
    answer: Result<Value>,
    what: &str,
    progress: &Progress,
) -> Result<(Option<Value>, Option<String>)> {
    match answer {
        Ok(doc) => Ok((Some(doc), None)),
        Err(Error::Api { status, body, .. }) => {
            unavailable_note(progress, what, status);
            Ok((None, Some(unread_reason(status, &body))))
        }
        Err(e) => Err(e),
    }
}

/// Note that Palette did not give `what`, and answered with the HTTP `status`.
fn unavailable_note(progress: &Progress, what: &str, status: u16) {
    progress.note(&format!("note: the {what} are unavailable (HTTP {status})"));
}

/// A part of the config of a cluster that Palette serves on a route of its own.
#[derive(Clone, Copy)]
enum ConfigPart {
    /// The role bindings of the cluster.
    Rbacs,
    /// The namespaces of the cluster, with the resources that each one can use.
    Namespaces,
}

impl ConfigPart {
    /// The last segment of the route of the part.
    fn route(self) -> &'static str {
        match self {
            ConfigPart::Rbacs => "rbacs",
            ConfigPart::Namespaces => "namespaces",
        }
    }

    /// What the part holds, for a note.
    fn noun(self) -> &'static str {
        match self {
            ConfigPart::Rbacs => "role bindings",
            ConfigPart::Namespaces => "namespaces",
        }
    }
}

/// Put the copies that a holder keeps of its manifests in place of the manifests of its
/// profiles.
///
/// Palette gives an edge host and a cluster group a copy of each manifest, with a uid of its
/// own. The template of the holder names each copy by that uid, with no content. An edit of the
/// profile does not reach the copy. `read_copy` reads one copy. Its arguments are the uid of the
/// profile, the name of the pack, and the uid of the copy.
fn own_manifests(
    templates: &[Value],
    read_copy: impl Fn(&str, &str, &str) -> Result<Value>,
    manifests: &mut Value,
) -> Result<()> {
    for (i, t) in templates.iter().enumerate() {
        let puid = str_at(t, "/uid");
        for pack in arr(t, "/packs") {
            let name = str_at(pack, "/name");
            let refs = arr(pack, "/manifests");
            // a copy with no uid cannot be read: the manifests of the profile stay
            if refs.is_empty() || refs.iter().any(|m| str_at(m, "/uid").is_empty()) {
                continue;
            }
            let mut own = Vec::with_capacity(refs.len());
            for m in refs {
                // Palette makes the entry of the template from the copy, so the entry has the
                // name and the uid of the copy. No route renames a copy, and a rename in the
                // profile reaches neither. So the name of the entry is the name of the copy,
                // which release 0.21 recorded.
                let got = read_copy(puid, name, str_at(m, "/uid"))?;
                own.push(manifest_of(&got, m, Template::Published));
            }
            let Some(packs) = manifests
                .pointer_mut(&format!("/profiles/{i}/spec/packs"))
                .and_then(Value::as_array_mut)
            else {
                continue;
            };
            if let Some(p) = packs
                .iter_mut()
                .find(|p| str_at(p, "/spec/name") == name || str_at(p, "/metadata/name") == name)
            {
                p["spec"]["manifests"] = Value::Array(own);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
