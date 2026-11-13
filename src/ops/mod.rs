//! The operations the CLI exposes, without argument parsing, printing, or exit codes.
//!
//! This module takes a snapshot and compares two. Each other command has a module of its own:
//! `verify`, `same_release`, `consumers`, `list`, `packs`, and `report`. `sides` gives the project
//! of each side of `compare` and `same-release`.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};

use crate::api::registry::Registries;
use crate::api::{Palette, PaletteApi, RawCluster};
use crate::candidate::pack::pack_snapshot;
use crate::diff::{self, Diff, Side};
use crate::error::{Error, Result};
use crate::json::{first_nonempty, str_at, text};
use crate::reference::Ref;
use crate::snapshot::{AppliedProfile, GroupMember, Snapshot, Source, SubjectKind};
use crate::term::progress::Progress;

mod behind;
mod consumer_list;
mod consumers;
mod fleet;
mod list;
mod live;
mod packs;
mod repave;
mod report;
mod same_release;
mod sides;
mod source;
mod validate;
mod verify;

pub use behind::*;
pub use consumer_list::*;
pub use consumers::*;
pub use fleet::*;
pub use list::*;
pub use packs::*;
pub use repave::*;
pub use report::*;
pub use same_release::*;
pub use sides::*;
pub use source::*;
pub use validate::*;
pub use verify::*;

// the command line writes each record through this public path, and names the records of its
// arguments through the other two
pub use crate::file::{expand_records, is_pattern, write_file};

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

/// The refusal of a command that reads one object, for a ref that names a set of clusters.
fn names_a_set(reference: &Ref) -> Error {
    Error::Usage {
        message: format!(
            "{reference} names a set of clusters, and this command reads one: \
             `strata snapshot {reference} --dir DIR` records each cluster, and \
             `strata list clusters` lists them"
        ),
    }
}

/// Turn a ref into a snapshot.
///
/// Strata loads a snapshot file, and reads each other ref through the [`Palette`] that `connect`
/// returns, with `options`. So a comparison of two files needs no credentials. The record names
/// the ref as it displays: the text as typed, with no empty part after a separator.
///
/// `connect` runs only when the ref needs Palette. `fetched_at` is the time of the read, in
/// RFC 3339: the record states it in `source.fetchedAt`.
///
/// # Errors
///
/// `Usage` for an option that does not fit the ref, and for a set of clusters, before any
/// request. Then the error of a request, such as a 404 for a uid that Palette does not have.
///
/// # Examples
///
/// Record a published profile version, read from an in-memory Palette:
///
/// ```
/// # use serde_json::json;
/// use strata::api::{MemoryApi, Palette};
/// use strata::ops::{self, SnapshotOptions};
/// use strata::reference::Ref;
/// use strata::term::progress::Progress;
///
/// # let uid = "64f0000000000000000000a1";
/// # let pack = json!({
/// #     "name": "cert-manager", "layer": "addon", "type": "spectro", "tag": "1.21.1",
/// #     "version": "1.21.1", "packUid": "p1", "registryUid": "r1", "digest": "sha256:0123",
/// #     "values": "installCRDs: true",
/// # });
/// # let api = MemoryApi::new()
/// #     .with(
/// #         format!("/v1/clusterprofiles/{uid}"),
/// #         json!({
/// #             "metadata": {"uid": uid, "name": "demo"},
/// #             "spec": {"version": "1.0.0", "published": {
/// #                 "cloudType": "aws", "type": "add-on", "packs": [pack],
/// #             }},
/// #         }),
/// #     )
/// #     .with(
/// #         format!("/v1/clusterprofiles/{uid}/packs/manifests"),
/// #         json!({"spec": {"packs": []}}),
/// #     )
/// #     .with(format!("/v1/clusterprofiles/{uid}/variables"), json!({"variables": []}));
/// let record = ops::snapshot_for(
///     &Ref::Uid(uid.to_string()),
///     &SnapshotOptions::default(),
///     || Ok(Palette::new(api)),
///     "2026-09-16T00:00:00Z",
///     &Progress::silent(),
/// )?;
/// assert_eq!(record.source.name, "demo");
/// assert_eq!(record.packs[0].key, "addon/cert-manager");
/// # Ok::<(), strata::Error>(())
/// ```
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
    let raw_dir = options.raw_dir.as_deref();
    let live = |read: &dyn Fn(&Palette<A>) -> Result<Snapshot>| read_live(connect, raw_dir, read);
    match reference {
        Ref::Snapshot(path) => Snapshot::load(path),
        Ref::ClusterSet { .. } => Err(names_a_set(reference)),
        Ref::NameVersion { name, version } => live(&|palette| {
            let uid = palette.profile_uid(name, Some(version), progress)?;
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
            pack_snapshot(
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
            let reads = HolderReads::cluster(profile.as_deref(), *pending, options);
            cluster_snapshot(palette, uid, &reads, &text, fetched_at, progress)
        }),
        Ref::AppProfile { id, version } => live(&|palette| {
            app_profile_snapshot(palette, id, version.as_deref(), &text, fetched_at, progress)
        }),
        Ref::ClusterGroup { uid } => live(&|palette| {
            cluster_group_snapshot(palette, uid, options.macros, &text, fetched_at, progress)
        }),
        Ref::EdgeHost { uid, profile } => live(&|palette| {
            let reads = HolderReads {
                profile: profile.as_deref(),
                ..HolderReads::default()
            };
            held_snapshot(
                palette,
                SubjectKind::EdgeHost,
                uid,
                &reads,
                &text,
                fetched_at,
                progress,
            )
        }),
    }
}

