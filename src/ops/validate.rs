//! `snapshot --validate`: ask Palette whether it takes the packs of a profile version, for a
//! candidate before its create, or for a version that Palette holds as an update of itself. For a
//! candidate, also ask whether it takes the name, the version, and the names of the packs and
//! manifests. `compare --validate`: ask Palette whether it accepts the packs of a profile version on a
//! cluster. `behind` asks the same routes about the bump of a pin: see the `bump` module.

use serde_json::{Value, json};

use super::repave::{cluster_and_target, validate_body};
use crate::api::codes::ValidateRoute;
use crate::api::{Palette, PaletteApi, RawCluster};
use crate::error::{Error, Result};
use crate::json::{arr, first_nonempty, str_at, value_at};
use crate::registry::{Registries, registry_entry};
use crate::snapshot::{AppliedProfile, Pack, Snapshot, SubjectKind};
use crate::validation::{PaletteRefusal, PaletteValidation, ProfileValidation};

/// The refusal of `--validate` for a ref or a record that is not one version of a cluster
/// profile.
pub(crate) const VALIDATE_TAKES: &str = "--validate takes a profile version or \
                                         --from-candidate: Palette checks the packs of a profile, \
                                         and not of a cluster, an edge host, a cluster group, an \
                                         app profile, or one pack";

/// The refusal of the whole request in an error of strata: an HTTP error of Palette whose status
/// and code are a refusal of `route`. Palette gives such a refusal before it checks each pack, so
/// it is an answer, and not an error of strata.
fn whole_request_refusal(e: &Error, route: ValidateRoute) -> Option<PaletteRefusal> {
    let Error::Api { status, body, .. } = e else {
        return None;
    };
    route.refusal(*status, body)
}

/// The refusal in `answer`, or None when Palette takes the request. An error that is not a
/// refusal of `route` is an error of strata.
fn refusal_in<T>(answer: Result<T>, route: ValidateRoute) -> Result<Option<PaletteRefusal>> {
    match answer {
        Ok(_) => Ok(None),
        Err(e) => whole_request_refusal(&e, route).map(Some).ok_or(e),
    }
}

/// The manifests of the pack `p` in a body of a validate route: the name and the content of each.
fn manifests_body(p: &Pack) -> Vec<Value> {
    p.manifests
        .iter()
        .map(|m| json!({"name": m.name, "content": m.content}))
        .collect()
}

/// The body of `POST /v1/clusterprofiles/import/validate`, a `v1ClusterProfileImportEntity`:
/// the name, the version, the variables, the type, the cloud, and each pack of the record in
/// profile order. Palette finds each pack by its name, its `version`, and its registry, so the
/// body holds no pack uid. `registries` are the entries of `GET /v1/registries/metadata`: they
/// give the name, the kind, and `isPrivate` of the registry of each pack. A manifest pack has no
/// registry.
pub fn profile_import_body(snap: &Snapshot, registries: &[Value]) -> Value {
    let packs: Vec<Value> = snap
        .packs
        .iter()
        .map(|p| {
            let mut pack = json!({
                "name": p.name,
                "type": p.kind,
                "layer": p.layer,
                "version": p.version,
                "tag": p.tag,
                "values": p.values,
                "manifests": manifests_body(p),
            });
            if !p.registry_uid.is_empty() {
                let mut metadata = json!({"uid": p.registry_uid});
                if let Some(entry) = registry_entry(registries, "/uid", &p.registry_uid) {
                    for field in ["name", "kind", "isPrivate"] {
                        if let Some(v) = entry.get(field) {
                            metadata[field] = v.clone();
                        }
                    }
                }
                if let Some(provider) = &p.registry_provider_type {
                    metadata["providerType"] = Value::from(provider.as_str());
                }
                pack["registry"] = json!({"metadata": metadata});
            }
            pack
        })
        .collect();
    json!({
        "metadata": {"name": snap.source.name},
        "spec": {
            "version": snap.source.version,
            "variables": snap.variables,
            "template": {
                "type": snap.source.kind,
                "cloudType": snap.source.cloud_type,
                "packs": packs,
            },
        },
    })
}

