//! The operations over any [`PaletteApi`]: find a profile or a cluster, and read the documents a
//! snapshot is built from.

use serde_json::Value;

use super::{AuditKind, PaletteApi, RawCluster, RawProfile, ReadErrors, mask};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::LATEST;
use crate::snapshot::{APP_KIND, SubjectKind};
use crate::version;

const PROFILE_LISTING: &str = "/v1/dashboard/clusterprofiles/metadata";
const CLUSTER_LISTING: &str = "/v1/dashboard/spectroclusters/meta";
const PROJECT_LISTING: &str = "/v1/dashboard/projects/metadata";
/// The edge host search. Palette gives at most 50 on a page.
const EDGE_HOST_SEARCH: &str = "/v1/dashboard/edgehosts/search?limit=50";
const CLUSTER_GROUP_LISTING: &str = "/v1/clustergroups/hostCluster/metadata";
const APP_PROFILE_LISTING: &str = "/v1/dashboard/appProfiles/metadata";
/// The cluster search. Palette gives at most 50 on a page.
const CLUSTER_SEARCH: &str = "/v1/dashboard/spectroclusters/search?limit=50";
const CLUSTER_SEARCH_SCHEMA: &str = "/v1/dashboard/spectroclusters/search/schema";
const TAG_FILTER_LISTING: &str = "/v1/filters/metadata?filterType=tag";
/// The most entries that Palette serves on one page of the audit log. A larger `limit` answers
/// `500` with the code `InvalidLimit`. The API reference gives 50, but Palette serves 100.
const AUDIT_PAGE: u32 = 100;
/// The field of a tier that gives its place in the install of the app profile. Palette installs
/// the tiers in this order, and does not give the field for a tier whose order is 0.
const TIER_INSTALL_ORDER: &str = "installOrder";
/// The field of a container tier that gives the registry of its image.
const TIER_CONTAINER_REGISTRY: &str = "containerRegistryUid";

/// The install order of a tier, when Palette gives one.
fn install_order(tier: &Value) -> Option<&Value> {
    tier.pointer(&format!("/spec/{TIER_INSTALL_ORDER}"))
}

