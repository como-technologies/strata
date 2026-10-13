//! The operations the CLI exposes, without argument parsing, printing, or exit codes.
//!
//! This module takes a snapshot and compares two. Each other command has a module of its own:
//! `verify`, `same_release`, `consumers`, `list`, and `packs`.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use crate::api::{Palette, PaletteApi, RawCluster, RawProfile};
use crate::candidate::LoadedCandidate;
use crate::diff::{self, Diff};
use crate::error::{Error, Result};
use crate::json::{first_nonempty, str_at, text};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{GroupMember, Snapshot, SubjectKind};

mod behind;
mod consumer_list;
mod consumers;
mod fleet;
mod list;
mod packs;
mod repave;
mod same_release;
mod validate;
mod verify;

pub use behind::*;
pub use consumer_list::*;
pub use consumers::*;
pub use fleet::*;
pub use list::*;
pub use packs::*;
pub use repave::*;
pub use same_release::*;
pub use validate::*;
pub use verify::*;

/// What `snapshot` records beyond the content of a ref, and what it asks Palette about it.
///
/// Each option fits some kinds of ref. [`snapshot_for`] refuses the others with `Usage`, before
/// any request. The default records the content alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotOptions {
    /// `--macros`: record the digest of the value that Palette gives each macro that the content
    /// uses. It takes a profile version, a cluster, or a cluster group.
    pub macros: bool,
    /// `--shape`: for a cluster, record the shape tier, the machine pools, and the cloud part,
    /// with one more request.
    pub shape: bool,
    /// `--raw-dir`: also save the raw answers of Palette in this directory. It takes a profile
    /// version or a cluster: a record read back from the directory does not say pending, draft,
    /// edge host, cluster group, app profile, or pack.
    pub raw_dir: Option<PathBuf>,
    /// `--validate`: ask Palette whether it takes the profile version, as [`profile_validation`]
    /// does. `source.paletteValidation` of the record holds the answer, and the caller decides
    /// what a refusal does.
    pub validate: bool,
}

