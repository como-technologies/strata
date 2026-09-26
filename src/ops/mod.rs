//! The operations the CLI exposes, without argument parsing, printing, or exit codes.
//!
//! This module takes a snapshot and compares two. Each other command has a module of its own:
//! `verify`, `same_release`, `consumers`, `list`, and `packs`.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

use serde_json::Value;

use crate::api::{Palette, PaletteApi, RawCluster, RawProfile};
use crate::candidate::LoadedCandidate;
use crate::diff::{self, Diff};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::reference::Ref;
use crate::snapshot::{GroupMember, Snapshot, SubjectKind};

mod behind;
mod consumers;
mod fleet;
mod list;
mod packs;
mod repave;
mod same_release;
mod verify;

pub use behind::*;
pub use consumers::*;
pub use fleet::*;
pub use list::*;
pub use packs::*;
pub use repave::*;
pub use same_release::*;
pub use verify::*;

/// Turn a ref into a snapshot. A snapshot file is loaded; anything else is fetched through the
/// [`Palette`] that `connect` returns, so comparing two files never needs credentials.
pub fn snapshot_for<A, F>(
    reference: &Ref,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    match reference {
        Ref::Snapshot(path) => Snapshot::load(path),
        Ref::NameVersion { name, version } => {
            let palette = connect()?;
            let uid = palette.resolve(name, version)?;
            let raw = palette.fetch_profile(&uid, progress)?;
            Ok(
                Snapshot::from_raw(&raw, &reference.to_string(), fetched_at)?
                    .with_project(palette.project_uid()),
            )
        }
        Ref::Uid(uid) => {
            let palette = connect()?;
            let raw = palette.fetch_profile(uid, progress)?;
            Ok(Snapshot::from_raw(&raw, uid, fetched_at)?.with_project(palette.project_uid()))
        }
        Ref::Pack {
            registry,
            name,
            version,
        } => {
            let palette = connect()?;
            Ok(crate::candidate::pack_snapshot(
                &palette,
                registry.as_deref(),
                name,
                version,
                &reference.to_string(),
                fetched_at,
                progress,
            )?
            .with_project(palette.project_uid()))
        }
        Ref::Cluster {
            uid,
            profile,
            pending,
        } => {
            let palette = connect()?;
            let mut raw = palette.fetch_cluster(uid, progress)?;
            if *pending {
                raw = palette.with_pending(raw)?;
            }
            let snap = Snapshot::from_cluster(
                &raw,
                profile.as_deref(),
                &reference.to_string(),
                fetched_at,
            )?;
            let snap = if *pending { snap.mark_pending() } else { snap };
            Ok(snap.with_project(palette.project_uid()))
        }
        Ref::ClusterSet { .. } => Err(Error::Usage {
            message: format!(
                "{reference} names a set of clusters, and this command reads one: \
                 `strata snapshot {reference} --dir DIR` records each cluster, and \
                 `strata list clusters` lists them"
            ),
        }),
        Ref::AppProfile { id, version } => {
            let palette = connect()?;
            Ok(app_profile_snapshot(
                &palette,
                id,
                version.as_deref(),
                &reference.to_string(),
                fetched_at,
                progress,
            )?
            .with_project(palette.project_uid()))
        }
        Ref::ClusterGroup { uid } => {
            let palette = connect()?;
            Ok(
                cluster_group_snapshot(
                    &palette,
                    uid,
                    &reference.to_string(),
                    fetched_at,
                    progress,
                )?
                .with_project(palette.project_uid()),
            )
        }
        Ref::EdgeHost { uid, profile } => {
            let palette = connect()?;
            let raw = palette.fetch_edge_host(uid, progress)?;
            Ok(Snapshot::from_cluster(
                &raw,
                profile.as_deref(),
                &reference.to_string(),
                fetched_at,
            )?
            .with_kind(SubjectKind::EdgeHost)
            .with_project(palette.project_uid()))
        }
    }
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
    let uid = match version {
        Some(v) => palette.resolve_app_profile(id, v)?,
        None => id.to_string(),
    };
    let mut snap = Snapshot::from_raw(
        &palette.fetch_app_profile(&uid, progress)?,
        reference,
        fetched_at,
    )?;
    snap.source.consumers = None;
    Ok(snap)
}

/// A record of a cluster group: the profiles that it pins, and its virtual clusters.
pub fn cluster_group_snapshot<A: PaletteApi>(
    palette: &Palette<A>,
    group: &str,
    reference: &str,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let raw = palette.fetch_cluster_group(group, progress)?;
    let uid = text(&raw.cluster, "/metadata/uid");
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
    if open(a) && !open(b) {
        let sb = snapshot_for(b, &mut connect, fetched_at, progress)?;
        let sa = snapshot_for(&narrowed(a, &sb), &mut connect, fetched_at, progress)?;
        Ok((sa, sb))
    } else {
        let sa = snapshot_for(a, &mut connect, fetched_at, progress)?;
        let sb = snapshot_for(&narrowed(b, &sa), &mut connect, fetched_at, progress)?;
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

/// Build a snapshot of a candidate file: read and check it (no network), then resolve its packs
/// through the [`Palette`] that `connect` returns. The snapshot has `source.template` `candidate`
/// and no uid.
pub fn snapshot_from_candidate<A, F>(
    path: &Path,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    snapshot_from_loaded(
        &crate::candidate::load(path)?,
        connect,
        fetched_at,
        progress,
    )
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
    let palette = connect()?;
    let raw = crate::candidate::resolve(loaded, &palette, progress)?;
    let mut snap = Snapshot::from_raw(&raw, &loaded.path.display().to_string(), fetched_at)?;
    snap.source.template = crate::candidate::TEMPLATE.to_string();
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

/// Compare two snapshots.
pub fn compare(a: &Snapshot, b: &Snapshot, generated_at: &str) -> Diff {
    diff::compute(a, b, generated_at, None)
}

/// The string at `ptr` in `v`, or empty.
fn text(v: &Value, ptr: &str) -> String {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The uids of the clusters a profile document says use it: `status.inUseClusterUids`, then any
/// uid that only `status.inUseClusters` names, in order and without repeats.
fn consumer_uids(doc: &Value) -> Vec<String> {
    let mut uids: Vec<String> = doc
        .pointer("/status/inUseClusterUids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let named = doc
        .pointer("/status/inUseClusters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for n in named {
        let u = text(n, "/uid");
        if !u.is_empty() && !uids.contains(&u) {
            uids.push(u);
        }
    }
    uids
}

/// The entries of a listing: its `items` array, or the document itself when it is the array.
fn items(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("items")
        .or(Some(doc))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
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
