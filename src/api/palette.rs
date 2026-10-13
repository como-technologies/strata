//! The operations over any [`PaletteApi`]: find a profile or a cluster, and read the documents a
//! snapshot is built from.

use std::collections::BTreeSet;

use serde_json::Value;

use super::{AuditKind, PaletteApi, RawCluster, RawProfile, ReadErrors, mask};
use crate::cloud::{CLOUD_ALL, CLOUD_NESTED};
use crate::conjunction::Conjunction;
use crate::error::{Error, Result, found, unread_reason};
use crate::json::{arr, items, opt_text, str_at, take_items, text};
use crate::pack_type::PackType;
use crate::profile_type::ProfileType;
use crate::progress::Progress;
use crate::reference::{LATEST, one_named};
use crate::scope::{ProfileScope, ScopedUid};
use crate::snapshot::SubjectKind;
use crate::version;

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
/// The dashboard of a cluster group: its virtual clusters are under it.
const CLUSTER_GROUP_DASHBOARD: &str = "/v1/dashboard/clustergroups";
/// The notifications of a cluster: `/v1/notifications/spectrocluster/{uid}`.
const CLUSTER_NOTIFICATIONS: &str = "/v1/notifications/spectrocluster";
/// The audit log.
const AUDITS: &str = "/v1/audits";

/// A route of Palette: `prefix`, then each segment of `segments`, each escaped by
/// [`query_value`]. So a `/` or a `?` in a uid or a name stays in its segment. A fixed word of a
/// route, such as `packs`, is a segment too, and the escape leaves it as it is.
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

