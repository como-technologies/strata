//! The reads of a profile and of an app profile.
//!
//! They give the uid of a name, the documents, the variables, the manifests, and the validate
//! routes of a profile.

use crate::closed_set::ClosedSet;
use std::collections::BTreeSet;

use serde_json::Value;

use super::{
    APP_DEPLOYMENTS, APP_PROFILE_LISTING, APP_PROFILES, EDGE_HOSTS, PACKS, PROFILE_SEARCH,
    PROFILES, Palette, SEARCH_PAGE, TIER_CONTAINER_REGISTRY, TIER_INSTALL_ORDER, install_order,
    listed_versions, query_value, route, with_limit,
};
use crate::api::{PaletteApi, RawProfile, mask};
use crate::cloud::CLOUD_ALL;
use crate::error::{Error, Result, found};
use crate::json::{arr, opt_text, text};
use crate::profile_type::ProfileType;
use crate::reference::{LATEST, one_named, version};
use crate::scope::{ProfileScope, ScopedUid};
use crate::snapshot::{Template, manifest_of};
use crate::term::progress::Progress;

impl<A: PaletteApi> Palette<A> {
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

    /// The uid of the app profile `name` at `version`, or at its highest version for
    /// [`LATEST`], from `GET /v1/dashboard/appProfiles/metadata`.
    ///
    /// Palette refuses a second app profile with one name and one version with the code
    /// `AppProfileAlreadyExists`, but only in one scope. So two scopes can each hold one.
    ///
    /// # Errors
    ///
    /// [`Error::AppProfileNotFound`] when no app profile has the name and the version.
    /// [`Error::AppProfileNameAmbiguous`] when two or more have them. Each error of the request.
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
        let not_found = || {
            let mut available: Vec<String> =
                all.iter().map(|(n, v, _)| format!("{n}@{v}")).collect();
            available.sort();
            Error::AppProfileNotFound {
                name: name.to_string(),
                version: version.to_string(),
                available,
            }
        };
        let named = all.iter().filter(|(n, _, _)| n == name);
        let Some((_, picked, _)) = version::pick(named, |(_, v, _)| v.as_str(), version) else {
            return Err(not_found());
        };
        let uids = all
            .iter()
            .filter(|(n, v, _)| n == name && v == picked)
            .map(|(_, _, uid)| uid.clone())
            .collect();
        one_named(uids, String::as_str, not_found, |uids| {
            Error::AppProfileNameAmbiguous {
                name: name.to_string(),
                version: picked.clone(),
                uids,
            }
        })
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
                manifests.push(manifest_of(&d, m, Template::Published));
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
    pub(super) fn variables_or(
        &self,
        owner: &str,
        none: Value,
        progress: &Progress,
    ) -> Result<(Value, bool)> {
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
        if !crate::api::raw::has_draft(profile) {
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
    pub(super) fn profile_pack_manifests(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&route(PROFILES, &[uid, "packs", "manifests"]))
    }

    /// One manifest of a pack of the profile version `uid`,
    /// `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}`: its draft
    /// content and its published content.
    pub(super) fn profile_manifest(&self, uid: &str, pack: &str, manifest: &str) -> Result<Value> {
        self.api.get_json(&route(
            PROFILES,
            &[uid, "packs", pack, "manifests", manifest],
        ))
    }

    /// The copy of a manifest that the edge host `uid` holds,
    /// `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}`.
    pub(super) fn edge_host_manifest(&self, uid: &str, manifest: &str) -> Result<Value> {
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
    /// [`PROFILE_EXISTS_CODE`](crate::api::codes::PROFILE_EXISTS_CODE). See
    /// [`ValidateRoute::ProfileName`](crate::api::codes::ValidateRoute::ProfileName). Each other error
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
    ///   [`ValidateRoute::ProfileImport`](crate::api::codes::ValidateRoute::ProfileImport).
    /// - HTTP 409 with [`PROFILE_EXISTS_CODE`](crate::api::codes::PROFILE_EXISTS_CODE).
    /// - HTTP 404 with [`PACK_MACRO_UNDEFINED_CODE`](crate::api::codes::PACK_MACRO_UNDEFINED_CODE).
    /// - The HTTP 422 of the API schema.
    ///
    /// Each other error of the request, such as a refused connection.
    pub fn validate_profile_import(&self, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(PROFILES, &["import", "validate"]), body)
    }
}