/// One pack in the body of the profile validate routes, a `v1PackManifestEntity`. It has the name,
/// the layer, and the manifests of `p`, and the tag, the uids, the type, and the values of
/// `content`. `content` is `p` for a pack as the record holds it.
fn pack_entry(p: &Pack, content: &Pack) -> Value {
    json!({
        "name": p.name,
        "tag": content.tag,
        "uid": content.pack_uid,
        "registryUid": content.registry_uid,
        "type": content.kind,
        "layer": p.layer,
        "values": content.values,
        "manifests": manifests_body(p),
    })
}

/// The body of the profile validate routes, a `v1ClusterProfileTemplateDraft`: the `cloudType`,
/// the `type`, and each pack of the record in profile order as a `v1PackManifestEntity`.
pub fn profile_validate_body(snap: &Snapshot) -> Value {
    let (cloud_type, kind) = (&snap.source.cloud_type, &snap.source.kind);
    profile_body(
        cloud_type,
        kind,
        snap.packs.iter().map(|p| pack_entry(p, p)),
    )
}

/// [`profile_validate_body`] for the move of the packs that `is_moved` accepts to the pack `to`.
/// A moved pack takes the version, the tag, the uids, the type, and the values of `to`, which are
/// the defaults of that version. It keeps its layer and its manifests.
pub(super) fn moved_profile_body(
    snap: &Snapshot,
    is_moved: impl Fn(&Pack) -> bool,
    to: &Pack,
) -> Value {
    let (cloud_type, kind) = (&snap.source.cloud_type, &snap.source.kind);
    let entries = snap
        .packs
        .iter()
        .map(|p| pack_entry(p, if is_moved(p) { to } else { p }));
    profile_body(cloud_type, kind, entries)
}

/// The uid and the body of the profile validate routes for each profile version that the cluster
/// document `doc` runs and whose template holds a pack that `is_moved` accepts. The move is to the
/// pack `to`. The profiles come in the canonical order of [`AppliedProfile::of_cluster`].
///
/// The packs are those of the template, as the cluster runs them. A moved pack takes the tag, the
/// uids, the type, and the values of `to`, as in [`moved_profile_body`]. The body holds no
/// manifest: the document of a cluster lists the manifests of a pack by name, with no content.
pub(super) fn moved_template_bodies(
    doc: &Value,
    is_moved: impl Fn(&Value) -> bool,
    to: &Pack,
) -> Vec<(String, Value)> {
    AppliedProfile::of_cluster(doc)
        .into_iter()
        .filter_map(|applied| {
            let template = AppliedProfile::template_of(doc, &applied.uid)?;
            let packs = arr(template, "/packs");
            if !packs.iter().any(&is_moved) {
                return None;
            }
            let cloud_type =
                first_nonempty(&[str_at(template, "/cloudType"), RawCluster::cloud_type(doc)]);
            let entries = packs.iter().map(|raw| {
                let held = Pack::from_api(raw, &[]);
                pack_entry(&held, if is_moved(raw) { to } else { &held })
            });
            Some((
                applied.uid,
                profile_body(&cloud_type, str_at(template, "/type"), entries),
            ))
        })
        .collect()
}

/// The body of the profile validate routes: the cloud type, the type, and `entries`, the packs in
/// profile order.
fn profile_body(cloud_type: &str, kind: &str, entries: impl Iterator<Item = Value>) -> Value {
    json!({
        "cloudType": cloud_type,
        "type": kind,
        "packs": entries.collect::<Vec<Value>>(),
    })
}

/// Palette's answer to `POST validate/packs` of a profile: the errors that it gives. `uid` is the
/// version for the route of an update, or `None` for a profile that Palette does not hold. A refusal
/// of the whole request is an error of the answer too, with the codes of
/// [`ValidateRoute::ProfilePacks`].
///
/// # Errors
///
/// Each error of the request that is not such a refusal.
pub(super) fn profile_packs_answer<A: PaletteApi>(
    palette: &Palette<A>,
    uid: Option<&str>,
    body: &Value,
) -> Result<Vec<PaletteRefusal>> {
    match palette.validate_profile_packs(uid, body) {
        Ok(answer) => Ok(PaletteRefusal::from_constraints(value_at(
            &answer, "/packs",
        ))),
        Err(e) => whole_request_refusal(&e, ValidateRoute::ProfilePacks)
            .map(|refusal| vec![refusal])
            .ok_or(e),
    }
}

