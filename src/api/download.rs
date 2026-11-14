//! The download of a profile version: the archive of each pack of its published content.
//!
//! `GET /v1/clusterprofiles/{uid}/spc/download` gives a tar archive that gzip compresses. All its
//! paths start with one folder. In that folder, `metadata.json` names the list of the packs in
//! `profiles[].packsMetaFilePath`. The list maps the name of each pack to its `version`, its
//! `type`, and its `filePath`. `filePath` is the path of the archive of the pack from the folder of
//! the list.
//! The archive of a pack is a tar archive that gzip compresses, with one top folder. An empty
//! `filePath` means that the download holds no archive of the pack. The file of a pack can also be
//! another file, such as an error answer of the host of a Helm chart, or no file at all: see
//! [`PackFile::NotReadable`]. Strata keeps each other pack of such a download.
//!
//! Palette resolves the macros in the other files of the download, so they can hold the value of
//! a macro. Strata reads the two lists and the archives of the packs, and no other file. It keeps
//! a digest of each file of an archive, and not the file.

use std::collections::BTreeMap;
use std::io::Read;

use serde_json::Value;

use super::Method;
use crate::content::canon::{canonical_json, sha256_digest};
use crate::error::{Error, Result};
use crate::json::{arr, object_at, opt_str, str_at};
use crate::text::{cut, plural};

/// The file at the top of a download that names the list of the packs of each profile.
const METADATA: &str = "metadata.json";

/// The name of the file of a list of the packs. The list of a profile is in the folder of that
/// profile.
const PACKS_METADATA: &str = "packs-metadata.json";

/// The file of a pack archive that names the pack and its version.
const PACK_FILE: &str = "pack.json";

/// The file of a Helm chart that names the chart and its version. Palette puts the chart of a Helm
/// registry in the download as the registry serves it, with no [`PACK_FILE`].
const CHART_FILE: &str = "Chart.yaml";

/// The field of [`PACK_FILE`] and of [`CHART_FILE`] that names the version. The build of a pack or
/// of a chart under another version changes only this field: strata does not compare it.
const VERSION_FIELD: &str = "version";

/// The ends of the name of an archive in a download: a pack archive and a Helm chart.
const ARCHIVE_ENDS: [&str; 2] = [".tar.gz", ".tgz"];

/// The bytes of one MiB.
const MIB: u64 = 1024 * 1024;

/// The most bytes that strata reads from the files of one download, after the gzip, with the files
/// of each pack archive in it. A larger download is an error, so a small file cannot fill the
/// memory.
const UNPACKED_LIMIT: u64 = 1024 * MIB;

/// The most characters of a file or of an error that the reason of [`PackFile::NotReadable`]
/// shows.
const REASON_TEXT: usize = 120;

/// The download of one profile version: each pack that its list names, by the name of the pack.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDownload {
    packs: BTreeMap<String, DownloadedPack>,
}

/// One pack of a download, as the list of the packs names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedPack {
    /// The version of the pack, as the list gives it.
    pub version: String,
    /// What the download holds for the pack: its archive, no archive, or a file that strata cannot
    /// read.
    pub file: PackFile,
}

/// What the download holds for one pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackFile {
    /// The download holds no archive of the pack: its entry has an empty `filePath`. Palette serves
    /// none for a pack of a private pack registry or Helm registry, or of an OCI registry of Helm
    /// charts or of Zarf packages.
    NoArchive,
    /// The archive of the pack.
    Archive(PackArchive),
    /// The file of the pack is not a tar archive that gzip compresses, or the download does not
    /// hold it: the reason.
    ///
    /// The reason gives the size of the file. When the file is text, it gives the start of the
    /// text on one line, such as `263 bytes of text: <?xml ...`. Else it gives the error of the
    /// read. Palette puts the answer of the host of a Helm chart in the download in place of the
    /// chart, also an error answer, and the route answers HTTP 200. When the list names a file
    /// that the download does not hold, the reason is `the download does not hold <path>`.
    NotReadable(String),
}

/// The files of the archive of one pack: the path of each file under its top folder, with a
/// digest of the file.
///
/// The digest of `pack.json` covers its JSON with no `version`. The digest of `Chart.yaml` covers
/// its lines with no line `version`. So two builds of one content under two versions have the same
/// files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackArchive {
    files: BTreeMap<String, String>,
}

