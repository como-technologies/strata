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
use crate::reference::Ref;
use crate::registry::Registries;
use crate::snapshot::{GroupMember, Snapshot, SubjectKind};
use crate::term::progress::Progress;

mod behind;
mod bump;
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

// the command line writes each record through this public path
pub use crate::file::write_file;

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
    /// `--raw-dir`: also save the raw answers of Palette in this directory.
    ///
    /// It takes a profile version or a cluster. A record that strata reads from the directory
    /// does not say pending, draft, edge host, cluster group, app profile, or pack.
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
            return usage(validate::VALIDATE_TAKES.to_string());
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
                "--raw-dir takes a profile or a cluster. A record that strata reads from the \
                 directory does not say pending, draft, edge host, cluster group, app profile, \
                 or pack"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// Turn a ref into a snapshot.
///
/// Strata loads a snapshot file, and reads each other ref through the [`Palette`] that `connect`
/// returns, with `options`. So a comparison of two files needs no credentials. The record names
/// the ref as it displays: the text as typed, with no empty part after a separator.
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
    let text = reference.to_string();
    // a ref that a request answers connects first, and the record names the project of the client
    let live = |read: &dyn Fn(&Palette<A>) -> Result<Snapshot>| {
        let palette = connect()?;
        Ok(read(&palette)?.with_project(palette.project_uid()))
    };
    match reference {
        Ref::Snapshot(path) => Snapshot::load(path),
        Ref::ClusterSet { .. } => Err(Error::Usage {
            message: format!(
                "{reference} names a set of clusters, and this command reads one: \
                 `strata snapshot {reference} --dir DIR` records each cluster, and \
                 `strata list clusters` lists them"
            ),
        }),
        Ref::NameVersion { name, version } => live(&|palette| {
            let uid = palette.resolve(name, version)?;
            profile_snapshot(palette, &uid, options, &text, fetched_at, progress)
        }),
        Ref::Uid(uid) => {
            live(&|palette| profile_snapshot(palette, uid, options, &text, fetched_at, progress))
        }
        Ref::Draft { id, version } => live(&|palette| {
            draft_snapshot(palette, id, version.as_deref(), &text, fetched_at, progress)
        }),
        Ref::Pack {
            registry,
            name,
            version,
        } => live(&|palette| {
            crate::candidate::pack_snapshot(
                palette,
                registry.as_deref(),
                name,
                version,
                &text,
                fetched_at,
                progress,
            )
        }),
        Ref::Cluster {
            uid,
            profile,
            pending,
        } => live(&|palette| {
            let raw = palette.fetch_subject(SubjectKind::Cluster, uid, progress)?;
            let reads = HolderReads {
                profile: profile.as_deref(),
                pending: *pending,
                macros: options.macros,
                shape: options.shape,
                members: false,
                raw_dir: options.raw_dir.as_deref(),
            };
            holder_snapshot(
                palette,
                SubjectKind::Cluster,
                raw,
                uid,
                &reads,
                &text,
                fetched_at,
            )
        }),
        Ref::AppProfile { id, version } => live(&|palette| {
            app_profile_snapshot(palette, id, version.as_deref(), &text, fetched_at, progress)
        }),
        Ref::ClusterGroup { uid } => live(&|palette| {
            cluster_group_snapshot(palette, uid, options.macros, &text, fetched_at, progress)
        }),
        Ref::EdgeHost { uid, profile } => live(&|palette| {
            let raw = palette.fetch_subject(SubjectKind::EdgeHost, uid, progress)?;
            let reads = HolderReads {
                profile: profile.as_deref(),
                ..HolderReads::default()
            };
            holder_snapshot(
                palette,
                SubjectKind::EdgeHost,
                raw,
                uid,
                &reads,
                &text,
                fetched_at,
            )
        }),
    }
}

/// A record of the published template of the profile `uid`, with what `options` add.
///
/// The options add the macro digests, the raw answers saved, and the answer of Palette to
/// `--validate`.
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

/// What the record of a cluster, an edge host, or a cluster group reads beyond its release.
///
/// The default reads nothing more: the profiles that the holder runs, or pins, as they are.
#[derive(Debug, Clone, Copy, Default)]
struct HolderReads<'a> {
    /// The profiles to record, as the `profile` of [`Ref::Cluster`]. `None` records them all.
    profile: Option<&'a str>,
    /// Record what the holder runs after it accepts the update that waits.
    pending: bool,
    /// Record the digest of the value that Palette gives each macro that the content uses.
    macros: bool,
    /// Record the shape tier of a cluster, with one more request.
    shape: bool,
    /// Record the virtual clusters of a cluster group, with one more request. `verify` reads none:
    /// the record keeps them outside the root, and no verdict shows them.
    members: bool,
    /// Save the raw answers of Palette in this directory.
    raw_dir: Option<&'a Path>,
}

