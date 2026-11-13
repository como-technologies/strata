//! `behind`: the profile versions that the records hold, against the newer published versions of
//! each profile.
//!
//! No route of Palette says that a newer version of a profile exists. The document of a version
//! gives the published versions of its profile in `spec.versions[]`. They have the same name, in
//! the scope of the version. Palette selects them, so strata does not copy that rule. The list has
//! no order, so strata orders it with [`version::compare`], as it orders the pins. When Palette
//! gives the newer versions of a profile, strata reads that answer and drops this order.
//!
//! With the impact, strata also compares each profile version that is behind with the newest
//! version, as `compare` of the two versions does. It adds no rule of a change.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::documents::ProfileDocuments;
use super::newer::is_newer;
use super::{Bump, Check, PinnedBy};
use crate::api::PaletteApi;
use crate::error::{ErrorDocument, Result, found};
use crate::json::{arr, text};
use crate::model::scope::ProfileScope;
use crate::model::update::UpdateChecks;
use crate::reference::{name_version, version};
use crate::snapshot::Snapshot;
use crate::term::progress::Progress;
use crate::timestamp::Rfc3339;

/// One profile version that the records hold, and the newer published versions of its profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePin {
    /// The profile name, as the first record that holds the version gives it.
    pub name: String,
    /// The version that the records hold.
    pub version: String,
    /// The uid of the version.
    pub uid: String,
    /// The scope of the version, as its document gives it in `metadata.annotations.scope`. Absent
    /// when strata could not read the document, or the document does not give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ProfileScope>,
    /// Each record that holds the version: a record of the version, or of a cluster that runs it.
    pub pinned_by: Vec<PinnedBy>,
    /// The newer published versions of the profile, newest first. Palette gives the published
    /// versions of the same name in the scope of the version. A pre-release is not a newer
    /// version of a version that is not one, unless [`super::BehindOptions::pre_release`] asks
    /// for it.
    pub newer: Vec<NewerProfile>,
    /// True when `newer` is not empty.
    pub behind: bool,
    /// For a version that is behind: what a move to the newest version does, from `compare` of the
    /// two versions. Absent with `--no-impact`, or when strata could not compare them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bump: Option<Bump>,
    /// True when Palette no longer holds the version: its document answers HTTP 404. Strata then
    /// cannot read the other versions of the profile from it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gone: bool,
    /// Why strata could not check the version, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<ErrorDocument>")]
    pub error: Option<Value>,
}

/// One newer published version of a profile.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NewerProfile {
    /// The version.
    pub version: String,
    /// The uid of the version.
    pub uid: String,
    /// For the newest version: when Palette created it, `metadata.creationTimestamp` of its
    /// document. Palette gives no time of the publish. Empty for each other version, and when the
    /// document gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[schemars(with = "Rfc3339")]
    pub created: String,
}

/// One profile version in the records: a name, a version, and a uid. The order is the order of
/// the profiles in the report: by name, then by [`version::compare`], then by uid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProfileKey {
    name: String,
    version: String,
    uid: String,
}

impl Ord for ProfileKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // two texts can name one version, such as `1.0.1+2` and `1.0.1`: the text then decides,
        // so that the order of the keys is total
        self.name
            .cmp(&other.name)
            .then_with(|| version::compare(&self.version, &other.version))
            .then_with(|| self.version.cmp(&other.version))
            .then_with(|| self.uid.cmp(&other.uid))
    }
}

impl PartialOrd for ProfileKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A profile version of the records, with the project of its first record.
pub(super) struct Held {
    pub(super) profile: ProfilePin,
    /// The project of the first record that holds it. Strata reads the version there.
    pub(super) project: String,
}

/// The profile versions that the record `snap` holds.
///
/// A record of a profile version holds that version. A record of a cluster, an edge host, or a
/// cluster group holds each profile that it runs. A candidate has no uid, and a pack record and
/// an app profile hold no profile version, so they give none.
pub(super) fn held_keys(snap: &Snapshot) -> impl Iterator<Item = ProfileKey> + '_ {
    let source = &snap.source;
    let own = source
        .is_profile_version()
        .then_some((&source.name, &source.version, &source.uid));
    let applied = source
        .cluster
        .iter()
        .flat_map(|c| &c.profiles)
        .map(|p| (&p.name, &p.version, &p.uid));
    own.into_iter()
        .chain(applied)
        .filter(|(_, _, uid)| !uid.is_empty())
        .map(|(name, version, uid)| ProfileKey {
            name: name.clone(),
            version: version.clone(),
            uid: uid.clone(),
        })
}