impl ProfileDownload {
    /// The pack named `name`, when the list of the packs names it. A manifest pack has no entry
    /// of its own in the list.
    pub fn pack(&self, name: &str) -> Option<&DownloadedPack> {
        self.packs.get(name)
    }

    /// Each pack that the list names, in the order of the names.
    pub fn packs(&self) -> impl Iterator<Item = (&str, &DownloadedPack)> {
        self.packs.iter().map(|(name, pack)| (name.as_str(), pack))
    }

    /// Read `file`, the download that `path` gave.
    pub(crate) fn read(path: &str, file: &[u8]) -> Result<ProfileDownload> {
        read_download(file, UNPACKED_LIMIT).map_err(|message| Error::NotArchive {
            method: Method::Get,
            path: path.to_string(),
            message,
        })
    }
}

impl DownloadedPack {
    /// The archive of the pack, when the download holds one that strata reads.
    pub fn archive(&self) -> Option<&PackArchive> {
        match &self.file {
            PackFile::Archive(archive) => Some(archive),
            PackFile::NoArchive | PackFile::NotReadable(_) => None,
        }
    }
}

impl PackArchive {
    /// Each file of the archive, by its path in the order of the paths, with its digest.
    pub fn files(&self) -> impl Iterator<Item = (&str, &str)> {
        self.files
            .iter()
            .map(|(path, digest)| (path.as_str(), digest.as_str()))
    }

    /// The path of each file that differs between this archive and `other`, in the order of the
    /// paths. A file differs when its digest differs, or when one archive alone holds it.
    pub fn changed_files(&self, other: &PackArchive) -> Vec<String> {
        let mut paths: Vec<String> = self
            .files
            .iter()
            .filter(|(path, digest)| other.files.get(*path) != Some(*digest))
            .chain(
                other
                    .files
                    .iter()
                    .filter(|(path, _)| !self.files.contains_key(*path)),
            )
            .map(|(path, _)| path.clone())
            .collect();
        paths.sort();
        paths
    }
}

/// The bytes that strata can still read from the files of one download, and the limit of the
/// read: see [`UNPACKED_LIMIT`].
struct Budget {
    left: u64,
    limit: u64,
}

impl Budget {
    /// The budget of a read of `limit` bytes at most.
    fn new(limit: u64) -> Self {
        Budget { left: limit, limit }
    }

    /// Take `size` bytes, or fail with the limit of the read when fewer are left.
    fn take(&mut self, size: u64) -> std::result::Result<(), Fault> {
        self.left = self
            .left
            .checked_sub(size)
            .ok_or(Fault::Limit(self.limit))?;
        Ok(())
    }
}

/// Why strata cannot read a tar archive that gzip compresses.
#[derive(Debug, PartialEq, Eq)]
enum Fault {
    /// The files of the download hold more bytes than the limit of the read, such as
    /// [`UNPACKED_LIMIT`]. Strata refuses the whole download, also for the file of one pack.
    Limit(u64),
    /// The bytes are not a tar archive that gzip compresses: the error of the read.
    Format(String),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // a limit of whole MiB, as UNPACKED_LIMIT is, reads in MiB
            Fault::Limit(limit) if limit % MIB == 0 => {
                write!(f, "the files hold more than {} MiB", limit / MIB)
            }
            Fault::Limit(limit) => write!(f, "the files hold more than {}", byte_count(*limit)),
            Fault::Format(error) => f.write_str(error),
        }
    }
}

