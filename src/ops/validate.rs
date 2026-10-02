//! `snapshot --validate`: ask Palette whether it takes the packs of a profile version, for a
//! candidate before its create, or for a version that Palette holds as an update of itself.

use serde_json::{Value, json};

use crate::api::{Palette, PaletteApi};
use crate::diff::PaletteRefusal;
use crate::error::{Error, PROFILE_PACK_REFUSAL_CODES, Result, error_code};
use crate::snapshot::{ProfileValidation, Snapshot};

/// The body of the profile validate routes, a `v1ClusterProfileTemplateDraft`: the `cloudType`,
/// the `type`, and each pack of the record in profile order as a `v1PackManifestEntity`.
pub fn profile_validate_body(snap: &Snapshot) -> Value {
    let packs: Vec<Value> = snap
        .packs
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "tag": p.tag,
                "uid": p.pack_uid,
                "registryUid": p.registry_uid,
                "type": p.kind,
                "layer": p.layer,
                "values": p.values,
                "manifests": p.manifests.iter()
                    .map(|m| json!({"name": m.name, "content": m.content}))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "cloudType": snap.source.cloud_type,
        "type": snap.source.kind,
        "packs": packs,
    })
}

/// Ask Palette whether it takes the packs of a profile record. A record with no uid, of a
/// candidate, goes to `POST /v1/clusterprofiles/validate/packs`, the check of a create. A record
/// of a version that Palette holds goes to `POST /v1/clusterprofiles/{uid}/validate/packs`, the
/// check of an update of that version, which also checks the packs against the clusters that run
/// it. Neither request changes anything. A record of a cluster or of a pack is a usage error.
///
/// Palette answers `200` with an error for each pack that it refuses. It refuses the whole
/// request with HTTP 500 and a code of [`PROFILE_PACK_REFUSAL_CODES`]: that is an answer too, and
/// the retry does not send it again. Each other error is an error of strata.
pub fn profile_validation<A: PaletteApi>(
    palette: &Palette<A>,
    snap: &Snapshot,
) -> Result<ProfileValidation> {
    if snap.source.cluster.is_some() || snap.source.pack.is_some() {
        return Err(Error::Usage {
            message: "--validate takes a profile version or a candidate: Palette checks the \
                      packs of a profile, and not of a cluster or of one pack"
                .to_string(),
        });
    }
    let uid = Some(snap.source.uid.as_str()).filter(|u| !u.is_empty());
    match palette.validate_profile_packs(uid, &profile_validate_body(snap)) {
        Ok(answer) => Ok(ProfileValidation::refused(
            answer
                .get("packs")
                .map(PaletteRefusal::from_constraints)
                .unwrap_or_default(),
        )),
        Err(Error::Api {
            status: 500, body, ..
        }) if error_code(&body)
            .is_some_and(|c| PROFILE_PACK_REFUSAL_CODES.contains(&c.as_str())) =>
        {
            let error: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let text = |key: &str| {
                error
                    .get(key)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Ok(ProfileValidation::refused(vec![PaletteRefusal {
                pack: String::new(),
                code: text("code"),
                message: text("message"),
            }]))
        }
        Err(e) => Err(e),
    }
}