impl SnapshotOptions {
    /// `Usage` for the first option that does not fit `reference`. Each message names what the
    /// option takes.
    fn check(&self, reference: &Ref) -> Result<()> {
        let usage = |message: String| Err(Error::Usage { message });
        if self.macros
            && !matches!(
                reference,
                Ref::Cluster { .. }
                    | Ref::ClusterGroup { .. }
                    | Ref::Uid(_)
                    | Ref::NameVersion { .. }
            )
        {
            return usage(
                "--macros takes <name>@<version>, a profile uid, cluster:<cluster>, or \
                 clustergroup:<group>"
                    .to_string(),
            );
        }
        if self.validate && matches!(reference, Ref::Draft { .. }) {
            return usage(format!(
                "--validate takes a profile version or --from-candidate, not a draft \
                 ({reference}): give the candidate of the draft, from `strata candidate \
                 {reference}`, to --from-candidate"
            ));
        }
        if self.validate && !reference.is_profile_version() {
            return usage(
                "--validate takes a profile version or --from-candidate: Palette checks the \
                 packs of a profile, and not of a cluster, a cluster group, an app profile, or \
                 one pack"
                    .to_string(),
            );
        }
        if self.shape && !matches!(reference, Ref::Cluster { .. }) {
            return usage(
                "--shape needs a cluster ref: a profile has no machine pools".to_string(),
            );
        }
        if self.raw_dir.is_some()
            && !matches!(
                reference,
                Ref::Uid(_) | Ref::NameVersion { .. } | Ref::Cluster { pending: false, .. }
            )
        {
            return usage(
                "--raw-dir takes a profile or a cluster: a record read back from the directory \
                 does not say pending, draft, edge host, cluster group, or app profile"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// Turn a ref into a snapshot. A snapshot file is loaded; anything else is fetched through the
/// [`Palette`] that `connect` returns, with `options`, so comparing two files never needs
/// credentials. The record names the ref as it displays: the text as typed, without an empty
/// part after a separator.
///
/// # Errors
///
/// `Usage` for an option that does not fit the ref, and for a set of clusters, before any
/// request. Then the error of a request, such as a 404 for a uid that Palette does not have.
pub fn snapshot_for<A, F>(
    reference: &Ref,
    options: &SnapshotOptions,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    options.check(reference)?;
    match reference {
        Ref::Snapshot(path) => Snapshot::load(path),
        Ref::ClusterSet { .. } => Err(Error::Usage {
            message: format!(
                "{reference} names a set of clusters, and this command reads one: \
                 `strata snapshot {reference} --dir DIR` records each cluster, and \
                 `strata list clusters` lists them"
            ),
        }),
        _ => {
            let palette = connect()?;
            Ok(fetched(reference, options, &palette, fetched_at, progress)?
                .with_project(palette.project_uid()))
        }
    }
}

/// The snapshot of `reference` through `palette`, for each kind of ref that a request answers.
fn fetched<A: PaletteApi>(
    reference: &Ref,
    options: &SnapshotOptions,
    palette: &Palette<A>,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let text = reference.to_string();
    match reference {
        Ref::NameVersion { name, version } => {
            let uid = palette.resolve(name, version)?;
            profile_snapshot(palette, &uid, options, &text, fetched_at, progress)
        }
        Ref::Uid(uid) => profile_snapshot(palette, uid, options, &text, fetched_at, progress),
        Ref::Draft { id, version } => {
            draft_snapshot(palette, id, version.as_deref(), &text, fetched_at, progress)
        }
        Ref::Pack {
            registry,
            name,
            version,
        } => crate::candidate::pack_snapshot(
            palette,
            registry.as_deref(),
            name,
            version,
            &text,
            fetched_at,
            progress,
        ),
        Ref::Cluster {
            uid,
            profile,
            pending,
        } => {
            let mut raw = palette.fetch_subject(SubjectKind::Cluster, uid, progress)?;
            if *pending {
                raw = palette.with_pending(raw)?;
            }
            let snap = cluster_snapshot(
                raw,
                palette,
                uid,
                profile.as_deref(),
                options,
                &text,
                fetched_at,
            )?;
            Ok(if *pending { snap.mark_pending() } else { snap })
        }
        Ref::AppProfile { id, version } => {
            app_profile_snapshot(palette, id, version.as_deref(), &text, fetched_at, progress)
        }
        Ref::ClusterGroup { uid } => {
            cluster_group_snapshot(palette, uid, options.macros, &text, fetched_at, progress)
        }
        Ref::EdgeHost { uid, profile } => {
            let raw = palette.fetch_subject(SubjectKind::EdgeHost, uid, progress)?;
            Ok(
                Snapshot::from_cluster(&raw, profile.as_deref(), &text, fetched_at)?
                    .with_kind(SubjectKind::EdgeHost),
            )
        }
        Ref::Snapshot(_) | Ref::ClusterSet { .. } => {
            unreachable!("snapshot_for answers a file and a set before it connects")
        }
    }
}

/// A record of the published template of the profile `uid`, with what `options` add: the macro
/// digests, the raw answers saved, and the answer of Palette to `--validate`.
fn profile_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    options: &SnapshotOptions,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let mut raw = palette.fetch_profile(uid, progress)?;
    if options.macros {
        raw.resolved_values = Some(palette.profile_resolved_values(uid)?);
    }
    if let Some(dir) = &options.raw_dir {
        raw.save_dir(dir, fetched_at)?;
    }
    let mut snap = Snapshot::from_raw(&raw, reference, fetched_at)?;
    if options.validate {
        snap.source.palette_validation = Some(profile_validation(palette, &snap)?);
    }
    Ok(snap)
}

/// A record of what the cluster of `raw` runs, or of its profile `profile`, with what `options`
/// add: the macro digests, the shape, and the raw answers saved. `uid` is the cluster as the ref
/// names it.
fn cluster_snapshot<A: PaletteApi>(
    mut raw: RawCluster,
    palette: &Palette<A>,
    uid: &str,
    profile: Option<&str>,
    options: &SnapshotOptions,
    reference: &str,
    fetched_at: &str,
) -> Result<Snapshot> {
    if options.macros {
        // the ref can name the cluster by name, and the document gives its uid
        let uid = first_nonempty(&[str_at(&raw.cluster, "/metadata/uid"), uid]);
        raw.resolved_values = Some(palette.resolved_values(SubjectKind::Cluster, &uid)?);
    }
    if options.shape {
        raw.cloud_config = Some(palette.cloud_config(&raw.cluster)?);
    }
    if let Some(dir) = &options.raw_dir {
        raw.save_dir(dir, fetched_at)?;
    }
    Snapshot::from_cluster(&raw, profile, reference, fetched_at)
}

/// A record of an app profile, by uid, or by name at `version`. Its consumers are app
/// deployments, and not clusters, so the record counts none.
pub fn app_profile_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    id: &str,
    version: Option<&str>,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let uid = palette.app_profile_uid(id, version)?;
    let mut snap = Snapshot::from_raw(
        &palette.fetch_app_profile(&uid, progress)?,
        reference,
        fetched_at,
    )?;
    snap.source.consumers = None;
    Ok(snap)
}

/// A record of a cluster group: the profiles that it pins, and its virtual clusters. With
/// `macros`, also the digest of the value Palette gives each macro that the pins use, for the
/// group.
///
/// # Errors
///
/// An error of a request, such as a 404 for a group that Palette does not have.
pub fn cluster_group_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    group: &str,
    macros: bool,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let mut raw = palette.fetch_subject(SubjectKind::ClusterGroup, group, progress)?;
    let uid = text(&raw.cluster, "/metadata/uid");
    if macros {
        raw.resolved_values = Some(palette.resolved_values(SubjectKind::ClusterGroup, &uid)?);
    }
    let members = palette
        .cluster_group_members(&uid)?
        .iter()
        .map(|m| GroupMember {
            uid: text(m, "/metadata/uid"),
            name: text(m, "/metadata/name"),
            project_uid: text(m, "/metadata/annotations/projectUid"),
        })
        .collect();
    let mut snap = Snapshot::from_cluster(&raw, None, reference, fetched_at)?
        .with_kind(SubjectKind::ClusterGroup);
    if let Some(c) = snap.source.cluster.as_mut() {
        c.members = members;
    }
    Ok(snap)
}

/// The two sides of a comparison. When one side is a cluster group, and the other is a cluster ref
/// that selects no profile, the cluster side holds only the profiles that the group pins: the
/// question is whether the member runs the pin, and not what else it runs. So the other side is
/// taken first.
pub fn snapshot_pair<A, F>(
    a: &Ref,
    b: &Ref,
    mut connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<(Snapshot, Snapshot)>
where
    A: PaletteApi,
    F: FnMut() -> Result<Palette<A>>,
{
    let open = |r: &Ref| matches!(r, Ref::Cluster { profile: None, .. });
    let content = SnapshotOptions::default();
    if open(a) && !open(b) {
        let sb = snapshot_for(b, &content, &mut connect, fetched_at, progress)?;
        let sa = snapshot_for(
            &narrowed(a, &sb),
            &content,
            &mut connect,
            fetched_at,
            progress,
        )?;
        Ok((sa, sb))
    } else {
        let sa = snapshot_for(a, &content, &mut connect, fetched_at, progress)?;
        let sb = snapshot_for(
            &narrowed(b, &sa),
            &content,
            &mut connect,
            fetched_at,
            progress,
        )?;
        Ok((sa, sb))
    }
}

/// `cluster:<uid>` narrowed to the profiles that `other` pins, when `other` is a cluster group.
fn narrowed(r: &Ref, other: &Snapshot) -> Ref {
    match (r, other.source.cluster.as_ref()) {
        (
            Ref::Cluster {
                uid,
                profile: None,
                pending,
            },
            Some(c),
        ) if c.kind == SubjectKind::ClusterGroup && !c.profiles.is_empty() => Ref::Cluster {
            uid: uid.clone(),
            profile: Some(
                c.profiles
                    .iter()
                    .map(|p| p.uid.as_str())
                    .collect::<Vec<_>>()
                    .join("+"),
            ),
            pending: *pending,
        },
        _ => r.clone(),
    }
}

/// Build a snapshot of the draft of a profile version, for `<ref>@draft`. `id` is a uid when
/// there is no `version`, and else a name at `version`. The snapshot has no project: the caller
/// sets it.
pub fn draft_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    id: &str,
    version: Option<&str>,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let uid = palette.profile_uid(id, version)?;
    let raw = palette.fetch_profile(&uid, progress)?;
    Snapshot::from_raw_draft(&raw, reference, fetched_at)
}

/// Build a snapshot of a candidate file: read and check it (no network), then resolve its packs
/// through the [`Palette`] that `connect` returns. The snapshot has `source.template` `candidate`
/// and no uid. With `validate`, strata then asks Palette whether it takes the packs of the
/// candidate, as [`profile_validation`] does. `source.paletteValidation` of the record holds the
/// answer: the caller decides what a refusal does.
pub fn snapshot_from_candidate<A, F>(
    path: &Path,
    validate: bool,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    let loaded = crate::candidate::load(path)?;
    let palette = connect()?;
    let mut snap = candidate_snapshot(&loaded, &palette, fetched_at, progress)?;
    if validate {
        snap.source.palette_validation = Some(profile_validation(&palette, &snap)?);
    }
    Ok(snap)
}

/// Build a snapshot of a candidate that is checked and in memory: resolve its packs through the
/// [`Palette`] that `connect` returns. [`snapshot_from_candidate`] reads a file and then does
/// this; a caller with a [`Candidate`](crate::candidate::Candidate) value gives it to
/// [`candidate::check`](crate::candidate::check) and starts here.
pub fn snapshot_from_loaded<A, F>(
    loaded: &LoadedCandidate,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    candidate_snapshot(loaded, &connect()?, fetched_at, progress)
}

fn candidate_snapshot<A: PaletteApi>(
    loaded: &LoadedCandidate,
    palette: &Palette<A>,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let raw = crate::candidate::resolve(loaded, palette, progress)?;
    let mut snap = Snapshot::from_raw(&raw, &loaded.path.display().to_string(), fetched_at)?;
    crate::candidate::warn_preset_options(&snap, progress);
    snap.source.template = crate::snapshot::Template::Candidate;
    // Palette does not hold a candidate, so no request asked which clusters use it
    snap.source.consumers = None;
    Ok(snap.with_project(palette.project_uid()))
}

/// Build a snapshot from raw API responses saved on disk, in the project they were fetched in. A
/// directory with `cluster.json` is a cluster's; a `reference` of the form
/// `cluster:<uid>/<profile>` then selects the profile.
pub fn snapshot_from_dir(dir: &Path, reference: &str, fetched_at: &str) -> Result<Snapshot> {
    if dir.join(RawCluster::MARKER).is_file() {
        let profile = match Ref::parse_with(reference, |_| false) {
            Ref::Cluster { profile, .. } => profile,
            _ => None,
        };
        let raw = RawCluster::load_dir(dir)?;
        // the responses were read when they were saved; this run read a directory
        let at = raw
            .fetched_at
            .clone()
            .unwrap_or_else(|| fetched_at.to_string());
        return Ok(
            Snapshot::from_cluster(&raw, profile.as_deref(), reference, &at)?
                .with_project(raw.project_uid.as_deref()),
        );
    }
    let raw = RawProfile::load_dir(dir)?;
    let fetched_at = &raw
        .fetched_at
        .clone()
        .unwrap_or_else(|| fetched_at.to_string());
    Ok(Snapshot::from_raw(&raw, reference, fetched_at)?.with_project(raw.project_uid.as_deref()))
}

/// Write `text` to `path`, creating the directory it names when that directory does not exist.
/// Every file strata writes goes through here, so `-o releases/1.2.0/dev.json` needs no `mkdir`.
pub fn write_file(path: &Path, text: &str) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(path, text).map_err(io)
}

