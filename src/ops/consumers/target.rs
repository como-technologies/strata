//! What a ref names for `consumers`: a profile version or an app profile, and the scope to read
//! it in.

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::ops::scope_of;
use crate::reference::Ref;
use crate::snapshot::Snapshot;
use crate::term::progress::Progress;

/// The uid of the profile that `reference` names. `hint` is the uid that the ref holds, when it
/// holds one. The other refs name the profile by name and version, which `home` resolves.
pub(super) fn uid_of_ref<A: PaletteApi>(
    reference: &Ref,
    hint: Option<String>,
    home: &Palette<A>,
    progress: &Progress,
) -> Result<String> {
    match (hint, reference) {
        (Some(uid), _) => Ok(uid),
        (None, Ref::NameVersion { name, version }) => {
            home.profile_uid(name, Some(version), progress)
        }
        (None, Ref::Draft { id, version }) => home.profile_uid(id, version.as_deref(), progress),
        (None, _) => unreachable!("every other ref carries a uid"),
    }
}

/// What a ref names for `consumers`.
pub(super) enum Target {
    /// A profile version. `scope` is the scope of its record, when the ref is a record. `uid` is
    /// its uid, when the ref holds one.
    Profile {
        scope: Option<String>,
        uid: Option<String>,
    },
    /// An app profile, in the scope of its record when the ref is a record.
    App {
        scope: Option<String>,
        id: String,
        version: Option<String>,
    },
}

/// What `reference` names, and the scope to read it in.
///
/// # Errors
///
/// [`Error::ProfileRefRequired`] for a ref that names no profile, and for a record of a cluster
/// or of a pack. [`Error::NoUid`] for a record with no uid. Each error of the read of a record.
pub(super) fn target_of(reference: &Ref) -> Result<Target> {
    let refused = || Error::ProfileRefRequired {
        reference: reference.to_string(),
    };
    match reference {
        Ref::Cluster { .. }
        | Ref::EdgeHost { .. }
        | Ref::ClusterGroup { .. }
        | Ref::ClusterSet { .. }
        | Ref::Pack { .. } => Err(refused()),
        Ref::AppProfile { id, version } => Ok(Target::App {
            scope: None,
            id: id.clone(),
            version: version.clone(),
        }),
        Ref::Snapshot(path) => {
            let recorded = Snapshot::load(path)?;
            // a record of a cluster or of a pack has no consumers. Refuse it here: the NoUid below
            // names `verify`, a command that does not run. An app profile has its app deployments
            if !(recorded.source.is_profile_version() || recorded.source.is_app()) {
                return Err(refused());
            }
            if recorded.source.uid.is_empty() {
                return Err(Error::NoUid);
            }
            let scope = scope_of(&recorded.source.project_uid).map(str::to_string);
            if recorded.source.is_app() {
                return Ok(Target::App {
                    scope,
                    id: recorded.source.uid,
                    version: None,
                });
            }
            Ok(Target::Profile {
                scope,
                uid: Some(recorded.source.uid),
            })
        }
        Ref::Uid(uid) => Ok(Target::Profile {
            scope: None,
            uid: Some(uid.clone()),
        }),
        // a draft has the consumers of its version: a cluster runs a published template
        Ref::NameVersion { .. } | Ref::Draft { .. } => Ok(Target::Profile {
            scope: None,
            uid: None,
        }),
    }
}
