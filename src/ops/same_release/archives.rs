//! The compare of the two archives of a renumber, from the download of each profile version.

use std::collections::HashMap;
use std::collections::btree_map::{BTreeMap, Entry};

use super::ArchiveNotReadable;
use crate::api::{PackFile, Palette, PaletteApi, ProfileDownload};
use crate::error::Result;
use crate::ops::{connected, scope_of};
use crate::snapshot::{Pack, Snapshot, Template};
use crate::term::progress::Progress;

/// What strata found when it compared the two archives of one renumber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArchiveCheck {
    /// The two archives hold the same files. `pack.json` differs only by its `version`, and
    /// `Chart.yaml` only by its line `version`.
    Same,
    /// The path of each file that differs between the two archives.
    Changed(Vec<String>),
    /// The file of the pack in the download of a side is not a tar archive that gzip compresses:
    /// each such side, with its reason. Strata cannot show that the files of the two builds are
    /// the same.
    NotReadable(Vec<ArchiveNotReadable>),
    /// Strata did not compare the archives. A side is no published version of a cluster profile,
    /// or its download holds no archive of the pack at the version of the record.
    NotCompared,
}

/// The project and the uid of the profile version of a record whose download Palette serves.
type VersionAt = (Option<String>, String);

/// The download of each profile version of a chain, read once.
///
/// Palette takes one download of a version in 10 seconds. So strata asks for each version once,
/// and each pair reads the answer again.
pub(super) struct Downloads<'p, A: PaletteApi, F> {
    connect: F,
    clients: HashMap<Option<String>, Palette<A>>,
    files: BTreeMap<VersionAt, ProfileDownload>,
    progress: &'p Progress,
}

impl<'p, A, F> Downloads<'p, A, F>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    /// No download yet. `connect` gives the client of a project, once for each project.
    pub(super) fn new(connect: F, progress: &'p Progress) -> Self {
        Downloads {
            connect,
            clients: HashMap::new(),
            files: BTreeMap::new(),
            progress,
        }
    }

    /// Compare the archive of the pack `x` of the record `a` with the archive of the pack `y` of
    /// the record `b`. Read the download of each version first, when no pair read it before.
    pub(super) fn check(
        &mut self,
        (a, x): (&Snapshot, &Pack),
        (b, y): (&Snapshot, &Pack),
    ) -> Result<ArchiveCheck> {
        let (Some(va), Some(vb)) = (version_of(a), version_of(b)) else {
            return Ok(ArchiveCheck::NotCompared);
        };
        self.read(&va, a)?;
        self.read(&vb, b)?;
        let sides = [
            (self.files.get(&va).and_then(|d| file_of(d, x)), x),
            (self.files.get(&vb).and_then(|d| file_of(d, y)), y),
        ];
        let not_readable: Vec<ArchiveNotReadable> = sides
            .iter()
            .filter_map(|(file, p)| match file {
                Some(PackFile::NotReadable(reason)) => Some(ArchiveNotReadable {
                    key: p.key.clone(),
                    version: p.version.clone(),
                    reason: reason.clone(),
                }),
                _ => None,
            })
            .collect();
        if !not_readable.is_empty() {
            return Ok(ArchiveCheck::NotReadable(not_readable));
        }
        Ok(match sides.map(|(file, _)| file) {
            [Some(PackFile::Archive(ax)), Some(PackFile::Archive(ay))] => {
                match ax.changed_files(ay) {
                    files if files.is_empty() => ArchiveCheck::Same,
                    files => ArchiveCheck::Changed(files),
                }
            }
            _ => ArchiveCheck::NotCompared,
        })
    }

    /// Read the download of the version `v` of the record `s`, when no pair read it before.
    fn read(&mut self, v: &VersionAt, s: &Snapshot) -> Result<()> {
        if let Entry::Vacant(entry) = self.files.entry(v.clone()) {
            self.progress.note(&format!(
                "reading the download of {} from Palette",
                s.source.title()
            ));
            let palette = connected(&mut self.clients, v.0.as_deref(), &mut self.connect)?;
            entry.insert(palette.profile_download(&v.1)?);
        }
        Ok(())
    }
}

/// The project and the uid of the version of the record `s`, when Palette serves a download of
/// it. Such a record holds the published template of a version of a cluster profile. A
/// candidate, a draft, a cluster, a pack, and an app profile have none.
fn version_of(s: &Snapshot) -> Option<VersionAt> {
    let source = &s.source;
    (source.is_profile_version()
        && source.template == Template::Published
        && !source.uid.is_empty())
    .then(|| {
        (
            scope_of(&source.project_uid).map(str::to_string),
            source.uid.clone(),
        )
    })
}

/// The file of the pack `p` in the download `d`, when the list of the download names the pack at
/// the version of `p`. A download of a version that changed since its record can hold another
/// version.
fn file_of<'d>(d: &'d ProfileDownload, p: &Pack) -> Option<&'d PackFile> {
    d.pack(&p.name)
        .filter(|q| q.version == p.version)
        .map(|q| &q.file)
}