/// Read the text of the file at `path`. Every text file strata reads goes through here.
///
/// # Errors
///
/// `Io`, with `path`, when the file cannot be read.
pub fn read_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Compare two snapshots.
pub fn compare(a: &Snapshot, b: &Snapshot, generated_at: &str) -> Diff {
    diff::compute(a, b, generated_at, None)
}

/// The scope that the project uid `project` names: `None`, the scope of the connection, when it
/// is empty. A record of the tenant scope and a cluster with no project both give an empty uid.
pub fn scope_of(project: &str) -> Option<&str> {
    Some(project).filter(|p| !p.is_empty())
}

/// The client that reads a cluster of project `project`: the one already connected for that
/// project, a new one, or `home` when the cluster names no project of its own. A cluster with no
/// project is in the scope the operation already works in, so `home` reads it; `consumers` and
/// `list --applied` answered that differently until they shared this.
fn client_for<'a, A, F>(
    clients: &'a mut HashMap<String, Palette<A>>,
    home: &'a Palette<A>,
    project: &str,
    connect: &mut F,
) -> Result<&'a Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    if project.is_empty() || Some(project) == home.project_uid() {
        return Ok(home);
    }
    match clients.entry(project.to_string()) {
        Entry::Occupied(e) => Ok(e.into_mut()),
        Entry::Vacant(v) => connect(Some(project)).map(|p| &*v.insert(p)),
    }
}