/// Palette's answer to `POST validate/packs` of the cluster `uid`, for the body `body`. A refusal
/// of the whole request is an answer too, with the codes of [`ValidateRoute::ClusterPacks`].
///
/// # Errors
///
/// Each error of the request that is not such a refusal.
pub(super) fn cluster_packs_answer<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    body: &Value,
) -> Result<PaletteValidation> {
    match palette.validate_packs(uid, body) {
        Ok(answer) => Ok(PaletteValidation::from_palette(uid, &answer)),
        Err(e) => whole_request_refusal(&e, ValidateRoute::ClusterPacks)
            .map(|refusal| PaletteValidation::refused(uid, vec![refusal]))
            .ok_or(e),
    }
}

/// Ask Palette whether it takes the name, the version, and the packs of a profile record.
///
/// A record with no uid, of a candidate, goes to the three routes that check a create. `GET
/// /v1/clusterprofiles/validate/name` checks the name and the version. `POST
/// /v1/clusterprofiles/import/validate` checks the names of the packs and the manifests, and a
/// repeat of each, and a disabled pack. `POST /v1/clusterprofiles/validate/packs` checks the packs. A record of a
/// version that Palette holds goes to `POST /v1/clusterprofiles/{uid}/validate/packs`, the check
/// of an update of that version. That check also reads the clusters that run the version. The
/// answer holds the refusals of each route. No request changes anything.
///
/// Palette answers `200` with an error for each pack that it refuses. It refuses the whole request
/// with the HTTP 422 of the API schema, or with an HTTP error and a code of the route. The routes
/// are [`ValidateRoute::ProfileName`], [`ValidateRoute::ProfileImport`], and
/// [`ValidateRoute::ProfilePacks`]. Such a refusal is an answer too, and the retry does not send
/// it again.
///
/// # Errors
///
/// [`Error::Usage`] for a record of a cluster or of a pack, before any request. Each error of a
/// request that is not a refusal. That is an answer of Palette with another status or code, or
/// a request that did not complete.
pub fn profile_validation<A: PaletteApi>(
    palette: &Palette<A>,
    snap: &Snapshot,
) -> Result<ProfileValidation> {
    profile_validation_in(&mut Registries::new(palette), snap)
}

/// [`profile_validation`] with the registry listings of the command, so that a candidate that
/// [`resolve`](crate::candidate::resolve) read sends no second read of them.
pub(crate) fn profile_validation_in<A: PaletteApi>(
    registries: &mut Registries<'_, A>,
    snap: &Snapshot,
) -> Result<ProfileValidation> {
    let palette = registries.palette();
    if !snap.source.is_profile_version() {
        return Err(Error::Usage {
            message: VALIDATE_TAKES.to_string(),
        });
    }
    let uid = Some(snap.source.uid.as_str()).filter(|u| !u.is_empty());
    let mut errors = Vec::new();
    if uid.is_none() {
        let name = palette.validate_profile_name(&snap.source.name, &snap.source.version);
        errors.extend(refusal_in(name, ValidateRoute::ProfileName)?);
        let body = profile_import_body(snap, registries.metadata()?);
        errors.extend(refusal_in(
            palette.validate_profile_import(&body),
            ValidateRoute::ProfileImport,
        )?);
    }
    errors.extend(profile_packs_answer(
        palette,
        uid,
        &profile_validate_body(snap),
    )?);
    Ok(ProfileValidation::refused(errors))
}