/// `(name, uid)` of each entry of a listing: an array, or an object with `items`.
fn named_entries(listing: &Value) -> Vec<(String, String)> {
    let field = |v: &Value, k: &str| {
        v.pointer(&format!("/metadata/{k}"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    listing
        .get("items")
        .unwrap_or(listing)
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| (field(e, "name"), field(e, "uid")))
        .collect()
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

/// The names of `entries`, sorted.
fn sorted_names(entries: &[(String, String)]) -> Vec<String> {
    let mut names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    names
}

/// A kind of registry, as the routes of Palette name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// the type `spectro`. None for a type with no registry of its own, such as `manifest`.
    pub(crate) fn of_pack_type(pack_type: &str) -> Option<Self> {
        match pack_type {
            "spectro" => Some(Self::Pack),
            "helm" => Some(Self::Helm),
            "oci" => Some(Self::Oci),
            _ => None,
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
    pub fn resolve(&self, name: &str, version: &str) -> Result<String> {
        if version == LATEST {
            return self.resolve_latest(name);
        }
        let listing = self.profile_listing()?;
        listing
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|it| {
                it.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
                    && it.pointer("/spec/version").and_then(Value::as_str) == Some(version)
            })
            .and_then(|it| it.pointer("/metadata/uid").and_then(Value::as_str))
            .map(str::to_string)
            .ok_or_else(|| Error::ProfileNotFound {
                name: name.to_string(),
                version: version.to_string(),
                tenant_scope: self.project_uid().is_none(),
            })
    }

    /// The uid of a profile that a ref names: `id` is a uid when there is no `version`, and else
    /// a name that [`Palette::resolve`] finds at `version`.
    pub(crate) fn profile_uid(&self, id: &str, version: Option<&str>) -> Result<String> {
        match version {
            Some(version) => self.resolve(id, version),
            None => Ok(id.to_string()),
        }
    }

    /// Find the uid of the highest version of `name` in the current scope, by
    /// [`version::compare`]. A name with no versions is [`Error::ProfileNotFound`].
    pub fn resolve_latest(&self, name: &str) -> Result<String> {
        let listing = self.profile_listing()?;
        listing
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|it| it.pointer("/metadata/name").and_then(Value::as_str) == Some(name))
            .filter_map(|it| {
                let version = it.pointer("/spec/version")?.as_str()?;
                let uid = it.pointer("/metadata/uid")?.as_str()?;
                Some((version, uid))
            })
            .max_by(|(x, _), (y, _)| version::compare(x, y))
            .map(|(_, uid)| uid.to_string())
            .ok_or_else(|| Error::ProfileNotFound {
                name: name.to_string(),
                version: LATEST.to_string(),
                tenant_scope: self.project_uid().is_none(),
            })
    }

    /// The uid of the app profile `name` at `version`, or at its highest version for
    /// [`LATEST`], from `GET /v1/dashboard/appProfiles/metadata`.
    pub fn resolve_app_profile(&self, name: &str, version: &str) -> Result<String> {
        let listing = self.api.get_json(APP_PROFILE_LISTING)?;
        let all: Vec<(String, String, String)> = listing
            .get("appProfiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|it| {
                let text = |p: &str| {
                    it.pointer(p)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                (
                    text("/metadata/name"),
                    text("/spec/version"),
                    text("/metadata/uid"),
                )
            })
            .collect();
        let named = all.iter().filter(|(n, _, _)| n == name);
        let found = if version == LATEST {
            named.max_by(|(_, x, _), (_, y, _)| version::compare(x, y))
        } else {
            named.into_iter().find(|(_, v, _)| v == version)
        };
        match found {
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
        let at = query_value(uid);
        let doc = self.api.get_json(&format!("/v1/appProfiles/{at}"))?;
        let tiers = self.api.get_json(&format!("/v1/appProfiles/{at}/tiers"))?;
        let text = |v: &Value, p: &str| {
            v.pointer(p)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        // Palette lists the tiers in the display order of the app profile, not in install order.
        let mut ordered: Vec<&Value> = tiers
            .pointer("/spec/appTiers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .collect();
        // A stable sort: tiers of one order keep the order of the listing. Palette gives no order
        // for a tier at 0. An order that is not an integer comes first, and the record keeps it.
        ordered.sort_by_key(|tier| install_order(tier).map_or(Some(0), Value::as_i64));
        let (mut packs, mut mpacks) = (Vec::new(), Vec::new());
        for tier in ordered {
            let name = text(tier, "/metadata/name");
            let mut params: serde_json::Map<String, Value> = tier
                .pointer("/spec/properties")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
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
            let tier_uid = query_value(&text(tier, "/metadata/uid"));
            let mut manifests = Vec::new();
            for m in tier
                .pointer("/spec/manifests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let d = self.api.get_json(&format!(
                    "/v1/appProfiles/{at}/tiers/{tier_uid}/manifests/{}",
                    query_value(&text(m, "/uid"))
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
                    "published": {"type": APP_KIND, "cloudType": "all", "packs": packs}},
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

    /// The uid of the one cluster named `name` in the scope, from [`Palette::cluster_listing`].
    pub fn cluster_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.cluster_listing()?;
        let clusters = named_entries(&listing);
        let mut uids: Vec<String> = clusters
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, uid)| uid.clone())
            .collect();
        uids.sort();
        match uids.len() {
            1 => Ok(uids.remove(0)),
            0 => Err(Error::ClusterNotFound {
                cluster: name.to_string(),
                available: sorted_names(&clusters),
            }),
            _ => Err(Error::ClusterNameAmbiguous {
                name: name.to_string(),
                uids,
            }),
        }
    }

    /// The project listing, `GET /v1/dashboard/projects/metadata`: `items[]` with each project's
    /// name and uid. At tenant scope it holds every project the key can see; in a project, only
    /// that project.
    pub fn project_listing(&self) -> Result<Value> {
        self.api.get_json(PROJECT_LISTING)
    }

    /// Reads a variables route, and gives `none` when the route is missing.
    ///
    /// An older Palette answers 404 or 405 for the route. Each other error returns. Palette also
    /// answers 404 when it does not have the object. So after a 404, strata reads `owner`, the
    /// document of the object, again. An error of that read returns: the object is gone, and "no
    /// variables" is wrong.
    ///
    /// The second value is true when the route is missing and `none` is in its place. A caller
    /// must record that flag. "No variables" and "strata could not read the variables" give the
    /// same root, and only the flag shows the difference.
    fn variables_or(
        &self,
        path: &str,
        owner: &str,
        none: Value,
        progress: &Progress,
    ) -> Result<(Value, bool)> {
        match self.api.get_json(path) {
            Ok(v) => Ok((v, false)),
            Err(Error::Api { status, .. }) if status == 404 || status == 405 => {
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
        let mut profile = self.api.get_json(&format!("/v1/clusterprofiles/{uid}"))?;
        let mut manifests = self
            .api
            .get_json(&format!("/v1/clusterprofiles/{uid}/packs/manifests"))?;
        self.mask_draft(&mut profile, &mut manifests, progress)?;
        let manifest_entities = self.manifest_entities(uid, &profile, progress)?;
        let (variables, variables_unavailable) = self.variables_or(
            &format!("/v1/clusterprofiles/{uid}/variables"),
            &format!("/v1/clusterprofiles/{uid}"),
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
            match self
                .api
                .get_json(&format!("/v1/packs/{}", query_value(&uid)))
            {
                Ok(pack) => {
                    let fields = mask::password_fields(&pack, &uid);
                    if !fields.is_empty() {
                        secrets.insert(uid, fields);
                    }
                }
                Err(Error::Api { status: 404, .. }) => {}
                Err(e) => return Err(e),
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
        let text = |v: &Value, p: &str| {
            v.pointer(p)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let mut out = serde_json::Map::new();
        for template in ["/spec/published/packs", "/spec/draft/packs"] {
            for pack in profile
                .pointer(template)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = text(pack, "/name");
                for m in pack
                    .get("manifests")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let manifest = text(m, "/uid");
                    if name.is_empty() || manifest.is_empty() || out.contains_key(&manifest) {
                        continue;
                    }
                    progress.note(&format!(
                        "reading manifest {manifest} of pack {name}: the profile has a draft"
                    ));
                    let doc = self.api.get_json(&format!(
                        "/v1/clusterprofiles/{uid}/packs/{}/manifests/{}",
                        query_value(&name),
                        query_value(&manifest)
                    ))?;
                    out.insert(manifest, doc);
                }
            }
        }
        Ok(Some(Value::Object(out)))
    }

    /// The profile document alone, `GET /v1/clusterprofiles/{uid}`: metadata, versions, and the
    /// clusters that use it in `status`.
    pub fn profile_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&format!("/v1/clusterprofiles/{uid}"))
    }

    /// The cluster document alone, `GET /v1/spectroclusters/{uid}`: its name, its project in the
    /// `projectUid` annotation, and its state in `status.state`.
    pub fn cluster_document(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&format!("/v1/spectroclusters/{uid}"))
    }

    /// The app profile document alone, `GET /v1/appProfiles/{uid}`: metadata, version, and the
    /// app deployments that use it in `status.inUseApps`.
    pub fn app_profile_document(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/appProfiles/{}", query_value(uid)))
    }

    /// One app deployment, `GET /v1/appDeployments/{uid}`: its name, its project in the
    /// `projectUid` annotation, the app profile version its copy names in
    /// `spec.profile.metadata.version`, and its state in `status.state`.
    pub fn app_deployment_document(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/appDeployments/{}", query_value(uid)))
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
        let mut query = String::new();
        if let Some(kind) = kind {
            query.push_str(&format!("resourceKind={}&", kind.as_str()));
        }
        query.push_str(&format!("resourceUid={}", query_value(uid)));
        if !since.is_empty() {
            query.push_str(&format!("&startTime={}", query_value(since)));
        }
        query.push_str(&format!("&limit={AUDIT_PAGE}"));
        self.paged(&format!("/v1/audits?{query}"))
    }

    /// The cluster summary, `GET /v1/dashboard/spectroclusters/{uid}`: what the dashboard shows
    /// for one cluster. Its `status.health` is the health of the cluster, which the cluster
    /// document does not carry.
    pub fn fetch_cluster_summary(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/dashboard/spectroclusters/{uid}"))
    }

    /// Fetch the three documents for one cluster: the cluster with its applied profile templates,
    /// the pack manifests of each applied profile, and each applied profile's variables. A missing
    /// variables endpoint is treated as "no variables"; other errors propagate. `cluster` is a uid,
    /// or, when no cluster has that uid, the name of one cluster in the scope; the documents are
    /// always read by uid.
    pub fn fetch_cluster(&self, cluster: &str, progress: &Progress) -> Result<RawCluster> {
        progress.note(&format!("fetching cluster {cluster}"));
        let (uid, cluster) = match self.cluster_document(cluster) {
            Ok(doc) => (cluster.to_string(), doc),
            Err(Error::Api { status: 404, .. }) => {
                let uid = self.cluster_uid_named(cluster)?;
                progress.note(&format!("cluster '{cluster}' is {uid}"));
                let doc = self.cluster_document(&uid)?;
                (uid, doc)
            }
            Err(e) => return Err(e),
        };
        self.cluster_raw(&uid, cluster, progress)
    }

    /// The document of the cluster, edge host, or cluster group with the uid `uid`, and no
    /// other: `GET /v1/spectroclusters/{uid}`, `GET /v1/edgehosts/{uid}`, or `GET
    /// /v1/clustergroups/{uid}`. A 404 is an error, and no name is tried. Palette keeps the
    /// document of a deleted cluster, with the state `Deleted`.
    pub fn subject_document(
        &self,
        kind: SubjectKind,
        uid: &str,
        progress: &Progress,
    ) -> Result<Value> {
        progress.note(&format!("fetching {} {uid}", kind.noun()));
        let base = match kind {
            SubjectKind::Cluster => "/v1/spectroclusters",
            SubjectKind::EdgeHost => "/v1/edgehosts",
            SubjectKind::ClusterGroup => "/v1/clustergroups",
        };
        self.api.get_json(&format!("{base}/{}", query_value(uid)))
    }

    /// The raw documents of a cluster, an edge host, or a cluster group, from the document that
    /// [`Palette::subject_document`] gives: the rest of what [`Palette::fetch_cluster`],
    /// [`Palette::fetch_edge_host`], or [`Palette::fetch_cluster_group`] read.
    pub fn subject_raw(
        &self,
        kind: SubjectKind,
        doc: Value,
        progress: &Progress,
    ) -> Result<RawCluster> {
        match kind {
            SubjectKind::Cluster => {
                let uid = doc
                    .pointer("/metadata/uid")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.cluster_raw(&uid, doc, progress)
            }
            SubjectKind::EdgeHost => self.holder_raw(doc, Holder::EdgeHost, progress),
            SubjectKind::ClusterGroup => self.holder_raw(doc, Holder::ClusterGroup, progress),
        }
    }

    /// The rest of the raw documents of the cluster `uid`, whose document is `cluster`.
    fn cluster_raw(&self, uid: &str, cluster: Value, progress: &Progress) -> Result<RawCluster> {
        let manifests = self.api.get_json(&format!(
            "/v1/spectroclusters/{uid}/profiles/packs/manifests"
        ))?;
        let (variables, variables_unavailable) = self.variables_or(
            &format!("/v1/spectroclusters/{uid}/variables"),
            &format!("/v1/spectroclusters/{uid}"),
            serde_json::json!([]),
            progress,
        )?;
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
        let uid = raw
            .cluster
            .pointer("/metadata/uid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let updates = self.api.get_json(&format!(
            "/v1/spectroclusters/{}/profileUpdates",
            query_value(&uid)
        ))?;
        let profiles: Vec<Value> = updates
            .get("profiles")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
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
            let puid = t.get("uid").and_then(Value::as_str).unwrap_or_default();
            let d = self.api.get_json(&format!(
                "/v1/clusterprofiles/{}/packs/manifests",
                query_value(puid)
            ))?;
            manifests.push(serde_json::json!({"metadata": d["metadata"], "spec": d["spec"]}));
        }
        Ok(serde_json::json!({ "profiles": manifests }))
    }

    /// Every edge host the scope can list, `POST /v1/dashboard/edgehosts/search`, every page.
    /// Each item names the profiles bound to the host in `spec.clusterProfileTemplates`, as a
    /// summary: uid, name, and version.
    pub fn edge_hosts(&self) -> Result<Vec<Value>> {
        self.searched(
            EDGE_HOST_SEARCH,
            &serde_json::json!({"filter": {}, "sort": []}),
        )
    }

    /// The clusters that a search filter selects, `POST /v1/dashboard/spectroclusters/search`,
    /// every page. `filter` is the `filter` of the body: groups of conditions on the properties
    /// that `cluster_search_schema` names. At tenant scope it holds the clusters of each project
    /// the key can read, each with its labels and its project. It holds no virtual cluster.
    pub fn cluster_search(&self, filter: &Value) -> Result<Vec<Value>> {
        self.searched(
            CLUSTER_SEARCH,
            &serde_json::json!({"filter": filter, "sort": [{"field": "clusterName", "order": "asc"}]}),
        )
    }

    /// The properties that the cluster search takes, `GET
    /// /v1/dashboard/spectroclusters/search/schema`: `schema.properties[]` with `name` and `type`.
    pub fn cluster_search_schema(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_SEARCH_SCHEMA)
    }

    /// The saved tag filters, `GET /v1/filters/metadata?filterType=tag`: the uid and the name of
    /// each. A tag filter belongs to the tenant.
    pub fn tag_filters(&self) -> Result<Value> {
        self.api.get_json(TAG_FILTER_LISTING)
    }

    /// One saved tag filter, `GET /v1/filters/tag/{uid}`: `spec.filterGroup` holds its
    /// conjunction and its conditions.
    pub fn tag_filter(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/filters/tag/{}", query_value(uid)))
    }

    /// Every item of a search of the dashboard, a POST of `body`, every page: Palette gives at
    /// most `limit` on a page, and `listmeta.continue` names the next one.
    fn searched(&self, search: &str, body: &Value) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        let mut next = String::new();
        loop {
            let path = if next.is_empty() {
                search.to_string()
            } else {
                format!("{search}&continue={}", query_value(&next))
            };
            let page = self.api.post_json(&path, body)?;
            let got = page
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let done = got.is_empty();
            items.extend(got);
            next = page
                .pointer("/listmeta/continue")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if next.is_empty() || done {
                return Ok(items);
            }
        }
    }

    /// The raw documents of an edge host, in the shape of a cluster: `GET /v1/edgehosts/{uid}`
    /// gives the bound profiles in `spec.clusterProfileTemplates`. The host holds its own copy of
    /// each manifest, which `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}` reads; the
    /// variables and the rest of each pack come from its profile. An edge host has no
    /// notifications. `host` is a
    /// uid, or, when no edge host has that uid, the name of one edge host in the scope.
    pub fn fetch_edge_host(&self, host: &str, progress: &Progress) -> Result<RawCluster> {
        progress.note(&format!("fetching edge host {host}"));
        let get = |uid: &str| {
            self.api
                .get_json(&format!("/v1/edgehosts/{}", query_value(uid)))
        };
        let doc = match get(host) {
            Ok(doc) => doc,
            Err(Error::Api { status: 404, .. }) => {
                let uid = self.edge_host_uid_named(host)?;
                progress.note(&format!("edge host '{host}' is {uid}"));
                get(&uid)?
            }
            Err(e) => return Err(e),
        };
        self.holder_raw(doc, Holder::EdgeHost, progress)
    }

    /// The raw documents of a cluster group, in the shape of a cluster: `GET
    /// /v1/clustergroups/{uid}` gives the profiles that the group pins for its virtual clusters
    /// in `spec.clusterProfileTemplates`. The group holds its own copy of each manifest, which
    /// `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifestUid}` reads by its uid; the
    /// variables and the rest of each pack come from its profile. `group` is a uid, or, when no cluster group has that uid, the name of one
    /// cluster group in the scope.
    pub fn fetch_cluster_group(&self, group: &str, progress: &Progress) -> Result<RawCluster> {
        progress.note(&format!("fetching cluster group {group}"));
        let get = |uid: &str| {
            self.api
                .get_json(&format!("/v1/clustergroups/{}", query_value(uid)))
        };
        let doc = match get(group) {
            Ok(doc) => doc,
            Err(Error::Api { status: 404, .. }) => {
                let uid = self.cluster_group_uid_named(group)?;
                progress.note(&format!("cluster group '{group}' is {uid}"));
                get(&uid)?
            }
            Err(e) => return Err(e),
        };
        self.holder_raw(doc, Holder::ClusterGroup, progress)
    }

    /// The virtual clusters of a cluster group, `POST
    /// /v1/dashboard/clustergroups/{uid}/virtualClusters`, every page: the clusters that run the
    /// profiles that the group pins.
    pub fn cluster_group_members(&self, uid: &str) -> Result<Vec<Value>> {
        self.searched(
            &format!(
                "/v1/dashboard/clustergroups/{}/virtualClusters?limit=50",
                query_value(uid)
            ),
            &serde_json::json!({"filter": {}, "sort": []}),
        )
    }

    /// The name and the uid of each cluster group in the scope, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    pub fn cluster_groups(&self) -> Result<Vec<(String, String)>> {
        let listing = self.api.get_json(CLUSTER_GROUP_LISTING)?;
        Ok(listing
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|g| {
                        let text = |k: &str| {
                            g.get(k)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        (text("name"), text("uid"))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// The document of the cluster group `uid`, `GET /v1/clustergroups/{uid}`: the profiles that
    /// the group pins for its virtual clusters are in `spec.clusterProfileTemplates`.
    pub fn cluster_group_document(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/clustergroups/{}", query_value(uid)))
    }

    /// The uid of the one cluster group in the scope that has the name `name`, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    fn cluster_group_uid_named(&self, name: &str) -> Result<String> {
        let groups = self.cluster_groups()?;
        let mut uids: Vec<String> = groups
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, uid)| uid.clone())
            .collect();
        uids.sort();
        match uids.len() {
            1 => Ok(uids.remove(0)),
            0 => Err(Error::ClusterGroupNotFound {
                cluster_group: name.to_string(),
                available: sorted_names(&groups),
            }),
            _ => Err(Error::ClusterGroupNameAmbiguous {
                name: name.to_string(),
                uids,
            }),
        }
    }

    /// The raw documents of an object that holds profiles in `spec.clusterProfileTemplates` and
    /// is not a cluster: the manifests and the variables of each profile come from the profile.
    /// Such an object gives no values to the variables, so a record holds the definitions of each
    /// profile, as a profile record does. It has no notifications.
    fn holder_raw(&self, doc: Value, holder: Holder, progress: &Progress) -> Result<RawCluster> {
        let templates: Vec<Value> = doc
            .pointer("/spec/clusterProfileTemplates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut variables = Vec::with_capacity(templates.len());
        let mut variables_unavailable = false;
        for t in &templates {
            let puid = t.get("uid").and_then(Value::as_str).unwrap_or_default();
            let profile = format!("/v1/clusterprofiles/{}", query_value(puid));
            let (v, unavailable) = self.variables_or(
                &format!("{profile}/variables"),
                &profile,
                serde_json::json!({ "variables": [] }),
                progress,
            )?;
            variables_unavailable |= unavailable;
            variables.push(serde_json::json!({"profileUid": puid, "variables": v["variables"]}));
        }
        let mut manifests = self.profile_manifests(&templates)?;
        self.own_manifests(&doc, &templates, holder, &mut manifests)?;
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

    /// Put the copies that a holder keeps of its manifests in place of the manifests of its
    /// profiles. Palette gives an edge host and a cluster group a copy of each manifest under a
    /// uid of its own, and the template of the holder names each copy by that uid, with no
    /// content. An edit of the profile does not reach the copy.
    fn own_manifests(
        &self,
        doc: &Value,
        templates: &[Value],
        holder: Holder,
        manifests: &mut Value,
    ) -> Result<()> {
        let holder_uid = doc
            .pointer("/metadata/uid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        for (i, t) in templates.iter().enumerate() {
            let puid = t.get("uid").and_then(Value::as_str).unwrap_or_default();
            for pack in t
                .get("packs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = pack.get("name").and_then(Value::as_str).unwrap_or_default();
                let refs = pack
                    .get("manifests")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                // a copy with no uid cannot be read: the manifests of the profile stay
                if refs.is_empty()
                    || refs.iter().any(|m| {
                        m.get("uid")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .is_empty()
                    })
                {
                    continue;
                }
                let mut own = Vec::with_capacity(refs.len());
                for m in &refs {
                    let muid = m.get("uid").and_then(Value::as_str).unwrap_or_default();
                    let path = match holder {
                        Holder::EdgeHost => format!(
                            "/v1/edgehosts/{}/pack/manifests/{}",
                            query_value(holder_uid),
                            query_value(muid)
                        ),
                        Holder::ClusterGroup => format!(
                            "/v1/clusterprofiles/{}/packs/{}/manifests/{}",
                            query_value(puid),
                            query_value(name),
                            query_value(muid)
                        ),
                    };
                    let got = self.api.get_json(&path)?;
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
                if let Some(p) = packs.iter_mut().find(|p| {
                    p.pointer("/spec/name").and_then(Value::as_str) == Some(name)
                        || p.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
                }) {
                    p["spec"]["manifests"] = Value::Array(own);
                }
            }
        }
        Ok(())
    }

    /// The uid of the one edge host in the scope that has the name `name`.
    fn edge_host_uid_named(&self, name: &str) -> Result<String> {
        let hosts: Vec<(String, String)> = self
            .edge_hosts()?
            .iter()
            .map(|h| {
                let text = |p: &str| {
                    h.pointer(p)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                (text("/metadata/name"), text("/metadata/uid"))
            })
            .collect();
        let mut uids: Vec<String> = hosts
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, uid)| uid.clone())
            .collect();
        uids.sort();
        match uids.len() {
            1 => Ok(uids.remove(0)),
            0 => Err(Error::EdgeHostNotFound {
                edge_host: name.to_string(),
                available: sorted_names(&hosts),
            }),
            _ => Err(Error::EdgeHostNameAmbiguous {
                name: name.to_string(),
                uids,
            }),
        }
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
        let uid = cluster
            .pointer("/spec/cloudConfigRef/uid")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if kind.is_empty() || uid.is_empty() {
            return Err(Error::Usage {
                message: "the cluster document names no cloud config".to_string(),
            });
        }
        self.api.get_json(&format!(
            "/v1/cloudconfigs/{}/{}",
            query_value(kind),
            query_value(uid)
        ))
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
        let base = match kind {
            SubjectKind::Cluster => "/v1/spectroclusters",
            SubjectKind::ClusterGroup => "/v1/clustergroups",
            SubjectKind::EdgeHost => {
                return Err(Error::Usage {
                    message: "strata reads no macro values for an edge host".to_string(),
                });
            }
        };
        self.api
            .get_json(&format!("{base}/{}/packs/resolvedValues", query_value(uid)))
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
        self.api.get_json(&format!(
            "/v1/clusterprofiles/{}/packs/resolvedValues",
            query_value(uid)
        ))
    }

    /// Palette's answer to whether a change of the profiles of a cluster replaces its nodes:
    /// `POST /v1/spectroclusters/{uid}/validate/repave`. The request changes nothing.
    pub fn validate_repave(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api.post_json(
            &format!("/v1/spectroclusters/{}/validate/repave", query_value(uid)),
            body,
        )
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
            &format!(
                "/v1/spectroclusters/clusterTemplates/{}/validate/repave",
                query_value(uid)
            ),
            body,
        )
    }

    /// Palette's answer to whether it accepts the packs of a change of the profiles of a cluster:
    /// `POST /v1/spectroclusters/{uid}/validate/packs`, with the body of `validate/repave`. The
    /// request changes nothing.
    pub fn validate_packs(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api.post_json(
            &format!("/v1/spectroclusters/{}/validate/packs", query_value(uid)),
            body,
        )
    }

    /// Palette's answer to whether it takes the packs of a profile: `POST
    /// /v1/clusterprofiles/validate/packs` for a profile that Palette does not hold, or `POST
    /// /v1/clusterprofiles/{uid}/validate/packs` for an update of the version `uid`. The body is
    /// a `v1ClusterProfileTemplateDraft`. The request changes nothing.
    pub fn validate_profile_packs(&self, uid: Option<&str>, body: &Value) -> Result<Value> {
        let path = match uid {
            Some(uid) => format!("/v1/clusterprofiles/{}/validate/packs", query_value(uid)),
            None => "/v1/clusterprofiles/validate/packs".to_string(),
        };
        self.api.post_json(&path, body)
    }

    /// Palette's answer to whether it takes `name` and `version` for a new profile version: `GET
    /// /v1/clusterprofiles/validate/name`. Palette answers `204` with no body when it takes them.
    /// The request changes nothing.
    pub fn validate_profile_name(&self, name: &str, version: &str) -> Result<()> {
        self.api
            .get_json(&format!(
                "/v1/clusterprofiles/validate/name?name={}&version={}",
                query_value(name),
                query_value(version)
            ))
            .map(drop)
    }

    /// Palette's answer to whether it takes a profile as an import: `POST
    /// /v1/clusterprofiles/import/validate`. The body is a `v1ClusterProfileImportEntity`, the
    /// shape of a profile export. The request changes nothing.
    pub fn validate_profile_import(&self, body: &Value) -> Result<Value> {
        self.api
            .post_json("/v1/clusterprofiles/import/validate", body)
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
        let path = format!(
            "/v1/spectroclusters/{}/config/{}",
            query_value(uid),
            part.route()
        );
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
        self.api.get_json(&format!(
            "/v1/spectroclusters/{}/repave/status",
            query_value(uid)
        ))
    }

    /// The notifications of one cluster, `GET /v1/notifications/spectrocluster/{uid}`, every page,
    /// newest first. Palette gives `items: null` when there are none, and at most 50 on a page;
    /// `listmeta.continue` names the next page, and is empty on the last one.
    pub fn cluster_notifications(&self, uid: &str) -> Result<Vec<Value>> {
        self.paged(&format!(
            "/v1/notifications/spectrocluster/{}",
            query_value(uid)
        ))
    }

    /// One page of the pack listing of `name`,
    /// `GET /v1/packs?filters=spec.name={name}&limit=100&offset={offset}`: each version of the
    /// pack in each registry, 100 on a page, with `listmeta.count` for the whole listing.
    pub(crate) fn pack_listing_page(&self, name: &str, offset: usize) -> Result<Value> {
        self.api.get_json(&format!(
            "/v1/packs?filters=spec.name={}&limit=100&offset={offset}",
            query_value(name)
        ))
    }

    /// The tags of the pack `name` in the registry `registry_uid`,
    /// `GET /v1/packs/{name}/registries/{uid}`: each tag with the version that it names, the
    /// values of the pack, and its layer. Palette answers 404 when the registry serves no
    /// version of the pack.
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
        let path = format!(
            "/v1/packs/{}/registries/{}",
            query_value(name),
            query_value(registry_uid)
        );
        match states {
            Some(states) => self.api.get_json(&format!("{path}?states={states}")),
            None => self.api.get_json(&path),
        }
    }

    /// The registries that the key can see, `GET /v1/registries/metadata`: the `uid`, the
    /// `name`, the `kind`, and `isPrivate` of each, as `items`.
    pub(crate) fn registry_metadata(&self) -> Result<Value> {
        self.api.get_json("/v1/registries/metadata")
    }

    /// The summary listing of the registries of one `kind`, `GET /v1/registries/{kind}/summary`:
    /// each registry as `items`, with its last sync in `status.sync`, and with the provider type
    /// of an OCI registry in `spec.providerType`.
    pub(crate) fn registry_summary(&self, kind: RegistryKind) -> Result<Value> {
        self.api
            .get_json(&format!("/v1/registries/{}/summary", kind.route()))
    }

    /// Every item of a paged listing at `path`, which may carry a query already: each page's
    /// `items`, in order. `listmeta.continue` names the next page, and is empty on the last one;
    /// a page with no items ends the listing too, and `items: null` is such a page.
    fn paged(&self, path: &str) -> Result<Vec<Value>> {
        let join = if path.contains('?') { '&' } else { '?' };
        let mut items = Vec::new();
        let mut next = String::new();
        loop {
            let page = if next.is_empty() {
                self.api.get_json(path)?
            } else {
                self.api
                    .get_json(&format!("{path}{join}continue={}", query_value(&next)))?
            };
            let got = page
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let done = got.is_empty();
            items.extend(got);
            next = page
                .pointer("/listmeta/continue")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if next.is_empty() || done {
                return Ok(items);
            }
        }
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
            Ok((None, Some(format!("HTTP {status}: {}", body.trim()))))
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

/// The object that holds its own copy of each manifest of its profiles, and the route that reads
/// a copy by its uid.
#[derive(Clone, Copy)]
enum Holder {
    EdgeHost,
    ClusterGroup,
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
        // a name with `/` or `?` would end the segment, or start a query, with no escape
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
            RegistryKind::of_pack_type("spectro"),
            Some(RegistryKind::Pack)
        );
        assert_eq!(RegistryKind::of_pack_type("helm"), Some(RegistryKind::Helm));
        assert_eq!(RegistryKind::of_pack_type("manifest"), None);
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
        // at tenant scope the listing advice is a dead end: the scope holds no profiles at all
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
}