/// The record that `read` makes through the [`Palette`] that `connect` returns. A ref that a
/// request answers connects first, and the record names the project of the client and the
/// registry of each pack. `raw_dir` is the directory of `--raw-dir`, which keeps the registries.
fn read_live<A, F>(
    connect: F,
    raw_dir: Option<&Path>,
    read: impl FnOnce(&Palette<A>) -> Result<Snapshot>,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    let palette = connect()?;
    let mut snap = read(&palette)?;
    source::name_registries(&mut Registries::new(&palette), &mut snap, raw_dir)?;
    Ok(snap.with_project(palette.project_uid()))
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

impl<'a> HolderReads<'a> {
    /// What `snapshot` reads of a cluster: the `profile` and the `pending` update of its ref, and
    /// what `options` ask for.
    fn cluster(profile: Option<&'a str>, pending: bool, options: &'a SnapshotOptions) -> Self {
        HolderReads {
            profile,
            pending,
            macros: options.macros,
            shape: options.shape,
            members: false,
            raw_dir: options.raw_dir.as_deref(),
        }
    }
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
///
/// `reference` is the text of the ref that the record names as its source. `fetched_at` is the
/// time of the read, in RFC 3339.
///
/// # Errors
///
/// [`Error::AppProfileNotFound`] or [`Error::AppProfileNameAmbiguous`] when a name and `version`
/// name no app profile, or several. The error of a request, such as a 404 for a uid that Palette
/// does not have.
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
/// `group` is the uid or the name of the cluster group. `reference` is the text of the ref that
/// the record names as its source, and `fetched_at` is the time of the read, in RFC 3339.
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
    let reads = HolderReads {
        macros,
        members: true,
        ..HolderReads::default()
    };
    held_snapshot(
        palette,
        SubjectKind::ClusterGroup,
        group,
        &reads,
        reference,
        fetched_at,
        progress,
    )
}

/// A record of the cluster `uid`, from the document that Palette gives for it, with what `reads`
/// ask for.
fn cluster_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    reads: &HolderReads<'_>,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    held_snapshot(
        palette,
        SubjectKind::Cluster,
        uid,
        reads,
        reference,
        fetched_at,
        progress,
    )
}

/// A record of the holder `uid` of `kind`, from the document that Palette gives for it, with what
/// `reads` ask for.
fn held_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    kind: SubjectKind,
    uid: &str,
    reads: &HolderReads<'_>,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let raw = palette.fetch_subject(kind, uid, progress)?;
    holder_snapshot(palette, kind, raw, uid, reads, reference, fetched_at)
}