/// Ask Palette whether it accepts the packs of the profile side on the cluster side: `POST
/// validate/packs` with the body of `--repave`, which changes nothing. The sides follow the rules
/// of [`palette_repave`](super::palette_repave). Palette answers `200` with an error for each pack
/// it refuses. It refuses the whole request with an HTTP error and a code of
/// [`ValidateRoute::ClusterPacks`]. That refusal is an answer too, and the retry does not send it
/// again. Each other error is an error of strata.
///
/// For a profile version that the cluster runs, the route of the cluster checks no sent tag: it
/// keeps the copy that the cluster holds. So the question goes to the route of that version, with
/// its packs, and Palette checks it against each cluster that runs it.
pub fn palette_validation<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
) -> Result<PaletteValidation> {
    let (cluster, target) = cluster_and_target(a, b, "--validate")?;
    let running =
        AppliedProfile::of_cluster(&palette.subject_document(SubjectKind::Cluster, &cluster.uid)?);
    let uid = &target.source.uid;
    if running.iter().any(|p| &p.uid == uid) {
        let errors = profile_packs_answer(palette, Some(uid), &profile_validate_body(target))?;
        return Ok(PaletteValidation::refused(&cluster.uid, errors));
    }
    cluster_packs_answer(palette, &cluster.uid, &validate_body(&running, target))
}

/// The error of `snapshot --validate` when Palette refuses the record:
/// [`Error::PaletteRefuses`] with each error. `Ok` when the record has no answer of Palette, or
/// when Palette accepts it.
pub fn require_acceptance(snap: &Snapshot) -> Result<()> {
    match &snap.source.palette_validation {
        Some(v) if !v.accepts() => Err(Error::PaletteRefuses {
            reference: snap.source.reference.clone(),
            errors: v.errors().to_vec(),
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_in_a_cluster_asks_the_route_of_each_profile_whose_template_holds_the_pack() {
        // `tag` and `values` are not in the template of the first pack of p1
        let doc = json!({"spec": {"cloudType": "aws", "clusterProfileTemplates": [
            {"uid": "p1", "name": "a", "type": "add-on", "cloudType": "all", "packs": [
                {"name": "csi", "layer": "csi", "type": "spectro"},
                {"name": "cni", "layer": "cni", "tag": "1.0", "type": "spectro",
                 "packUid": "u1", "registryUid": "r1", "values": "x: 1\n"},
            ]},
            {"uid": "p2", "name": "b", "type": "add-on", "packs": [{"name": "other", "tag": "2.0"}]},
            {"uid": "p3", "name": "c", "type": "infra", "packs": [
                {"name": "cni", "layer": "cni", "tag": "1.0", "type": "spectro"},
            ]},
        ]}});
        let to = Pack::from_api(
            &json!({"name": "cni", "tag": "2.0", "type": "spectro", "packUid": "u2",
                    "registryUid": "r1", "values": "x: 2\n"}),
            &[],
        );
        let asks = moved_template_bodies(&doc, |c| str_at(c, "/name") == "cni", &to);

        // the profiles in canonical order: the infra profile first. p2 holds no such pack
        let uids: Vec<&str> = asks.iter().map(|(uid, _)| uid.as_str()).collect();
        assert_eq!(uids, ["p3", "p1"]);
        let (_, body) = &asks[1];
        assert_eq!(body["cloudType"], "all");
        assert_eq!(body["type"], "add-on");
        let packs = body["packs"].as_array().unwrap();
        assert_eq!(packs.len(), 2);
        assert_eq!(packs[0]["name"], "csi");
        assert_eq!(
            packs[0]["tag"], "",
            "a field that the template lacks is empty text"
        );
        assert_eq!(
            packs[1],
            json!({"name": "cni", "tag": "2.0", "uid": "u2", "registryUid": "r1",
                   "type": "spectro", "layer": "cni", "values": "x: 2\n", "manifests": []}),
            "the moved pack takes the tag, the uids, the type, and the values of the target"
        );
        // a template with no cloud type takes the cloud type of the cluster
        assert_eq!(asks[0].1["cloudType"], "aws");
        assert!(moved_template_bodies(&doc, |c| str_at(c, "/name") == "absent", &to).is_empty());
    }
}