/// Read the download `file`: the two lists, and the archive of each pack. Strata reads `limit`
/// bytes at most from its files: see [`UNPACKED_LIMIT`].
///
/// The steps: unpack the lists and the archives, and no other file. Find the one `metadata.json`
/// at the top. Read the list of the packs that it names. Then read each archive that the list
/// names, from the folder of the list. A file of a pack that is not an archive, or that the
/// download does not hold, gets its reason, and the other packs stay.
fn read_download(file: &[u8], limit: u64) -> std::result::Result<ProfileDownload, String> {
    let mut budget = Budget::new(limit);
    let files = unpack(file, &mut budget, |path| {
        file_name(path) == METADATA
            || file_name(path) == PACKS_METADATA
            || ARCHIVE_ENDS.iter().any(|end| path.ends_with(end))
    })
    .map_err(|fault| fault.to_string())?;
    let mut tops = files
        .keys()
        .filter_map(|path| path.strip_suffix(METADATA)?.strip_suffix('/'))
        .filter(|top| !top.is_empty() && !top.contains('/'));
    let top = match (tops.next(), tops.next()) {
        (Some(top), None) => top,
        (None, _) => return Err(format!("no {METADATA} at the top")),
        (Some(_), Some(_)) => return Err(format!("two folders at the top hold {METADATA}")),
    };
    let metadata = json_file(&files, &format!("{top}/{METADATA}"))?;
    let lists: Vec<&str> = arr(&metadata, "/profiles")
        .iter()
        .filter_map(|p| opt_str(p, "/packsMetaFilePath"))
        .collect();
    let [list] = lists.as_slice() else {
        return Err(format!(
            "{METADATA} names {} lists of packs, and not one",
            lists.len()
        ));
    };
    let list_path = format!("{top}/{list}");
    let folder = list_path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let entries = json_file(&files, &list_path)?;
    let Some(entries) = object_at(&entries, "/packs") else {
        return Err(format!("{list} has no packs"));
    };
    let mut packs = BTreeMap::new();
    for (name, entry) in entries {
        // the list names each manifest of a pack too, under a key of its own
        if !str_at(entry, "/manifestName").is_empty() || str_at(entry, "/name") != name.as_str() {
            continue;
        }
        let file = match str_at(entry, "/filePath") {
            "" => PackFile::NoArchive,
            path => match files.get(&format!("{folder}/{path}")) {
                Some(bytes) => {
                    pack_file(bytes, &mut budget).map_err(|fault| format!("{path}: {fault}"))?
                }
                None => {
                    PackFile::NotReadable(format!("the download does not hold {}", one_line(path)))
                }
            },
        };
        let version = str_at(entry, "/version").to_string();
        packs.insert(name.clone(), DownloadedPack { version, file });
    }
    Ok(ProfileDownload { packs })
}

/// The file `bytes` of one pack: its archive, or the reason when it is not a tar archive that gzip
/// compresses. Only [`Fault::Limit`] is an error, because it refuses the whole download.
fn pack_file(bytes: &[u8], budget: &mut Budget) -> std::result::Result<PackFile, Fault> {
    match pack_archive(bytes, budget) {
        Ok(archive) => Ok(PackFile::Archive(archive)),
        Err(Fault::Format(error)) => Ok(PackFile::NotReadable(not_readable(bytes, &error))),
        Err(Fault::Limit(limit)) => Err(Fault::Limit(limit)),
    }
}

/// The reason of the file `bytes` of a pack, whose read gave `error`: its size, and the start of
/// its text when it is text, else `error`.
///
/// The file is text when it is UTF-8 with no control character other than white space. An example
/// is `263 bytes of text: <?xml version="1.0" ...`. See [`one_line`] for the form of the text.
fn not_readable(bytes: &[u8], error: &str) -> String {
    let size = byte_count(bytes.len() as u64);
    // a file that is no UTF-8 is no text, and the reason gives the error of the read
    let text = std::str::from_utf8(bytes)
        .ok()
        .filter(|text| !text.chars().any(|c| c.is_control() && !c.is_whitespace()))
        .map(one_line)
        .filter(|line| !line.is_empty());
    match text {
        Some(line) => format!("{size} of text: {line}"),
        None => format!("{size}: {}", one_line(error)),
    }
}

/// `bytes` as a count, such as `1 byte` or `263 bytes`.
fn byte_count(bytes: u64) -> String {
    // only a count of one is singular, so a count above the range of `usize` reads as many
    let n = usize::try_from(bytes).unwrap_or(usize::MAX);
    format!("{bytes} {}", plural(n, "byte", "bytes"))
}

/// The start of `text` on one line, as a reason shows it.
///
/// Each run of white space becomes one space, and strata drops each other control character. So
/// the answer of the host of a chart moves no cursor in a terminal. The line holds [`REASON_TEXT`]
/// characters at most, and [`cut`] marks a cut.
fn one_line(text: &str) -> String {
    let line: String = text
        .split_whitespace()
        .flat_map(|word| std::iter::once(' ').chain(word.chars()))
        .filter(|c| !c.is_control())
        .skip(1)
        .take(REASON_TEXT + 1)
        .collect();
    cut(&line, REASON_TEXT)
}