/// `(version, uid)` of each item of the profile listing named `name`. An item with no version
/// or no uid is not a version that a resolve can give: the resolve skips it, so a name with no
/// other item is [`Error::ProfileNotFound`].
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
        "property": property, "type": "string",
        "condition": {"string": {"operator": "eq", "negation": negation, "ignoreCase": false,
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

    /// Find the uid of `name` at `version` in the current scope. The version [`LATEST`] picks
    /// the highest version, see [`Palette::resolve_latest`].
    ///
    /// # Errors
    ///
    /// [`Error::ProfileNotFound`] when no version of the listing has the name and the version.
    /// [`Error::ProfileNameAmbiguous`] when two or more have them: a profile of the tenant and a
    /// profile of the project can have one name and one version.
    pub fn resolve(&self, name: &str, version: &str) -> Result<String> {
        if version == LATEST {
            return self.resolve_latest(name);
        }
        let listing = self.profile_listing()?;
        let uids = listed_versions(&listing, name)
            .filter(|(v, _)| v == version)
            .map(|(_, uid)| uid)
            .collect();
        self.one_of(name, version, uids)
    }

    /// The one uid of `uids`, the versions of `name` at `version` in the listing. None is
    /// [`Error::ProfileNotFound`]. Two or more are [`Error::ProfileNameAmbiguous`], each with
    /// the scope that its document gives.
    fn one_of(&self, name: &str, version: &str, uids: Vec<String>) -> Result<String> {
        one_named(
            uids,
            String::as_str,
            || Error::ProfileNotFound {
                name: name.to_string(),
                version: version.to_string(),
                tenant_scope: self.project_uid().is_none(),
            },
            |uids| self.ambiguous_profiles(name, version, uids),
        )
    }

    /// The error for `uids`, two or more versions of `name` at `version`:
    /// [`Error::ProfileNameAmbiguous`], with the scope that the document of each gives. A read of
    /// a document that fails gives its own error.
    fn ambiguous_profiles(&self, name: &str, version: &str, uids: Vec<String>) -> Error {
        let mut profiles = Vec::with_capacity(uids.len());
        for uid in uids {
            match self.profile_document(&uid) {
                Ok(doc) => profiles.push(ScopedUid {
                    scope: ProfileScope::of_document(&doc),
                    uid,
                }),
                Err(e) => return e,
            }
        }
        Error::ProfileNameAmbiguous {
            name: name.to_string(),
            version: version.to_string(),
            profiles,
        }
    }

    /// The uid of a profile that a ref names: `id` is a uid when there is no `version`, and else
    /// a name that [`Palette::resolve`] finds at `version`.
    pub(crate) fn profile_uid(&self, id: &str, version: Option<&str>) -> Result<String> {
        match version {
            Some(version) => self.resolve(id, version),
            None => Ok(id.to_string()),
        }
    }

    /// The uid of an app profile that a ref names. `id` is a uid when there is no `version`, and
    /// else a name that [`Palette::resolve_app_profile`] finds at `version`.
    pub(crate) fn app_profile_uid(&self, id: &str, version: Option<&str>) -> Result<String> {
        match version {
            Some(version) => self.resolve_app_profile(id, version),
            None => Ok(id.to_string()),
        }
    }

    /// Find the uid of the highest version of `name` in the current scope, by
    /// [`version::compare`].
    ///
    /// # Errors
    ///
    /// [`Error::ProfileNotFound`] for a name with no versions. [`Error::ProfileNameAmbiguous`]
    /// when two or more versions of the name are the highest. A profile of the tenant and a
    /// profile of the project can have one name and one version.
    pub fn resolve_latest(&self, name: &str) -> Result<String> {
        let listing = self.profile_listing()?;
        let versions: Vec<(String, String)> = listed_versions(&listing, name).collect();
        let Some((highest, _)) = version::pick(versions.iter(), |(v, _)| v.as_str(), LATEST) else {
            return Err(Error::ProfileNotFound {
                name: name.to_string(),
                version: LATEST.to_string(),
                tenant_scope: self.project_uid().is_none(),
            });
        };
        let uids = versions
            .iter()
            .filter(|(v, _)| version::compare(v, highest).is_eq())
            .map(|(_, uid)| uid.clone())
            .collect();
        self.one_of(name, highest, uids)
    }

    /// The uid of each version of each profile of `scope` in the current scope, from the profile
    /// search `POST /v1/dashboard/clusterprofiles?limit=50`, every page. The search gives one
    /// item for each profile name, with the uid of each version in `specSummary.versions[]`.
    /// `list` reads it for the scope of each version. The request changes nothing.
    pub(crate) fn profile_uids_in_scope(&self, scope: ProfileScope) -> Result<BTreeSet<String>> {
        let body = serde_json::json!({
            "filter": {"scope": scope.as_str()},
            "sort": [{"field": "profileName", "order": "asc"}],
        });
        let mut uids = BTreeSet::new();
        self.each_offset_page(
            SEARCH_PAGE,
            |offset| {
                let path = if offset == 0 {
                    with_limit(PROFILE_SEARCH)
                } else {
                    format!("{}&offset={offset}", with_limit(PROFILE_SEARCH))
                };
                self.api.post_json(&path, &body)
            },
            |item| {
                uids.extend(
                    arr(&item, "/specSummary/versions")
                        .iter()
                        .filter_map(|v| opt_text(v, "/uid")),
                );
                true
            },
        )?;
        Ok(uids)
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

    /// The uid of the app profile `name` at `version`, or at its highest version for
    /// [`LATEST`], from `GET /v1/dashboard/appProfiles/metadata`.
    pub fn resolve_app_profile(&self, name: &str, version: &str) -> Result<String> {
        let listing = self.api.get_json(APP_PROFILE_LISTING)?;
        let all: Vec<(String, String, String)> = arr(&listing, "/appProfiles")
            .iter()
            .map(|it| {
                (
                    text(it, "/metadata/name"),
                    text(it, "/spec/version"),
                    text(it, "/metadata/uid"),
                )
            })
            .collect();
        let named = all.iter().filter(|(n, _, _)| n == name);
        match version::pick(named, |(_, v, _)| v.as_str(), version) {
            Some((_, _, uid)) => Ok(uid.clone()),
            None => {
                let mut available: Vec<String> =
                    all.iter().map(|(n, v, _)| format!("{n}@{v}")).collect();
                available.sort();
                Err(Error::AppProfileNotFound {
                    name: name.to_string(),
                    version: version.to_string(),
                    available,
                })
            }
        }
    }

    /// Reads an app profile in the shape of a profile, so that a record of it has packs and a root.
    ///
    /// The requests are `GET /v1/appProfiles/{uid}` and `GET /v1/appProfiles/{uid}/tiers`. The
    /// content of each manifest of a tier comes from
    /// `GET /v1/appProfiles/{uid}/tiers/{tierUid}/manifests/{manifestUid}`. A tier has no digest.
    ///
    /// Each tier is a pack in the layer `app`. Its version is the tag, its source tier is the pack
    /// uid, and its properties are the params. The pack also holds the type and the values of the
    /// tier. The params also hold the install order of the tier and the registry of its image, when
    /// Palette gives them. The packs are in install order, and tiers of one order keep the order of
    /// the listing. So an app profile whose tiers have no install order keeps the root of an older
    /// record.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails.
    pub fn fetch_app_profile(&self, uid: &str, progress: &Progress) -> Result<RawProfile> {
        progress.note(&format!("fetching app profile {uid}"));
        let doc = self.app_profile_document(uid)?;
        let tiers = self.api.get_json(&route(APP_PROFILES, &[uid, "tiers"]))?;
        // Palette lists the tiers in the display order of the app profile, not in install order.
        let mut ordered: Vec<&Value> = arr(&tiers, "/spec/appTiers").iter().collect();
        // A stable sort: tiers of one order keep the order of the listing. Palette gives no order
        // for a tier at 0. An order that is not an integer comes first, and the record keeps it.
        ordered.sort_by_key(|tier| install_order(tier).map_or(Some(0), Value::as_i64));
        let (mut packs, mut mpacks) = (Vec::new(), Vec::new());
        for tier in ordered {
            let name = text(tier, "/metadata/name");
            let mut params: serde_json::Map<String, Value> = arr(tier, "/spec/properties")
                .iter()
                .map(|p| (text(p, "/name"), Value::from(text(p, "/value"))))
                .collect();
            if let Some(order) = install_order(tier) {
                params.insert(TIER_INSTALL_ORDER.to_string(), order.clone());
            }
            let registry = text(tier, &format!("/spec/{TIER_CONTAINER_REGISTRY}"));
            if !registry.is_empty() {
                params.insert(TIER_CONTAINER_REGISTRY.to_string(), Value::from(registry));
            }
            packs.push(serde_json::json!({
                "name": name, "layer": "app", "type": text(tier, "/spec/type"),
                "tag": text(tier, "/spec/version"), "version": text(tier, "/spec/version"),
                "packUid": text(tier, "/spec/sourceAppTierUid"),
                "registryUid": text(tier, "/spec/registryUid"),
                "values": text(tier, "/spec/values"), "params": params,
            }));
            let tier_uid = text(tier, "/metadata/uid");
            let mut manifests = Vec::new();
            for m in arr(tier, "/spec/manifests") {
                let d = self.api.get_json(&route(
                    APP_PROFILES,
                    &[uid, "tiers", &tier_uid, "manifests", &text(m, "/uid")],
                ))?;
                manifests.push(serde_json::json!({
                    "name": text(&d, "/metadata/name"), "uid": text(&d, "/metadata/uid"),
                    "content": text(&d, "/spec/published/content"),
                    "digest": text(&d, "/spec/published/digest"),
                }));
            }
            mpacks.push(serde_json::json!({"spec": {"name": name, "manifests": manifests}}));
        }
        Ok(RawProfile {
            profile: serde_json::json!({
                "metadata": doc["metadata"],
                "spec": {"version": doc["spec"]["version"],
                    "published": {"type": ProfileType::App.as_str(), "cloudType": CLOUD_ALL,
                        "packs": packs}},
            }),
            manifests: serde_json::json!({"spec": {"packs": mpacks}}),
            variables: serde_json::json!({ "variables": [] }),
            variables_unavailable: false,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            manifest_entities: None,
            resolved_values: None,
        })
    }

    /// The profile listing, `GET /v1/dashboard/clusterprofiles/metadata`: `items[]` with each
    /// version's name, uid, version, and cloud type. In a project it holds that project's
    /// profiles; at tenant scope, the tenant's own.
    pub fn profile_listing(&self) -> Result<Value> {
        self.api.get_json(PROFILE_LISTING)
    }

    /// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`: an array with each cluster's
    /// name, uid, cloud type, and state. At tenant scope it holds the clusters of every project.
    pub fn cluster_listing(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_LISTING)
    }

    /// The uid of the project named `name`, from [`Palette::project_listing`]. Call it at tenant
    /// scope: in a project, Palette lists only that project.
    pub fn project_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.project_listing()?;
        let projects = named_entries(&listing);
        match projects.iter().find(|(n, _)| n == name) {
            Some((_, uid)) => Ok(uid.clone()),
            None => Err(Error::ProjectNotFound {
                name: name.to_string(),
                available: sorted_names(&projects),
            }),
        }
    }

    /// The uid of the one cluster named `name` in the scope, from [`Palette::cluster_listing`]
    /// and from [`Palette::virtual_clusters`]: the listing leaves out each virtual cluster, so
    /// the search gives them, when it takes `environment`.
    ///
    /// # Errors
    ///
    /// [`Error::ClusterNotFound`] when no cluster has the name, and
    /// [`Error::ClusterNameAmbiguous`] when two or more have it.
    pub fn cluster_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.cluster_listing()?;
        let mut clusters = named_entries(&listing);
        let properties = self.cluster_search_properties()?;
        if let Some(found) = self.virtual_clusters(&properties, &Value::Null, || {})? {
            clusters.extend(named_entries(&Value::Array(found)));
        }
        one_named(
            uids_named(&clusters, name),
            String::as_str,
            || Error::ClusterNotFound {
                cluster: name.to_string(),
                available: sorted_names(&clusters),
            },
            |uids| Error::ClusterNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The project listing, `GET /v1/dashboard/projects/metadata`: `items[]` with each project's
    /// name and uid. At tenant scope it holds every project the key can see; in a project, only
    /// that project.
    pub fn project_listing(&self) -> Result<Value> {
        self.api.get_json(PROJECT_LISTING)
    }

    /// Reads the variables of the object at the route `owner`, `{owner}/variables`, and gives
    /// `none` when the route is missing.
    ///
    /// An older Palette answers 404 or 405 for the route. Each other error returns. Palette also
    /// answers 404 when it does not have the object. So after a 404, strata reads `owner`, the
    /// document of the object, again. An error of that read returns: the object is gone, and "no
    /// variables" is wrong.
    ///
    /// The second value is true when the route is missing and `none` is in its place. A caller
    /// must record that flag. "No variables" and "strata could not read the variables" give the
    /// same root, and only the flag shows the difference.
    fn variables_or(&self, owner: &str, none: Value, progress: &Progress) -> Result<(Value, bool)> {
        let path = format!("{owner}/variables");
        match self.api.get_json(&path) {
            Ok(v) => Ok((v, false)),
            Err(e @ Error::Api { status, .. }) if e.is_missing_route() => {
                if status == 404 {
                    self.api.get_json(owner)?;
                }
                progress.note(&format!(
                    "note: {path} is unavailable; treating as no variables"
                ));
                Ok((none, true))
            }
            Err(e) => Err(e),
        }
    }

    /// Fetch the three documents for one profile. If the variables endpoint answers 404 or 405, as
    /// on older Palette versions, the profile has no variables. If the profile is gone, the error
    /// of its document returns. Other errors return. For a profile that has a draft, also each of
    /// its manifests: see [`RawProfile::manifest_entities`].
    /// Palette serves the draft packs with each secret in clear: strata masks them first, see
    /// `mask_draft`.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails.
    pub fn fetch_profile(&self, uid: &str, progress: &Progress) -> Result<RawProfile> {
        progress.note(&format!("fetching profile {uid}"));
        let mut profile = self.profile_document(uid)?;
        let mut manifests = self.profile_pack_manifests(uid)?;
        self.mask_draft(&mut profile, &mut manifests, progress)?;
        let manifest_entities = self.manifest_entities(uid, &profile, progress)?;
        let (variables, variables_unavailable) = self.variables_or(
            &route(PROFILES, &[uid]),
            serde_json::json!({ "variables": [] }),
            progress,
        )?;
        Ok(RawProfile {
            profile,
            manifests,
            variables,
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            manifest_entities,
            resolved_values: None,
        })
    }

    /// Write `********` over each secret of the draft packs of a profile, as Palette does for the
    /// published packs. Palette masks a value whose schema format is `password` in
    /// `spec.published.packs`, but not in `spec.draft.packs`, and `packs/manifests` serves the
    /// draft packs when the profile has a draft. The schema of each pack comes from
    /// `GET /v1/packs/{uid}`. A pack that Palette no longer has (404) has no schema, so Palette
    /// masks nothing in it, and strata does the same.
    fn mask_draft(
        &self,
        profile: &mut Value,
        manifests: &mut Value,
        progress: &Progress,
    ) -> Result<()> {
        let mut secrets = std::collections::BTreeMap::new();
        for uid in mask::draft_pack_uids(profile) {
            progress.note(&format!(
                "reading the schema of pack {uid} to mask the draft"
            ));
            if let Some(pack) = found(self.api.get_json(&route(PACKS, &[&uid])))? {
                let fields = mask::password_fields(&pack, &uid);
                if !fields.is_empty() {
                    secrets.insert(uid, fields);
                }
            }
        }
        mask::mask_draft_packs(profile, manifests, &secrets)
    }

    /// Each manifest of the two templates of a profile that has a draft, by manifest uid:
    /// `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}`, which gives the
    /// draft content and the published content. `None` for a profile with no draft.
    ///
    /// With a draft, `packs/manifests` lists the packs of the draft, and gives the published
    /// content of each manifest. So it gives the content of neither template: a manifest that the
    /// draft adds or edits, and each manifest of a version that was never published, has the
    /// wrong content or none.
    fn manifest_entities(
        &self,
        uid: &str,
        profile: &Value,
        progress: &Progress,
    ) -> Result<Option<Value>> {
        if !super::raw::has_draft(profile) {
            return Ok(None);
        }
        let mut out = serde_json::Map::new();
        for template in ["/spec/published/packs", "/spec/draft/packs"] {
            for pack in arr(profile, template) {
                let name = text(pack, "/name");
                for m in arr(pack, "/manifests") {
                    let manifest = text(m, "/uid");
                    if name.is_empty() || manifest.is_empty() || out.contains_key(&manifest) {
                        continue;
                    }
                    progress.note(&format!(
                        "reading manifest {manifest} of pack {name}: the profile has a draft"
                    ));
                    let doc = self.profile_manifest(uid, &name, &manifest)?;
                    out.insert(manifest, doc);
                }
            }
        }
        Ok(Some(Value::Object(out)))
    }

    /// The profile document alone, `GET /v1/clusterprofiles/{uid}`: metadata, versions, and the
    /// clusters that use it in `status`.
    pub fn profile_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(PROFILES, &[uid]))
    }

    /// The pack manifests of the profile version `uid`,
    /// `GET /v1/clusterprofiles/{uid}/packs/manifests`: the manifests of each pack of the
    /// template, with the published content of each.
    fn profile_pack_manifests(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&route(PROFILES, &[uid, "packs", "manifests"]))
    }

    /// One manifest of a pack of the profile version `uid`,
    /// `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}`: its draft
    /// content and its published content.
    fn profile_manifest(&self, uid: &str, pack: &str, manifest: &str) -> Result<Value> {
        self.api.get_json(&route(
            PROFILES,
            &[uid, "packs", pack, "manifests", manifest],
        ))
    }

    /// The copy of a manifest that the edge host `uid` holds,
    /// `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}`.
    fn edge_host_manifest(&self, uid: &str, manifest: &str) -> Result<Value> {
        self.api
            .get_json(&route(EDGE_HOSTS, &[uid, "pack", "manifests", manifest]))
    }

    /// The app profile document alone, `GET /v1/appProfiles/{uid}`: metadata, version, and the
    /// app deployments that use it in `status.inUseApps`.
    pub fn app_profile_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(APP_PROFILES, &[uid]))
    }

    /// One app deployment, `GET /v1/appDeployments/{uid}`: its name, its project in the
    /// `projectUid` annotation, the app profile version its copy names in
    /// `spec.profile.metadata.version`, and its state in `status.state`.
    pub fn app_deployment_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(APP_DEPLOYMENTS, &[uid]))
    }

    /// The entries of the audit log of Palette for one resource, `GET /v1/audits`, that it logged
    /// at `since` or after, every page. With no `kind`, Palette gives the entries of each kind
    /// for the uid. Palette lists them newest first, and at most
    /// 100 on a page; `listmeta.continue` names the next page, and is empty on the last one. An
    /// empty `since` reads the full log of the resource.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails.
    pub fn audits(&self, kind: Option<AuditKind>, uid: &str, since: &str) -> Result<Vec<Value>> {
        let mut query = Vec::new();
        if let Some(kind) = kind {
            query.push(format!("resourceKind={}", kind.as_str()));
        }
        query.push(format!("resourceUid={}", query_value(uid)));
        if !since.is_empty() {
            query.push(format!("startTime={}", query_value(since)));
        }
        query.push(format!("limit={AUDIT_PAGE}"));
        self.listed(&format!("{AUDITS}?{}", query.join("&")))
    }

    /// The cluster summary, `GET /v1/dashboard/spectroclusters/{uid}`: what the dashboard shows
    /// for one cluster. Its `status.health` is the health of the cluster, which the cluster
    /// document does not carry.
    pub fn fetch_cluster_summary(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(CLUSTER_SUMMARIES, &[uid]))
    }

    /// The raw documents of the cluster, edge host, or cluster group `id`, in the shape of a
    /// cluster. `id` is a uid, or, when no object of `kind` has that uid, the name of one object
    /// of the kind in the scope. The documents are read by uid in each case.
    ///
    /// A cluster: its document with its applied profile templates, the pack manifests of each
    /// applied profile, and the variables of each. An edge host: `GET /v1/edgehosts/{uid}` gives
    /// the bound profiles in `spec.clusterProfileTemplates`. The host holds its own copy of each
    /// manifest, which `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}` reads. A cluster
    /// group: `GET /v1/clustergroups/{uid}` gives the profiles that the group pins for its
    /// virtual clusters. The group holds its own copy of each manifest, which
    /// `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifestUid}` reads by its uid.
    ///
    /// The variables and the rest of each pack of a host or a group come from its profile, and
    /// neither has notifications. A missing variables route is "no variables". Each other error
    /// returns.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails. [`Error::ClusterNotFound`],
    /// [`Error::EdgeHostNotFound`], or [`Error::ClusterGroupNotFound`] when `id` is no uid and no
    /// name in the scope. The `NameAmbiguous` kind of the same object when two have the name.
    pub fn fetch_subject(
        &self,
        kind: SubjectKind,
        id: &str,
        progress: &Progress,
    ) -> Result<RawCluster> {
        let doc = self.subject_by_id(kind, id, progress)?;
        self.subject_raw(kind, doc, progress)
    }

    /// The document of the cluster, edge host, or cluster group with the uid `uid`, and no
    /// other: `GET /v1/spectroclusters/{uid}`, `GET /v1/edgehosts/{uid}`, or `GET
    /// /v1/clustergroups/{uid}`. A 404 is an error, and no name is tried. Palette keeps the
    /// document of a deleted cluster, with the state `Deleted`.
    pub fn subject_document(&self, kind: SubjectKind, uid: &str) -> Result<Value> {
        self.api.get_json(&route(collection(kind), &[uid]))
    }

    /// The document of the object `id` of `kind`: [`Palette::subject_document`] of the uid `id`.
    /// When no object has that uid, it is the document of the one object named `id` in the
    /// scope. A note names the uid of a name.
    fn subject_by_id(&self, kind: SubjectKind, id: &str, progress: &Progress) -> Result<Value> {
        progress.note(&format!("fetching {} {id}", kind.noun()));
        match found(self.subject_document(kind, id))? {
            Some(doc) => Ok(doc),
            None => {
                let uid = self.subject_uid_named(kind, id)?;
                progress.note(&format!("{} '{id}' is {uid}", kind.noun()));
                self.subject_document(kind, &uid)
            }
        }
    }

    /// The uid of the one object of `kind` in the scope that has the name `name`.
    fn subject_uid_named(&self, kind: SubjectKind, name: &str) -> Result<String> {
        match kind {
            SubjectKind::Cluster => self.cluster_uid_named(name),
            SubjectKind::EdgeHost => self.edge_host_uid_named(name),
            SubjectKind::ClusterGroup => self.cluster_group_uid_named(name),
        }
    }

    /// The raw documents of a cluster, an edge host, or a cluster group, from the document that
    /// [`Palette::subject_document`] gives: the rest of what [`Palette::fetch_subject`] reads.
    pub fn subject_raw(
        &self,
        kind: SubjectKind,
        doc: Value,
        progress: &Progress,
    ) -> Result<RawCluster> {
        let uid = text(&doc, "/metadata/uid");
        match kind {
            SubjectKind::Cluster => self.cluster_raw(&uid, doc, progress),
            // an edge host keeps each copy of a manifest under its own uid, and a cluster group
            // keeps it with the profile
            SubjectKind::EdgeHost => self.holder_raw(doc, progress, |_, _, muid| {
                self.edge_host_manifest(&uid, muid)
            }),
            SubjectKind::ClusterGroup => self.holder_raw(doc, progress, |puid, name, muid| {
                self.profile_manifest(puid, name, muid)
            }),
        }
    }

    /// The rest of the raw documents of the cluster `uid`, whose document is `cluster`.
    fn cluster_raw(&self, uid: &str, cluster: Value, progress: &Progress) -> Result<RawCluster> {
        let manifests = self
            .api
            .get_json(&route(CLUSTERS, &[uid, "profiles", "packs", "manifests"]))?;
        let (variables, variables_unavailable) =
            self.variables_or(&route(CLUSTERS, &[uid]), serde_json::json!([]), progress)?;
        // the updates and the repave that wait, the bindings, and the namespaces are beside the
        // copy, not part of it. So strata records the error of a read, and still takes the record
        let (notifications, notifications_error) = or_unread(
            self.cluster_notifications(uid).map(Value::Array),
            &format!("notifications of cluster {uid}"),
            progress,
        )?;
        // the cluster document gives only the state of a repave; its own route gives the reasons
        let (repave, repave_error) = if crate::repave::is_pending(&cluster) {
            or_unread(
                self.cluster_repave(uid),
                &format!("reasons of the repave of cluster {uid}"),
                progress,
            )?
        } else {
            (None, None)
        };
        let (rbacs, rbacs_error) = self.cluster_config(uid, ConfigPart::Rbacs, progress)?;
        let (namespaces, namespaces_error) =
            self.cluster_config(uid, ConfigPart::Namespaces, progress)?;
        Ok(RawCluster {
            cluster,
            manifests,
            variables,
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            notifications,
            repave,
            resolved_values: None,
            cloud_config: None,
            rbacs,
            namespaces,
            errors: ReadErrors {
                notifications: notifications_error,
                repave: repave_error,
                rbacs: rbacs_error,
                namespaces: namespaces_error,
            },
        })
    }

    /// The raw documents of a cluster as they would be after the cluster accepts the update that
    /// waits: `GET /v1/spectroclusters/{uid}/profileUpdates` gives `profiles[]`, each a profile
    /// template in the shape of `spec.clusterProfileTemplates`, and they take the place of the
    /// templates of the cluster. The manifests of each come from its profile, `GET
    /// /v1/clusterprofiles/{uid}/packs/manifests`, since the cluster holds no copy of them yet.
    pub fn with_pending(&self, mut raw: RawCluster) -> Result<RawCluster> {
        let uid = text(&raw.cluster, "/metadata/uid");
        let updates = self
            .api
            .get_json(&route(CLUSTERS, &[&uid, "profileUpdates"]))?;
        let profiles = arr(&updates, "/profiles").to_vec();
        raw.manifests = self.profile_manifests(&profiles)?;
        raw.cluster["spec"]["clusterProfileTemplates"] = Value::Array(profiles);
        Ok(raw)
    }

    /// The manifests of the profile of each template, `GET
    /// /v1/clusterprofiles/{uid}/packs/manifests`, in the shape of the manifests of a cluster:
    /// `{"profiles": [{metadata, spec}]}`.
    fn profile_manifests(&self, templates: &[Value]) -> Result<Value> {
        let mut manifests = Vec::with_capacity(templates.len());
        for t in templates {
            let d = self.profile_pack_manifests(str_at(t, "/uid"))?;
            manifests.push(serde_json::json!({"metadata": d["metadata"], "spec": d["spec"]}));
        }
        Ok(serde_json::json!({ "profiles": manifests }))
    }

    /// Every edge host the scope can list, `POST /v1/dashboard/edgehosts/search`, every page.
    /// Each item names the profiles bound to the host in `spec.clusterProfileTemplates`, as a
    /// summary: uid, name, and version.
    pub fn edge_hosts(&self) -> Result<Vec<Value>> {
        self.searched(
            &with_limit(EDGE_HOST_SEARCH),
            &serde_json::json!({"filter": {}, "sort": []}),
        )
    }

    /// The clusters that a search filter selects, `POST /v1/dashboard/spectroclusters/search`,
    /// every page. `filter` is the `filter` of the body: groups of conditions on the properties
    /// that `cluster_search_schema` names. At tenant scope it holds the clusters of each project
    /// the key can read, each with its labels and its project. It holds no virtual cluster,
    /// unless a condition asks for the environment `nested`: see
    /// [`Palette::virtual_cluster_search`].
    pub fn cluster_search(&self, filter: &Value) -> Result<Vec<Value>> {
        self.searched(
            &with_limit(CLUSTER_SEARCH),
            &serde_json::json!({"filter": filter, "sort": [{"field": "clusterName", "order": "asc"}]}),
        )
    }

    /// The virtual clusters that a search filter selects: [`Palette::cluster_search`] with the
    /// condition `environment eq nested` added to `filter` as one more group. The search leaves
    /// out each virtual cluster unless a condition asks for that environment, the cloud type of
    /// a virtual cluster. `filter` joins its groups with `and`, and `Null` has no condition, so
    /// the answer then holds each virtual cluster in the scope. Each item names the host cluster
    /// and the cluster group of the virtual cluster in `status.virtual`.
    pub fn virtual_cluster_search(&self, filter: &Value) -> Result<Vec<Value>> {
        self.cluster_search(&with_nested(filter))
    }

    /// The virtual clusters that `filter` selects, when the search takes `environment`.
    ///
    /// It is [`Palette::virtual_cluster_search`] behind the one check of that property.
    /// `properties` is the answer of [`Palette::cluster_search_properties`]. `None` when the
    /// search does not take `environment`: no condition can then ask for a virtual cluster, and
    /// the search leaves each one out. `on_search` runs before the search, when there is one, so
    /// that a caller can note it.
    pub fn virtual_clusters(
        &self,
        properties: &BTreeSet<String>,
        filter: &Value,
        on_search: impl FnOnce(),
    ) -> Result<Option<Vec<Value>>> {
        if !properties.contains(SEARCH_ENVIRONMENT) {
            return Ok(None);
        }
        on_search();
        self.virtual_cluster_search(filter).map(Some)
    }

    /// The clusters that `filter` selects, with the virtual clusters among them.
    ///
    /// The clusters come from [`Palette::cluster_search`], and the virtual clusters from
    /// [`Palette::virtual_clusters`], when the search takes `environment`. `properties` is the
    /// answer of [`Palette::cluster_search_properties`], and `on_virtual` runs before the search
    /// of the virtual clusters, when there is one.
    pub fn cluster_search_with_virtual(
        &self,
        properties: &BTreeSet<String>,
        filter: &Value,
        on_virtual: impl FnOnce(),
    ) -> Result<SearchedClusters> {
        let mut items = self.cluster_search(filter)?;
        let nested = self.virtual_clusters(properties, filter, on_virtual)?;
        let includes_virtual = nested.is_some();
        items.extend(nested.into_iter().flatten());
        Ok(SearchedClusters {
            items,
            includes_virtual,
        })
    }

    /// The properties that the cluster search takes, `GET
    /// /v1/dashboard/spectroclusters/search/schema`: `schema.properties[]` with `name` and `type`.
    pub fn cluster_search_schema(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_SEARCH_SCHEMA)
    }

    /// The names of the string properties that the cluster search takes, from
    /// [`Palette::cluster_search_schema`]. Empty when Palette has no search: it answers 404 or
    /// 405 for the schema.
    ///
    /// # Errors
    ///
    /// Each other error of the request.
    pub fn cluster_search_properties(&self) -> Result<BTreeSet<String>> {
        match self.cluster_search_schema() {
            Ok(schema) => Ok(arr(&schema, "/schema/properties")
                .iter()
                .filter(|p| str_at(p, "/type") == "string")
                .filter_map(|p| opt_text(p, "/name"))
                .collect()),
            Err(e) if e.is_missing_route() => Ok(BTreeSet::new()),
            Err(e) => Err(e),
        }
    }

    /// The saved tag filters, `GET /v1/filters/metadata?filterType=tag`: the uid and the name of
    /// each. A tag filter belongs to the tenant.
    pub fn tag_filters(&self) -> Result<Value> {
        self.api.get_json(TAG_FILTER_LISTING)
    }

    /// One saved tag filter, `GET /v1/filters/tag/{uid}`: `spec.filterGroup` holds its
    /// conjunction and its conditions.
    pub fn tag_filter(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(TAG_FILTERS, &[uid]))
    }

    /// Every item of a listing that Palette pages by `continue`, every page. `path` is the first
    /// page, with its query when it has one, and `fetch` reads one page at a path.
    /// `listmeta.continue` names the next page, and is empty on the last one. A page with no
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

    /// The virtual clusters of a cluster group, `POST
    /// /v1/dashboard/clustergroups/{uid}/virtualClusters`, every page: the clusters that run the
    /// profiles that the group pins.
    pub fn cluster_group_members(&self, uid: &str) -> Result<Vec<Value>> {
        self.searched(
            &with_limit(&route(CLUSTER_GROUP_DASHBOARD, &[uid, "virtualClusters"])),
            &serde_json::json!({"filter": {}, "sort": []}),
        )
    }

    /// The name and the uid of each cluster group in the scope, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    pub fn cluster_groups(&self) -> Result<Vec<(String, String)>> {
        let listing = self.api.get_json(CLUSTER_GROUP_LISTING)?;
        Ok(items(&listing)
            .map(|g| (text(g, "/name"), text(g, "/uid")))
            .collect())
    }

    /// The uid of the one cluster group in the scope that has the name `name`, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    fn cluster_group_uid_named(&self, name: &str) -> Result<String> {
        let groups = self.cluster_groups()?;
        one_named(
            uids_named(&groups, name),
            String::as_str,
            || Error::ClusterGroupNotFound {
                cluster_group: name.to_string(),
                available: sorted_names(&groups),
            },
            |uids| Error::ClusterGroupNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The raw documents of an object that holds profiles in `spec.clusterProfileTemplates` and
    /// is not a cluster: the manifests and the variables of each profile come from the profile.
    /// Such an object gives no values to the variables, so a record holds the definitions of each
    /// profile, as a profile record does. It has no notifications. `read_copy` reads the copy
    /// that the holder keeps of one manifest. Its arguments are the uid of the profile, the name
    /// of the pack, and the uid of the copy.
    fn holder_raw(
        &self,
        doc: Value,
        progress: &Progress,
        read_copy: impl Fn(&str, &str, &str) -> Result<Value>,
    ) -> Result<RawCluster> {
        let templates = arr(&doc, "/spec/clusterProfileTemplates").to_vec();
        let mut variables = Vec::with_capacity(templates.len());
        let mut variables_unavailable = false;
        for t in &templates {
            let puid = str_at(t, "/uid");
            let (v, unavailable) = self.variables_or(
                &route(PROFILES, &[puid]),
                serde_json::json!({ "variables": [] }),
                progress,
            )?;
            variables_unavailable |= unavailable;
            variables.push(serde_json::json!({"profileUid": puid, "variables": v["variables"]}));
        }
        let mut manifests = self.profile_manifests(&templates)?;
        own_manifests(&templates, read_copy, &mut manifests)?;
        Ok(RawCluster {
            manifests,
            cluster: doc,
            variables: Value::Array(variables),
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            notifications: None,
            repave: None,
            resolved_values: None,
            cloud_config: None,
            rbacs: None,
            namespaces: None,
            errors: ReadErrors::default(),
        })
    }

    /// The uid of the one edge host in the scope that has the name `name`.
    fn edge_host_uid_named(&self, name: &str) -> Result<String> {
        let hosts = named_entries(&Value::Array(self.edge_hosts()?));
        one_named(
            uids_named(&hosts, name),
            String::as_str,
            || Error::EdgeHostNotFound {
                edge_host: name.to_string(),
                available: sorted_names(&hosts),
            },
            |uids| Error::EdgeHostNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The cloud config of a cluster, `GET /v1/cloudconfigs/{cloudType}/{configUid}`, from the
    /// `spec.cloudConfigRef` of the cluster document: its machine pools and its cloud part.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] when the cluster document names no cloud config. The error of the first
    /// request that fails.
    pub fn cloud_config(&self, cluster: &Value) -> Result<Value> {
        let kind = RawCluster::cloud_type(cluster);
        let uid = str_at(cluster, "/spec/cloudConfigRef/uid");
        if kind.is_empty() || uid.is_empty() {
            return Err(Error::Usage {
                message: "the cluster document names no cloud config".to_string(),
            });
        }
        self.api.get_json(&route(CLOUD_CONFIGS, &[kind, uid]))
    }

    /// The value Palette gives each macro for the cluster or the cluster group `uid`: `GET
    /// /v1/spectroclusters/{uid}/packs/resolvedValues` or `GET
    /// /v1/clustergroups/{uid}/packs/resolvedValues`. The answer has `profiles[]`, each with its
    /// `uid` and a `resolved` map from each placeholder, such as `{{.spectro.macro.region}}`, to
    /// its value. The map holds every macro Palette can resolve for the profile, the system ones
    /// too, and not only the ones it uses.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] for an edge host, before any request: strata reads no macro values for
    /// an edge host. An error of the request otherwise.
    pub fn resolved_values(&self, kind: SubjectKind, uid: &str) -> Result<Value> {
        if kind == SubjectKind::EdgeHost {
            return Err(Error::Usage {
                message: "strata reads no macro values for an edge host".to_string(),
            });
        }
        self.api
            .get_json(&route(collection(kind), &[uid, "packs", "resolvedValues"]))
    }

    /// The value Palette gives each macro at the scope of the profile version `uid`, with no
    /// cluster: `GET /v1/clusterprofiles/{uid}/packs/resolvedValues`. The answer is one
    /// `resolved` map from each placeholder to its value, with the user macros of the scope of the
    /// profile and the system macros. A system macro of a cluster, such as
    /// `{{.spectro.system.cluster.name}}`, has an empty value there.
    ///
    /// # Errors
    ///
    /// An error of the request, such as a 404 for a version that Palette does not have.
    pub fn profile_resolved_values(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&route(PROFILES, &[uid, "packs", "resolvedValues"]))
    }

    /// Palette's answer to whether a change of the profiles of a cluster replaces its nodes:
    /// `POST /v1/spectroclusters/{uid}/validate/repave`. The request changes nothing.
    pub fn validate_repave(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(CLUSTERS, &[uid, "validate", "repave"]), body)
    }

    /// Palette's answer to whether a change of the profiles of the cluster template `uid` replaces
    /// the nodes of its clusters: `POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave`.
    /// The body names profiles by uid, with no pack values. The request changes nothing.
    ///
    /// # Errors
    ///
    /// An error of the request, such as a 404 for a template that Palette does not have.
    pub fn validate_template_repave(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api.post_json(
            &route(CLUSTERS, &["clusterTemplates", uid, "validate", "repave"]),
            body,
        )
    }

    /// Palette's answer to whether it accepts the packs of a change of the profiles of a cluster:
    /// `POST /v1/spectroclusters/{uid}/validate/packs`, with the body of `validate/repave`. The
    /// request changes nothing.
    pub fn validate_packs(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(CLUSTERS, &[uid, "validate", "packs"]), body)
    }

    /// Palette's answer to whether it takes the packs of a profile: `POST
    /// /v1/clusterprofiles/validate/packs` for a profile that Palette does not hold, or `POST
    /// /v1/clusterprofiles/{uid}/validate/packs` for an update of the version `uid`. The body is
    /// a `v1ClusterProfileTemplateDraft`. The request changes nothing.
    pub fn validate_profile_packs(&self, uid: Option<&str>, body: &Value) -> Result<Value> {
        let path = match uid {
            Some(uid) => route(PROFILES, &[uid, "validate", "packs"]),
            None => route(PROFILES, &["validate", "packs"]),
        };
        self.api.post_json(&path, body)
    }

    /// Palette's answer to whether it takes `name` and `version` for a new profile version: `GET
    /// /v1/clusterprofiles/validate/name`. Palette answers `204` with no body when it takes them.
    /// The request changes nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] for each answer of Palette that is an error. A refusal of the name or the
    /// version is one: HTTP 500 with a code of the name route, or HTTP 409 with
    /// [`PROFILE_EXISTS_CODE`](super::codes::PROFILE_EXISTS_CODE). See
    /// [`ValidateRoute::ProfileName`](super::codes::ValidateRoute::ProfileName). Each other error
    /// of the request, such as a refused connection.
    pub fn validate_profile_name(&self, name: &str, version: &str) -> Result<()> {
        self.api
            .get_json(&format!(
                "{}?name={}&version={}",
                route(PROFILES, &["validate", "name"]),
                query_value(name),
                query_value(version)
            ))
            .map(drop)
    }

    /// Palette's answer to whether it takes a profile as an import: `POST
    /// /v1/clusterprofiles/import/validate`. The body is a `v1ClusterProfileImportEntity`, the
    /// shape of a profile export. The request changes nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Api`] for each answer of Palette that is an error. A refusal of the profile is
    /// one of these answers:
    ///
    /// - HTTP 500 with a code of the import validate route:
    ///   [`ValidateRoute::ProfileImport`](super::codes::ValidateRoute::ProfileImport).
    /// - HTTP 409 with [`PROFILE_EXISTS_CODE`](super::codes::PROFILE_EXISTS_CODE).
    /// - HTTP 404 with [`PACK_MACRO_UNDEFINED_CODE`](super::codes::PACK_MACRO_UNDEFINED_CODE).
    /// - The HTTP 422 of the API schema.
    ///
    /// Each other error of the request, such as a refused connection.
    pub fn validate_profile_import(&self, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(PROFILES, &["import", "validate"]), body)
    }

    /// One part of the config of the cluster `uid`, `GET
    /// /v1/spectroclusters/{uid}/config/{part}`: its role bindings or its namespaces, and why
    /// Palette did not give them.
    ///
    /// Palette gives `items`, one entry for each. A 404 gives no part and no error, so the record
    /// holds null for the part, and not an empty list. Another answer of Palette that is an error
    /// gives no part, and its error: see [`or_unread`].
    ///
    /// # Errors
    ///
    /// Each error of the request that is not an answer of Palette.
    fn cluster_config(
        &self,
        uid: &str,
        part: ConfigPart,
        progress: &Progress,
    ) -> Result<(Option<Value>, Option<String>)> {
        let path = route(CLUSTERS, &[uid, "config", part.route()]);
        let what = format!("{} of cluster {uid}", part.noun());
        match self.api.get_json(&path) {
            Err(Error::Api { status: 404, .. }) => {
                unavailable_note(progress, &what, 404);
                Ok((None, None))
            }
            answer => or_unread(answer, &what, progress),
        }
    }

    /// The last repave of one cluster, `GET /v1/spectroclusters/{uid}/repave/status`: its source,
    /// its reasons, and its state. The pack ref on each side of a reason holds the values of the
    /// pack, with the value of each macro in its place.
    pub(crate) fn cluster_repave(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&route(CLUSTERS, &[uid, "repave", "status"]))
    }

    /// The notifications of one cluster, `GET /v1/notifications/spectrocluster/{uid}`, every page,
    /// newest first. Palette gives `items: null` when there are none, and at most 50 on a page;
    /// `listmeta.continue` names the next page, and is empty on the last one.
    pub fn cluster_notifications(&self, uid: &str) -> Result<Vec<Value>> {
        self.listed(&route(CLUSTER_NOTIFICATIONS, &[uid]))
    }

    /// One page of the pack listing of `name`,
    /// `GET /v1/packs?filters=spec.name={name}&limit=100&offset={offset}`. It gives each version
    /// of the pack in each registry, [`PACK_LISTING_PAGE`] on a page. `listmeta.count` counts the
    /// whole listing.
    pub(crate) fn pack_listing_page(&self, name: &str, offset: usize) -> Result<Value> {
        self.api.get_json(&format!(
            "{PACKS}?filters=spec.name={}&limit={PACK_LISTING_PAGE}&offset={offset}",
            query_value(name)
        ))
    }

    /// The tags of the pack `name` in the registry `registry_uid`,
    /// `GET /v1/packs/{name}/registries/{uid}`. Each tag names its version. The answer also
    /// holds the values of the pack, and its layer. Palette answers 404 when the registry serves
    /// no version of the pack.
    ///
    /// With `states`, a list such as `deprecated,disabled,deleted`, the request adds the query
    /// `states=<states>`, and Palette gives the versions in those states only. The crate gives
    /// the list, as the route reads it, so it goes in with no escape.
    pub(crate) fn pack_tags(
        &self,
        name: &str,
        registry_uid: &str,
        states: Option<&str>,
    ) -> Result<Value> {
        let path = route(PACKS, &[name, "registries", registry_uid]);
        match states {
            Some(states) => self.api.get_json(&format!("{path}?states={states}")),
            None => self.api.get_json(&path),
        }
    }

    /// The registries that the key can see, `GET /v1/registries/metadata`: the `uid`, the
    /// `name`, the `kind`, and `isPrivate` of each, as `items`.
    pub(crate) fn registry_metadata(&self) -> Result<Value> {
        self.api.get_json(REGISTRY_METADATA)
    }

    /// The summary listing of the registries of one `kind`, `GET /v1/registries/{kind}/summary`.
    /// Each registry is an entry of `items`, with its last sync in `status.sync`. An OCI registry
    /// also has its provider type in `spec.providerType`.
    pub(crate) fn registry_summary(&self, kind: RegistryKind) -> Result<Value> {
        self.api
            .get_json(&route(REGISTRIES, &[kind.route(), "summary"]))
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
/// profiles. Palette gives an edge host and a cluster group a copy of each manifest under a
/// uid of its own, and the template of the holder names each copy by that uid, with no
/// content. An edit of the profile does not reach the copy. `read_copy` reads one copy. Its
/// arguments are the uid of the profile, the name of the pack, and the uid of the copy.
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
                let muid = str_at(m, "/uid");
                let got = read_copy(puid, name, muid)?;
                own.push(serde_json::json!({
                    "name": got.pointer("/metadata/name").cloned()
                        .or_else(|| m.get("name").cloned()).unwrap_or(Value::Null),
                    "uid": muid,
                    "content": got.pointer("/spec/published/content").cloned().unwrap_or(Value::Null),
                    "digest": got.pointer("/spec/published/digest").cloned().unwrap_or(Value::Null),
                }));
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
mod tests {
    use super::*;
    use crate::api::MemoryApi;
    use serde_json::json;

    fn listing() -> Value {
        json!({"items": [
            {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.1.0"}},
        ]})
    }

    #[test]
    fn a_pack_name_and_a_registry_uid_go_into_the_path_of_the_tags_escaped() {
        // with no escape, a `/` in the name ends the segment, and a `?` starts the query
        let p = Palette::new(
            MemoryApi::new()
                .with(
                    "/v1/packs/a%2Fb%3Fc/registries/r%201",
                    json!({"tags": [{"tag": "1.x"}]}),
                )
                .with(
                    "/v1/packs/a%2Fb%3Fc/registries/r%201?states=disabled",
                    json!({"tags": []}),
                )
                .with(
                    "/v1/packs?filters=spec.name=a%2Fb%3Fc&limit=100&offset=100",
                    json!({"items": [1]}),
                ),
        );
        assert_eq!(
            p.pack_tags("a/b?c", "r 1", None).unwrap()["tags"][0]["tag"],
            "1.x"
        );
        assert_eq!(
            p.pack_tags("a/b?c", "r 1", Some("disabled")).unwrap()["tags"],
            json!([])
        );
        assert_eq!(
            p.pack_listing_page("a/b?c", 100).unwrap()["items"],
            json!([1])
        );
        let err = p.pack_tags("a/b?c", "r 2", None).unwrap_err();
        assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    }

    #[test]
    fn the_registry_listings_read_the_metadata_and_the_summary_of_a_kind() {
        let p = Palette::new(
            MemoryApi::new()
                .with("/v1/registries/metadata", json!({"items": [1]}))
                .with("/v1/registries/pack/summary", json!({"items": [2]}))
                .with("/v1/registries/oci/summary", json!({"items": [3]})),
        );
        assert_eq!(p.registry_metadata().unwrap()["items"], json!([1]));
        assert_eq!(
            p.registry_summary(RegistryKind::Pack).unwrap()["items"],
            json!([2])
        );
        assert_eq!(
            p.registry_summary(RegistryKind::Oci).unwrap()["items"],
            json!([3])
        );
        assert_eq!(
            RegistryKind::of_pack_type(PackType::Spectro),
            Some(RegistryKind::Pack)
        );
        assert_eq!(
            RegistryKind::of_pack_type(PackType::Helm),
            Some(RegistryKind::Helm)
        );
        assert_eq!(RegistryKind::of_pack_type(PackType::Manifest), None);
        for kind in [RegistryKind::Pack, RegistryKind::Helm, RegistryKind::Oci] {
            assert_eq!(RegistryKind::of_kind(kind.route()), Some(kind));
            assert_eq!(RegistryKind::of_pack_type(kind.pack_type()), Some(kind));
        }
        assert_eq!(RegistryKind::of_kind("spectro"), None);
    }

    #[test]
    fn resolve_finds_the_exact_name_and_version() {
        let p = Palette::new(
            MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing()),
        );
        assert_eq!(p.resolve("aws-stack", "1.1.0").unwrap(), "u2");
        let err = p.resolve("aws-stack", "9.9.9").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }));
        assert!(err.to_string().contains("'aws-stack' with version '9.9.9'"));
        // at tenant scope the listing advice is a dead end: the scope holds only tenant profiles
        assert!(
            err.to_string().contains("this is tenant scope"),
            "a miss at tenant scope asks for a project first: {err}"
        );

        // in a project, the listing is the right next command, and the name may be a pack's
        let in_project = Palette::new(
            MemoryApi::new()
                .with("/v1/dashboard/clusterprofiles/metadata", listing())
                .in_project("p1"),
        );
        let err = in_project.resolve("aws-stack", "9.9.9").unwrap_err();
        assert!(
            err.to_string().contains("`strata list --pack aws-stack`"),
            "a miss in a project points at the listing: {err}"
        );
    }

    /// A listing in a project with a profile of the tenant and a profile of the project named
    /// `aws-stack`, both at 1.1.1, and the document of each with its scope.
    fn two_scopes() -> MemoryApi {
        MemoryApi::new()
            .in_project("p1")
            .with(
                "/v1/dashboard/clusterprofiles/metadata",
                json!({"items": [
                    {"metadata": {"name": "aws-stack", "uid": "u-tenant"}, "spec": {"version": "1.1.1"}},
                    {"metadata": {"name": "aws-stack", "uid": "u-project"}, "spec": {"version": "1.1.1"}},
                    {"metadata": {"name": "aws-stack", "uid": "u-old"}, "spec": {"version": "1.0.0"}},
                ]}),
            )
            .with(
                "/v1/clusterprofiles/u-tenant",
                json!({"metadata": {"uid": "u-tenant", "annotations": {"scope": "tenant"}}}),
            )
            .with(
                "/v1/clusterprofiles/u-project",
                json!({"metadata": {"uid": "u-project", "annotations": {"scope": "project"}}}),
            )
    }

    #[test]
    fn a_name_and_version_that_two_scopes_answer_is_an_error_that_names_each_uid_with_its_scope() {
        let p = Palette::new(two_scopes());
        let err = p.resolve("aws-stack", "1.1.1").unwrap_err();
        assert_eq!(err.kind(), "profile-name-ambiguous");
        let Error::ProfileNameAmbiguous {
            name,
            version,
            profiles,
        } = &err
        else {
            panic!("{err}");
        };
        assert_eq!((name.as_str(), version.as_str()), ("aws-stack", "1.1.1"));
        assert_eq!(
            profiles
                .iter()
                .map(|p| (p.uid.as_str(), p.scope))
                .collect::<Vec<_>>(),
            [
                ("u-project", Some(ProfileScope::Project)),
                ("u-tenant", Some(ProfileScope::Tenant)),
            ],
            "by uid, each with the scope of its document"
        );
        assert!(
            err.to_string()
                .contains("u-project (project), u-tenant (tenant); give the uid"),
            "{err}"
        );
        // the version that one scope holds still resolves
        assert_eq!(p.resolve("aws-stack", "1.0.0").unwrap(), "u-old");
    }

    #[test]
    fn latest_is_ambiguous_only_when_two_scopes_hold_the_highest_version() {
        let p = Palette::new(two_scopes());
        let err = p.resolve_latest("aws-stack").unwrap_err();
        assert!(matches!(err, Error::ProfileNameAmbiguous { .. }), "{err}");
        assert!(
            err.to_string().contains("with version '1.1.1'"),
            "the message names the highest version, not 'latest': {err}"
        );

        // two scopes hold an older version, and one holds the highest: no error
        let listing = json!({"items": [
            {"metadata": {"name": "aws-stack", "uid": "u-tenant"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u-project"}, "spec": {"version": "1.0.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u-new"}, "spec": {"version": "1.1.0"}},
        ]});
        let p =
            Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
        assert_eq!(p.resolve_latest("aws-stack").unwrap(), "u-new");
    }

    #[test]
    fn the_profile_search_gives_the_uid_of_each_version_of_a_scope_over_each_page() {
        let body = json!({
            "filter": {"scope": "tenant"},
            "sort": [{"field": "profileName", "order": "asc"}],
        });
        // a first page of 50 names, each with one version, and a second page of one name
        let names = |from: usize, n: usize| -> Vec<Value> {
            (from..from + n)
                .map(|i| {
                    json!({"metadata": {"name": format!("p{i}")},
                           "specSummary": {"versions": [{"version": "1.0.0", "uid": format!("u{i}")}]}})
                })
                .collect()
        };
        let api = MemoryApi::new()
            .with_post(
                &with_limit(PROFILE_SEARCH),
                &body,
                json!({"items": names(0, 50), "listmeta": {"count": 51, "limit": 50, "offset": 0}}),
            )
            .with_post(
                &format!("{}&offset=50", with_limit(PROFILE_SEARCH)),
                &body,
                json!({"items": [{"metadata": {"name": "p50"}, "specSummary": {"versions": [
                    {"version": "1.0.0", "uid": "u50"}, {"version": "2.0.0", "uid": "u50b"}]}}],
                       "listmeta": {"count": 51, "limit": 50, "offset": 50}}),
            );
        let uids = Palette::new(&api)
            .profile_uids_in_scope(ProfileScope::Tenant)
            .unwrap();
        assert_eq!(uids.len(), 52, "{uids:?}");
        assert!(uids.contains("u0") && uids.contains("u50") && uids.contains("u50b"));
        assert_eq!(api.posted().len(), 2, "two pages, two requests");

        // a Palette with no profile search answers 404, and the caller decides
        let err = Palette::new(MemoryApi::new())
            .profile_uids_in_scope(ProfileScope::System)
            .unwrap_err();
        assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
    }

    #[test]
    fn latest_resolves_to_the_highest_version_by_natural_order() {
        let listing = json!({"items": [
            {"metadata": {"name": "aws-stack", "uid": "u1"}, "spec": {"version": "1.9.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u2"}, "spec": {"version": "1.10.0"}},
            {"metadata": {"name": "aws-stack", "uid": "u0"}, "spec": {"version": "1.2.0"}},
            {"metadata": {"name": "other", "uid": "u3"}, "spec": {"version": "9.0.0"}},
        ]});
        let p =
            Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
        assert_eq!(p.resolve_latest("aws-stack").unwrap(), "u2");
        assert_eq!(p.resolve("aws-stack", "latest").unwrap(), "u2");
        let err = p.resolve_latest("nope").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
        assert!(err.to_string().contains("'nope'"));
        assert!(err.to_string().contains("'latest'"));
    }

    #[test]
    fn an_item_with_no_version_or_no_uid_is_not_a_version_of_the_name() {
        // the one parse keeps each item of the listing; a resolve skips an item with no version
        // or no uid, so a name with no other item is profile-not-found, not an empty uid
        let listing = json!({"items": [
            {"metadata": {"name": "x"}},
            {"metadata": {"name": "y", "uid": "u1"}},
            {"metadata": {"name": "y"}, "spec": {"version": "2.0.0"}},
            {"metadata": {"name": "y", "uid": "u2"}, "spec": {"version": "1.0.0"}},
        ]});
        let p =
            Palette::new(MemoryApi::new().with("/v1/dashboard/clusterprofiles/metadata", listing));
        let err = p.resolve_latest("x").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
        assert_eq!(p.resolve_latest("y").unwrap(), "u2");
        assert_eq!(p.resolve("y", "1.0.0").unwrap(), "u2");
        let err = p.resolve("y", "2.0.0").unwrap_err();
        assert!(matches!(err, Error::ProfileNotFound { .. }), "{err}");
    }

    #[test]
    fn fetch_profile_tolerates_a_missing_variables_endpoint_only() {
        let base = MemoryApi::new()
            .with("/v1/clusterprofiles/u1", json!({"metadata": {"uid": "u1"}}))
            .with(
                "/v1/clusterprofiles/u1/packs/manifests",
                json!({"spec": {"packs": []}}),
            );
        let raw = Palette::new(base.clone())
            .fetch_profile("u1", &Progress::silent())
            .unwrap();
        assert_eq!(raw.variables, json!({"variables": []}));

        let err = Palette::new(base.failing("/v1/clusterprofiles/u1/variables", 500, "boom"))
            .fetch_profile("u1", &Progress::silent())
            .unwrap_err();
        assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
    }

    #[test]
    fn a_uid_with_a_slash_goes_into_each_route_escaped() {
        // with no escape, a `/` in a uid ends the segment, and a `?` starts the query
        assert_eq!(
            route(PROFILES, &["a/b", "packs", "x?y"]),
            "/v1/clusterprofiles/a%2Fb/packs/x%3Fy"
        );
        let uid = "a/b";
        let cluster = "/v1/spectroclusters/a%2Fb";
        let profile = "/v1/clusterprofiles/a%2Fb";
        let p = Palette::new(
            MemoryApi::new()
                .with(cluster, json!({"metadata": {"uid": uid}}))
                .with(
                    format!("{cluster}/packs/resolvedValues"),
                    json!({"profiles": []}),
                )
                .with(
                    "/v1/notifications/spectrocluster/a%2Fb",
                    json!({"items": [1], "listmeta": {"continue": ""}}),
                )
                .with(profile, json!({"metadata": {"uid": uid}}))
                .with(
                    format!("{profile}/packs/manifests"),
                    json!({"spec": {"packs": []}}),
                )
                .with(format!("{profile}/variables"), json!({"variables": []})),
        );
        let doc = p.subject_document(SubjectKind::Cluster, uid).unwrap();
        assert_eq!(text(&doc, "/metadata/uid"), uid);
        let values = p.resolved_values(SubjectKind::Cluster, uid).unwrap();
        assert_eq!(values["profiles"], json!([]));
        assert_eq!(p.cluster_notifications(uid).unwrap(), vec![json!(1)]);
        assert_eq!(
            text(&p.profile_document(uid).unwrap(), "/metadata/uid"),
            uid
        );
        let raw = p.fetch_profile(uid, &Progress::silent()).unwrap();
        assert_eq!(raw.variables, json!({"variables": []}));
    }

    #[test]
    fn one_pager_walks_a_get_listing_and_a_post_search_by_continue() {
        let notifications = "/v1/notifications/spectrocluster/c1";
        let search = with_limit(EDGE_HOST_SEARCH);
        let api = MemoryApi::new()
            .with(
                notifications,
                json!({"items": [1, 2], "listmeta": {"continue": "t1"}}),
            )
            .with(
                format!("{notifications}?continue=t1"),
                json!({"items": [3], "listmeta": {"continue": ""}}),
            )
            .with(
                format!("POST {search}"),
                json!({"items": [4], "listmeta": {"continue": "t 2"}}),
            )
            .with(
                format!("POST {search}&continue=t%202"),
                json!({"items": null, "listmeta": {"continue": "t3"}}),
            );
        let p = Palette::new(&api);
        assert_eq!(
            p.cluster_notifications("c1").unwrap(),
            vec![json!(1), json!(2), json!(3)]
        );
        // a page with no items ends the search, also with a token
        assert_eq!(p.edge_hosts().unwrap(), vec![json!(4)]);
        assert_eq!(api.posted().len(), 2);
        assert_eq!(page_after("/v1/x", ""), "/v1/x");
        assert_eq!(page_after("/v1/x", "a b"), "/v1/x?continue=a%20b");
        assert_eq!(
            page_after("/v1/x?limit=50", "t"),
            "/v1/x?limit=50&continue=t"
        );
    }

    #[test]
    fn fetch_subject_reads_each_kind_by_uid_and_then_by_name_after_a_404() {
        // the three kinds share one lookup: the uid first, then the one object of the name
        let templates =
            |uid: &str| json!({"metadata": {"uid": uid}, "spec": {"clusterProfileTemplates": []}});
        let api = MemoryApi::new()
            .with(
                CLUSTER_LISTING,
                json!([{"metadata": {"name": "lab", "uid": "c1"}}]),
            )
            .with(CLUSTER_SEARCH_SCHEMA, json!({"schema": {"properties": []}}))
            .with("/v1/spectroclusters/c1", templates("c1"))
            .with(
                "/v1/spectroclusters/c1/profiles/packs/manifests",
                json!({"profiles": []}),
            )
            .with("/v1/spectroclusters/c1/variables", json!([]))
            .with(
                CLUSTER_GROUP_LISTING,
                json!({"items": [{"name": "dev", "uid": "g1"}]}),
            )
            .with("/v1/clustergroups/g1", templates("g1"))
            .with(
                format!("POST {}", with_limit(EDGE_HOST_SEARCH)),
                json!({"items": [{"metadata": {"name": "store", "uid": "e1"}}],
                    "listmeta": {"continue": ""}}),
            )
            .with("/v1/edgehosts/e1", templates("e1"));
        let p = Palette::new(&api);
        let quiet = Progress::silent();
        for (kind, name, uid) in [
            (SubjectKind::Cluster, "lab", "c1"),
            (SubjectKind::ClusterGroup, "dev", "g1"),
            (SubjectKind::EdgeHost, "store", "e1"),
        ] {
            let by_uid = p.fetch_subject(kind, uid, &quiet).unwrap();
            assert_eq!(text(&by_uid.cluster, "/metadata/uid"), uid);
            let by_name = p.fetch_subject(kind, name, &quiet).unwrap();
            assert_eq!(text(&by_name.cluster, "/metadata/uid"), uid);
            let err = p.fetch_subject(kind, "nope", &quiet).unwrap_err();
            let expected = match kind {
                SubjectKind::Cluster => "cluster-not-found",
                SubjectKind::ClusterGroup => "cluster-group-not-found",
                SubjectKind::EdgeHost => "edge-host-not-found",
            };
            assert_eq!(err.kind(), expected, "{err}");
        }
        // another error of the document by uid returns, and no name is tried
        let lookups = api.calls(CLUSTER_LISTING);
        let broken = api.clone().failing("/v1/spectroclusters/c9", 500, "{}");
        let err = Palette::new(&broken)
            .fetch_subject(SubjectKind::Cluster, "c9", &quiet)
            .unwrap_err();
        assert!(matches!(err, Error::Api { status: 500, .. }), "{err}");
        assert_eq!(broken.calls(CLUSTER_LISTING), lookups);
    }
}