/// The two sides of a comparison, each as [`snapshot_for`] reads it.
///
/// A cluster ref that selects no profile holds only the profiles that the other side asks about.
/// Beside a cluster group, it holds the profiles that the group pins. Beside one version of a
/// cluster profile, it holds the profile that the version replaces, as [`validate_body`] does.
/// That is the profile with the uid of the version, else with its name. Else, for a version that
/// builds a cluster, it is the profile that builds the cluster. The question is whether the
/// cluster runs the pin or the version, and not what else it runs. So the function reads the other side first. Two clusters
/// stay whole. `connect` gives the [`Palette`] of a [`Side`], for each side that a request
/// answers, so each side can read its own project. `fetched_at` is the time of the read, in
/// RFC 3339.
///
/// # Errors
///
/// The errors of [`snapshot_for`] for the first side that fails.
pub fn snapshot_pair<A, F>(
    a: &Ref,
    b: &Ref,
    mut connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<(Snapshot, Snapshot)>
where
    A: PaletteApi,
    F: FnMut(Side) -> Result<Palette<A>>,
{
    let open = |r: &Ref| matches!(r, Ref::Cluster { profile: None, .. });
    let content = SnapshotOptions::default();
    if open(a) && !open(b) {
        let sb = snapshot_for(b, &content, || connect(Side::B), fetched_at, progress)?;
        let sa = side_beside(a, &sb, || connect(Side::A), fetched_at, progress)?;
        Ok((sa, sb))
    } else {
        let sa = snapshot_for(a, &content, || connect(Side::A), fetched_at, progress)?;
        let sb = side_beside(b, &sa, || connect(Side::B), fetched_at, progress)?;
        Ok((sa, sb))
    }
}

/// The record of `r`, one side of a comparison, beside `other`, the record of the other side: see
/// [`snapshot_pair`].
fn side_beside<A, F>(
    r: &Ref,
    other: &Snapshot,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    match r {
        Ref::Cluster {
            uid,
            profile: None,
            pending,
        } if other.source.is_profile_version() => read_live(connect, None, |palette| {
            cluster_beside(palette, uid, *pending, &other.source, fetched_at, progress)
        }),
        _ => snapshot_for(
            &narrowed(r, other),
            &SnapshotOptions::default(),
            connect,
            fetched_at,
            progress,
        ),
    }
}

/// The record of the cluster `uid` beside the profile version `target`. It holds only the profile
/// that `target` replaces, and it names the ref as `cluster:<uid>/<profile>`.
///
/// The cluster document shows the profile, so the selection comes after the read. A cluster that
/// runs no profile that `target` replaces stays whole, because the move adds `target`. With
/// `pending`, the record holds the update that waits for the cluster.
fn cluster_beside<A: PaletteApi>(
    palette: &Palette<A>,
    uid: &str,
    pending: bool,
    target: &Source,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let raw = palette.fetch_subject(SubjectKind::Cluster, uid, progress)?;
    let running = AppliedProfile::of_cluster(&raw.cluster);
    let profile = AppliedProfile::replaced_by(&running, target).map(|p| p.name.clone());
    let reference = Ref::Cluster {
        uid: uid.to_string(),
        profile: profile.clone(),
        pending,
    };
    let reads = HolderReads {
        profile: profile.as_deref(),
        pending,
        ..HolderReads::default()
    };
    holder_snapshot(
        palette,
        SubjectKind::Cluster,
        raw,
        uid,
        &reads,
        &reference.to_string(),
        fetched_at,
    )
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

/// Build a snapshot of the draft of a profile version, for `<ref>@draft`.
///
/// `id` is a uid when there is no `version`, and else a name at `version`. The snapshot has no
/// project: the caller sets it. `reference` is the text of the ref that the record names as its
/// source, and `fetched_at` is the time of the read, in RFC 3339.
///
/// # Errors
///
/// [`Error::ProfileNotFound`] or [`Error::ProfileNameAmbiguous`] when a name and `version` name no
/// profile, or several. [`Error::NoDraft`] for a profile with no draft. The error of a request.
pub fn draft_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    id: &str,
    version: Option<&str>,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let uid = palette.profile_uid(id, version, progress)?;
    let raw = palette.fetch_profile(&uid, progress)?;
    Snapshot::from_raw_draft(&raw, reference, fetched_at)
}

/// Compare two snapshots.
///
/// `a` is the base side and `b` is the candidate side: the diff reads a move from `a` to `b`.
/// `generated_at` is the time of the diff, in RFC 3339. The comparison reads nothing from Palette.
///
/// # Examples
///
/// Two records of one profile version have one root, so the diff says that nothing changed:
///
/// ```
/// # use serde_json::json;
/// use strata::api::{MemoryApi, Palette};
/// use strata::ops::{self, SnapshotOptions};
/// use strata::reference::Ref;
/// use strata::term::progress::Progress;
///
/// # let uid = "64f0000000000000000000a1";
/// # let pack = json!({
/// #     "name": "cert-manager", "layer": "addon", "type": "spectro", "tag": "1.21.1",
/// #     "version": "1.21.1", "packUid": "p1", "registryUid": "r1", "digest": "sha256:0123",
/// #     "values": "installCRDs: true",
/// # });
/// # let api = MemoryApi::new()
/// #     .with(
/// #         format!("/v1/clusterprofiles/{uid}"),
/// #         json!({
/// #             "metadata": {"uid": uid, "name": "demo"},
/// #             "spec": {"version": "1.0.0", "published": {
/// #                 "cloudType": "aws", "type": "add-on", "packs": [pack],
/// #             }},
/// #         }),
/// #     )
/// #     .with(
/// #         format!("/v1/clusterprofiles/{uid}/packs/manifests"),
/// #         json!({"spec": {"packs": []}}),
/// #     )
/// #     .with(format!("/v1/clusterprofiles/{uid}/variables"), json!({"variables": []}));
/// # let record = |at: &str| {
/// #     ops::snapshot_for(
/// #         &Ref::Uid(uid.to_string()),
/// #         &SnapshotOptions::default(),
/// #         || Ok(Palette::new(api.clone())),
/// #         at,
/// #         &Progress::silent(),
/// #     )
/// # };
/// let (a, b) = (record("2026-09-16T00:00:00Z")?, record("2026-09-17T00:00:00Z")?);
/// let diff = ops::compare(&a, &b, "2026-09-17T00:00:00Z");
/// assert!(diff.same, "the roots are equal");
/// # Ok::<(), strata::Error>(())
/// ```
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