/// The last segment of `path`.
fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// The JSON of the file at `path` of `files`.
fn json_file(files: &BTreeMap<String, Vec<u8>>, path: &str) -> std::result::Result<Value, String> {
    let bytes = files
        .get(path)
        .ok_or_else(|| format!("the download does not hold {path}"))?;
    serde_json::from_slice(bytes).map_err(|e| format!("{path} is not JSON: {e}"))
}

/// The files of the pack archive `gz`, by their path under its top folder, with a digest of each.
///
/// The paths keep the top folder when the files do not share one. The digest of `pack.json`
/// covers its JSON with no `version`, when the file is a JSON object. The digest of `Chart.yaml`
/// covers its lines with no line `version`.
fn pack_archive(gz: &[u8], budget: &mut Budget) -> std::result::Result<PackArchive, Fault> {
    let files = unpack(gz, budget, |_| true)?;
    let top = files
        .keys()
        .next()
        .and_then(|path| path.split_once('/'))
        .map(|(top, _)| format!("{top}/"))
        .filter(|top| files.keys().all(|path| path.starts_with(top.as_str())));
    let files = files
        .into_iter()
        .map(|(path, bytes)| {
            let path = match top.as_deref().and_then(|top| path.strip_prefix(top)) {
                Some(under) => under.to_string(),
                None => path,
            };
            let digest = match path.as_str() {
                PACK_FILE => pack_file_digest(&bytes),
                CHART_FILE => chart_file_digest(&bytes),
                _ => sha256_digest(&bytes),
            };
            (path, digest)
        })
        .collect();
    Ok(PackArchive { files })
}

/// The digest of the bytes of `pack.json`: of its canonical JSON with no `version` when the file is
/// a JSON object, else of the bytes.
fn pack_file_digest(bytes: &[u8]) -> String {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(mut fields)) => {
            fields.remove(VERSION_FIELD);
            sha256_digest(&canonical_json(&Value::Object(fields)))
        }
        _ => sha256_digest(bytes),
    }
}

/// The digest of the bytes of `Chart.yaml` with no line of its top field `version`.
///
/// Each other line keeps its bytes and its place. So a change of `appVersion`, of a comment, of
/// the order of the lines, or of the `version` of a dependency changes the digest.
fn chart_file_digest(bytes: &[u8]) -> String {
    let kept: Vec<u8> = bytes
        .split_inclusive(|b| *b == b'\n')
        .filter(|line| !is_version_line(line))
        .flatten()
        .copied()
        .collect();
    sha256_digest(&kept)
}

/// True when the line `line` of a YAML file sets the top field `version`: the line starts with
/// the key, with no indent.
fn is_version_line(line: &[u8]) -> bool {
    line.strip_prefix(VERSION_FIELD.as_bytes())
        .is_some_and(|rest| rest.trim_ascii_start().starts_with(b":"))
}

/// Each regular file of the tar archive that gzip compresses in `gz`, whose path `keep` takes, by
/// its path: its bytes.
///
/// A path loses a `./` at its start. A directory, a link, and each other entry that is not a
/// regular file have no bytes, so the map does not hold them. When two entries have one path,
/// the last one counts, as `tar` extracts it.
fn unpack(
    gz: &[u8],
    budget: &mut Budget,
    keep: impl Fn(&str) -> bool,
) -> std::result::Result<BTreeMap<String, Vec<u8>>, Fault> {
    let format = |e: std::io::Error| Fault::Format(e.to_string());
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
    let mut files = BTreeMap::new();
    for entry in archive.entries().map_err(format)? {
        let mut entry = entry.map_err(format)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(format)?;
        let path = path.to_string_lossy();
        let path = path.strip_prefix("./").unwrap_or(&path).to_string();
        if !keep(&path) {
            continue;
        }
        let size = entry.size();
        budget.take(size)?;
        let mut bytes = Vec::new();
        (&mut entry)
            .take(size)
            .read_to_end(&mut bytes)
            .map_err(|e| Fault::Format(format!("{path}: {e}")))?;
        files.insert(path, bytes);
    }
    Ok(files)
}

#[cfg(test)]
use crate::model::pack_type::PackType;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../tests/common/download.rs"]
mod written;

#[cfg(test)]
mod tests;
