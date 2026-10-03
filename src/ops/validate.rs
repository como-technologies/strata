//! `snapshot --validate`: ask Palette whether it takes the packs of a profile version, for a
//! candidate before its create, or for a version that Palette holds as an update of itself.
//! `compare --validate`: ask Palette whether it accepts the packs of a profile version on a
//! cluster.

use serde_json::{Value, json};

use super::repave::{cluster_and_target, validate_body};
use crate::api::{Palette, PaletteApi};
use crate::error::{
    Error, PACK_FORBIDDEN_CODE, PACK_REFUSAL_CODES, PROFILE_PACK_REFUSAL_CODES, Result,
};
use crate::snapshot::{AppliedProfile, Snapshot};
use crate::validation::{PaletteRefusal, PaletteValidation, ProfileValidation};

/// The refusal of the whole request in an error of strata, when it is an HTTP error of Palette
/// whose status and code `accept` takes. Palette gives such a refusal before it checks each pack,
/// so it is an answer, and not an error of strata.
fn whole_request_refusal(e: &Error, accept: impl Fn(u16, &str) -> bool) -> Option<PaletteRefusal> {
    let Error::Api { status, body, .. } = e else {
        return None;
    };
    PaletteRefusal::from_error_body(body).filter(|r| accept(*status, &r.code))
}

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
    let answer = match palette.validate_profile_packs(uid, &profile_validate_body(snap)) {
        Ok(answer) => answer,
        Err(e) => {
            return whole_request_refusal(&e, |status, code| {
                status == 500 && PROFILE_PACK_REFUSAL_CODES.contains(&code)
            })
            .map(|refusal| ProfileValidation::refused(vec![refusal]))
            .ok_or(e);
        }
    };
    Ok(ProfileValidation::refused(
        answer
            .get("packs")
            .map(PaletteRefusal::from_constraints)
            .unwrap_or_default(),
    ))
}

/// Ask Palette whether it accepts the packs of the profile side on the cluster side: `POST
/// validate/packs` with the body of `--repave`, which changes nothing. The sides follow the rules
/// of [`palette_repave`](super::palette_repave). Palette answers `200` with an error for each pack
/// it refuses. It refuses the whole request with HTTP 500 and a code of [`PACK_REFUSAL_CODES`], or
/// with HTTP 403 and [`PACK_FORBIDDEN_CODE`]: that is an answer too, and the retry does not send
/// it again. Each other error is an error of strata.
pub fn palette_validation<A: PaletteApi>(
    palette: &Palette<A>,
    a: &Snapshot,
    b: &Snapshot,
) -> Result<PaletteValidation> {
    let (cluster, target) = cluster_and_target(a, b, "--validate")?;
    let running = AppliedProfile::of_cluster(&palette.cluster_document(&cluster.uid)?);
    match palette.validate_packs(&cluster.uid, &validate_body(&running, target)) {
        Ok(answer) => Ok(PaletteValidation::from_palette(&cluster.uid, &answer)),
        Err(e) => whole_request_refusal(&e, |status, code| match status {
            500 => PACK_REFUSAL_CODES.contains(&code),
            403 => code == PACK_FORBIDDEN_CODE,
            _ => false,
        })
        .map(|refusal| PaletteValidation::refused(&cluster.uid, vec![refusal]))
        .ok_or(e),
    }
}

/// The error of `snapshot --validate` when Palette refuses the packs of the record:
/// [`Error::PaletteRefuses`] with each error. `Ok` when the record has no answer of Palette, or
/// when Palette accepts the packs.
pub fn require_acceptance(snap: &Snapshot) -> Result<()> {
    match &snap.source.palette_validation {
        Some(v) if !v.accepts => Err(Error::PaletteRefuses {
            reference: snap.source.reference.clone(),
            errors: v.errors.clone(),
        }),
        _ => Ok(()),
    }
}