/// Add each profile version that the record `snap` holds to `held`: see [`held_keys`]. `path` is
/// the path of the record.
pub(super) fn collect(held: &mut BTreeMap<ProfileKey, Held>, snap: &Snapshot, path: &Path) {
    let source = &snap.source;
    for key in held_keys(snap) {
        let owner = source.title_of_profile(&name_version(&key.name, &key.version));
        let entry = held.entry(key.clone()).or_insert_with(|| Held {
            profile: ProfilePin {
                name: key.name,
                version: key.version,
                uid: key.uid,
                scope: None,
                pinned_by: Vec::new(),
                newer: Vec::new(),
                behind: false,
                bump: None,
                gone: false,
                error: None,
            },
            project: source.project_uid.clone(),
        });
        PinnedBy::add(&mut entry.profile.pinned_by, path, &source.title(), owner);
    }
}

/// Give `profile` its scope and the newer published versions of its profile. With the impact, also
/// give a profile version that is behind its bump: see [`read_move`].
///
/// `documents` holds the documents of the scope of its first record. `check.pre_release` also names
/// a pre-release above a version that is not one: see [`is_newer`].
///
/// A version that Palette no longer holds is `gone`. Each other failed read is the error of
/// `profile`, with no newer version.
pub(super) fn check_profile<A: PaletteApi>(
    documents: &mut ProfileDocuments<'_, A>,
    profile: &mut ProfilePin,
    check: &Check<'_>,
) {
    check.progress.note(&format!(
        "reading the versions of profile {}",
        name_version(&profile.name, &profile.version)
    ));
    match newer_of(documents, profile, check.pre_release) {
        Ok(Some((scope, newer))) => {
            profile.scope = scope;
            profile.behind = !newer.is_empty();
            profile.newer = newer;
        }
        Ok(None) => profile.gone = true,
        Err(e) => profile.error = Some(e.to_json()),
    }
    if check.impact && profile.behind {
        read_move(documents, profile, check.generated_at, check.progress);
    }
}

/// The scope of `profile` and the newer published versions of its profile, newest first. None
/// when Palette no longer holds the version.
///
/// The document of the version gives the published versions in `spec.versions[]`, in no order.
/// Strata keeps each version above the version of `profile` by [`is_newer`]. It reads the
/// document of the newest one for the time of its create.
fn newer_of<A: PaletteApi>(
    documents: &mut ProfileDocuments<'_, A>,
    profile: &ProfilePin,
    pre_release: bool,
) -> Result<Option<(Option<ProfileScope>, Vec<NewerProfile>)>> {
    let Some(doc) = found(documents.get(&profile.uid))? else {
        return Ok(None);
    };
    let scope = ProfileScope::of_document(doc);
    let mut newer: Vec<NewerProfile> = arr(doc, "/spec/versions")
        .iter()
        .map(|v| NewerProfile {
            version: text(v, "/version"),
            uid: text(v, "/uid"),
            created: String::new(),
        })
        .filter(|n| !n.uid.is_empty() && is_newer(&n.version, &profile.version, pre_release))
        .collect();
    // newest first. Palette gives the list in no order, so the uid orders two equal versions
    newer.sort_by(|a, b| a.uid.cmp(&b.uid));
    newer.sort_by(|a, b| version::compare(&b.version, &a.version));
    if let Some(newest) = newer.first_mut() {
        newest.created = text(documents.get(&newest.uid)?, "/metadata/creationTimestamp");
    }
    Ok(Some((scope, newer)))
}

/// Give `profile` its bump: what a move to the newest version of its profile does, from `compare`
/// of the two versions, with the checks of [`UpdateChecks::between`]. `at` is the time of the
/// report, in RFC 3339. A comparison that fails is the error of `profile`.
fn read_move<A: PaletteApi>(
    documents: &mut ProfileDocuments<'_, A>,
    profile: &mut ProfilePin,
    at: &str,
    progress: &Progress,
) {
    let Some(newest) = profile.newer.first() else {
        return;
    };
    let (held, to) = (
        name_version(&profile.name, &profile.version),
        name_version(&profile.name, &newest.version),
    );
    progress.note(&format!("comparing profile {held} with {to}"));
    let compared = version_record(documents, &profile.uid, &held, at, progress).and_then(|a| {
        let b = version_record(documents, &newest.uid, &to, at, progress)?;
        let mut bump = Bump::of_profile(&newest.version, &crate::ops::compare(&a, &b, at));
        bump.update_checks = UpdateChecks::between(&a, &b);
        Ok(bump)
    });
    match compared {
        Ok(bump) => profile.bump = Some(bump),
        Err(e) => profile.error = Some(e.to_json()),
    }
}

/// The record of the profile version `uid`, which `reference` names: its document in `documents`,
/// with its pack manifests and its variables.
fn version_record<A: PaletteApi>(
    documents: &mut ProfileDocuments<'_, A>,
    uid: &str,
    reference: &str,
    at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    // the read of a draft writes a mask over its secrets, so the read takes its own copy
    let doc = documents.get(uid)?.clone();
    let raw = documents.palette.profile_raw(uid, doc, progress)?;
    Snapshot::from_raw(&raw, reference, at)
}