/// The record of a holder, from its raw documents `raw`, with what `reads` adds.
///
/// A holder is a cluster, an edge host, or a cluster group, and `kind` tells which. `raw` comes
/// from [`Palette::fetch_subject`], or from [`Palette::subject_raw`] when the caller read the
/// document already. `uid` is the holder as the caller names it. The document gives the uid when
/// it has one. `snapshot` and `verify` both make their record here, so the two records have the
/// same parts in the root. The record of a cluster group holds its virtual clusters, outside the
/// root, only when `reads.members` is true.
fn holder_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    kind: SubjectKind,
    mut raw: RawCluster,
    uid: &str,
    reads: &HolderReads<'_>,
    reference: &str,
    fetched_at: &str,
) -> Result<Snapshot> {
    if reads.pending {
        raw = palette.with_pending(raw)?;
    }
    // the ref can name the holder by name, and the document gives its uid
    let uid = first_nonempty(&[str_at(&raw.cluster, "/metadata/uid"), uid]);
    if reads.macros {
        raw.resolved_values = Some(palette.resolved_values(kind, &uid)?);
    }
    if reads.shape {
        raw.cloud_config = Some(palette.cloud_config(&raw.cluster)?);
    }
    if let Some(dir) = reads.raw_dir {
        raw.save_dir(dir, fetched_at)?;
    }
    let snap = Snapshot::from_cluster(&raw, reads.profile, reference, fetched_at)?;
    let mut snap = if reads.pending {
        snap.mark_pending()
    } else {
        snap
    }
    .with_kind(kind);
    if reads.members
        && let Some(c) = snap.source.cluster.as_mut()
    {
        c.members = palette
            .cluster_group_members(&uid)?
            .iter()
            .map(|m| GroupMember {
                uid: text(m, "/metadata/uid"),
                name: text(m, "/metadata/name"),
                project_uid: list::project_uid_of(m),
            })
            .collect();
    }
    Ok(snap)
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
    Snapshot::from_app_profile(
        &palette.fetch_app_profile(&uid, progress)?,
        reference,
        fetched_at,
    )
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
    let raw = palette.fetch_subject(SubjectKind::ClusterGroup, group, progress)?;
    let reads = HolderReads {
        macros,
        members: true,
        ..HolderReads::default()
    };
    holder_snapshot(
        palette,
        SubjectKind::ClusterGroup,
        raw,
        group,
        &reads,
        reference,
        fetched_at,
    )
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
    // one cache for the resolve and the validate: each registry listing is read one time
    let mut registries = Registries::new(&palette);
    let mut snap = candidate_snapshot(&loaded, &mut registries, fetched_at, progress)?;
    if validate {
        snap.source.palette_validation =
            Some(validate::profile_validation_in(&mut registries, &snap)?);
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
    candidate_snapshot(
        loaded,
        &mut Registries::new(&connect()?),
        fetched_at,
        progress,
    )
}

fn candidate_snapshot<A: PaletteApi>(
    loaded: &LoadedCandidate,
    registries: &mut Registries<'_, A>,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let palette = registries.palette();
    let mut snap = crate::candidate::resolve::resolve_with(loaded, registries, progress)?
        .into_snapshot(&loaded.path.display().to_string(), fetched_at)?;
    crate::candidate::warn_preset_options(&snap, progress);
    snap.source.template = crate::snapshot::Template::Candidate.into();
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
    clients: &'a mut HashMap<Option<String>, Palette<A>>,
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
    connected(clients, Some(project), connect)
}

/// The client of the scope `scope`: the one that `clients` holds for it, or a new one from
/// `connect`, which `clients` then holds.
///
/// `None` is the scope of the command, and `Some` with the empty text is the tenant. `verify` reads
/// each record in its own scope with it. An operation that has a home client reads a cluster
/// through [`client_for`].
fn connected<'a, A, F>(
    clients: &'a mut HashMap<Option<String>, Palette<A>>,
    scope: Option<&str>,
    connect: &mut F,
) -> Result<&'a Palette<A>>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    match clients.entry(scope.map(str::to_string)) {
        Entry::Occupied(e) => Ok(e.into_mut()),
        Entry::Vacant(v) => connect(scope).map(|p| &*v.insert(p)),
    }
}
