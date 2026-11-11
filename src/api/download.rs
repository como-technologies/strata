//! The download of a profile version: the archive of each pack of its published content.
//!
//! `GET /v1/clusterprofiles/{uid}/spc/download` gives a tar archive that gzip compresses. All its
//! paths start with one folder. In that folder, `metadata.json` names the list of the packs in
//! `profiles[].packsMetaFilePath`. The list maps the name of each pack to its `version`, its
//! `type`, and its `filePath`. `filePath` is the path of the archive of the pack from the folder of
//! the list.
//! The archive of a pack is a tar archive that gzip compresses, with one top folder. An empty
//! `filePath` means that the download holds no archive of the pack. The file of a pack can also be
//! another file, such as an error answer of the host of a Helm chart: see [`PackFile::NotReadable`].
//! Strata keeps each other pack of such a download.
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
use crate::text::plural;

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

/// The most bytes that strata reads from the files of one download, after the gzip, with the files
/// of each pack archive in it. A larger download is an error, so a small file cannot fill the
/// memory.
const UNPACKED_LIMIT: u64 = 1024 * 1024 * 1024;

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
    /// The file of the pack is not a tar archive that gzip compresses: the reason.
    ///
    /// The reason gives the size of the file. When the file is text, it gives the start of the
    /// text on one line, such as `263 bytes of text: <?xml ...`. Else it gives the error of the
    /// read. Palette puts the answer of the host of a Helm chart in the download in place of the
    /// chart, also an error answer, and the route answers HTTP 200.
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

/// The bytes that strata can still read from the files of one download: see [`UNPACKED_LIMIT`].
struct Budget(u64);

impl Budget {
    /// Take `size` bytes, or fail when fewer are left.
    fn take(&mut self, size: u64) -> std::result::Result<(), Fault> {
        self.0 = self.0.checked_sub(size).ok_or(Fault::Limit)?;
        Ok(())
    }
}

/// Why strata cannot read a tar archive that gzip compresses.
#[derive(Debug, PartialEq, Eq)]
enum Fault {
    /// The files of the download hold more bytes than [`UNPACKED_LIMIT`]. Strata refuses the whole
    /// download, also for the file of one pack.
    Limit,
    /// The bytes are not a tar archive that gzip compresses: the error of the read.
    Format(String),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fault::Limit => write!(
                f,
                "the files hold more than {} MiB",
                UNPACKED_LIMIT / 1024 / 1024
            ),
            Fault::Format(error) => f.write_str(error),
        }
    }
}

/// Read the download `file`: the two lists, and the archive of each pack. Strata reads `limit`
/// bytes at most from its files: see [`UNPACKED_LIMIT`].
///
/// The steps: unpack the lists and the archives, and no other file. Find the one `metadata.json`
/// at the top. Read the list of the packs that it names. Then read each archive that the list
/// names, from the folder of the list. A file of a pack that is not an archive gets its reason,
/// and the other packs stay.
fn read_download(file: &[u8], limit: u64) -> std::result::Result<ProfileDownload, String> {
    let mut budget = Budget(limit);
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
            path => {
                let at = format!("{folder}/{path}");
                let Some(bytes) = files.get(&at) else {
                    return Err(format!(
                        "{list} names {path}, which the download does not hold"
                    ));
                };
                pack_file(bytes, &mut budget).map_err(|fault| format!("{path}: {fault}"))?
            }
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
        Err(Fault::Limit) => Err(Fault::Limit),
    }
}

/// The reason of the file `bytes` of a pack, whose read gave `error`: its size, and the start of
/// its text when it is text, else `error`.
///
/// The file is text when it is UTF-8 with no control character other than white space. An example
/// is `263 bytes of text: <?xml version="1.0" ...`. See [`one_line`] for the form of the text.
fn not_readable(bytes: &[u8], error: &str) -> String {
    let size = format!("{} {}", bytes.len(), plural(bytes.len(), "byte", "bytes"));
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

/// The start of `text` on one line, as a reason shows it.
///
/// Each run of white space becomes one space, and strata drops each other control character. The
/// line holds [`REASON_TEXT`] characters at most, and `...` follows a cut. So the answer of the
/// host of a chart moves no cursor in a terminal.
fn one_line(text: &str) -> String {
    let mut chars = text
        .split_whitespace()
        .flat_map(|word| std::iter::once(' ').chain(word.chars()))
        .filter(|c| !c.is_control())
        .skip(1);
    let start: String = chars.by_ref().take(REASON_TEXT).collect();
    match chars.next() {
        Some(_) => format!("{start}..."),
        None => start,
    }
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
mod tests {
    use super::written::{
        DownloadPack, archive_top, chart_archive, chart_file, pack_archive, profile_download, tgz,
    };
    use super::*;
    use crate::api::download_path;

    const UID: &str = "64f0000000000000000007a1";

    /// A pack of a pack registry, with the archive of its build at `version` under `top`, and
    /// `files` beside `pack.json`.
    fn built<'a>(
        name: &'a str,
        version: &'a str,
        top: &str,
        files: &[(&str, &str)],
    ) -> DownloadPack<'a> {
        DownloadPack {
            name,
            version,
            kind: PackType::Spectro,
            private: false,
            archive: Some(pack_archive(name, version, "k8s", top, files)),
            manifests: &[],
        }
    }

    /// The download of `packs`, read.
    fn read(packs: &[DownloadPack]) -> ProfileDownload {
        ProfileDownload::read(
            &download_path(UID),
            &profile_download(UID, "aws-stack", packs),
        )
        .unwrap()
    }

    /// The message of the error of the read of `file`.
    fn refusal(file: &[u8]) -> String {
        match ProfileDownload::read(&download_path(UID), file) {
            Err(Error::NotArchive { path, message, .. }) => {
                assert_eq!(path, download_path(UID));
                message
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_builds_of_one_content_differ_only_by_the_version_in_pack_json() {
        let values = [
            ("values.yaml", "pack:\n  k8sHardening: true\n"),
            ("README.md", "# k8s\n"),
        ];
        let a = read(&[built(
            "kubernetes",
            "1.36.2-rc.1",
            "kubernetes-0123456789ab",
            &values,
        )]);
        let b = read(&[built(
            "kubernetes",
            "1.36.2",
            "kubernetes-ba9876543210",
            &values,
        )]);
        let (a, b) = (a.pack("kubernetes").unwrap(), b.pack("kubernetes").unwrap());
        assert_eq!(
            (a.version.as_str(), b.version.as_str()),
            ("1.36.2-rc.1", "1.36.2")
        );
        let (a, b) = (a.archive().unwrap(), b.archive().unwrap());
        // the top folder differs between the two builds, so the paths are those under it
        let paths: Vec<&str> = a.files().map(|(path, _)| path).collect();
        assert_eq!(paths, ["README.md", "pack.json", "values.yaml"]);
        assert!(a.changed_files(b).is_empty(), "{a:?} {b:?}");
        assert_eq!(a, b);
        // an OCI registry of packs names the top folder by the name and the version of the pack
        let oci = |version: &str| {
            let top = archive_top(PackType::Oci, "kubernetes", version, "");
            let download = read(&[built("kubernetes", version, &top, &values)]);
            download
                .pack("kubernetes")
                .unwrap()
                .archive()
                .cloned()
                .unwrap()
        };
        let (c, d) = (oci("1.36.2-rc.1"), oci("1.36.2"));
        assert!(c.changed_files(&d).is_empty(), "{c:?} {d:?}");
        assert_eq!(&c, a);
    }

    #[test]
    fn each_file_that_differs_in_two_builds_is_a_changed_file() {
        let a = read(&[built(
            "kubernetes",
            "1.36.2-rc.1",
            "kubernetes-0123456789ab",
            &[("values.yaml", "a: 1\n"), ("charts/k8s.tgz", "one")],
        )]);
        let b = read(&[built(
            "kubernetes",
            "1.36.2",
            "kubernetes-ba9876543210",
            &[("values.yaml", "a: 2\n"), ("presets.yaml", "b: 1\n")],
        )]);
        let (a, b) = (
            a.pack("kubernetes").unwrap().archive(),
            b.pack("kubernetes").unwrap().archive(),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_eq!(
            a.changed_files(b),
            ["charts/k8s.tgz", "presets.yaml", "values.yaml"]
        );
        assert_eq!(b.changed_files(a), a.changed_files(b));
    }

    #[test]
    fn pack_json_with_another_field_is_a_changed_file() {
        let pack = |layer: &str| {
            let archive = pack_archive(
                "csi-aws-ebs",
                "1.65.0",
                layer,
                "csi-aws-ebs-0123456789ab",
                &[],
            );
            read(&[DownloadPack {
                archive: Some(archive),
                ..built("csi-aws-ebs", "1.65.0", "csi-aws-ebs-0123456789ab", &[])
            }])
        };
        let (a, b) = (pack("csi"), pack("storage"));
        let a = a.pack("csi-aws-ebs").unwrap().archive().unwrap();
        let b = b.pack("csi-aws-ebs").unwrap().archive().unwrap();
        assert_eq!(a.changed_files(b), ["pack.json"]);
    }

    /// The archive of the chart `nginx` of a Helm registry at `version`, read from a download. Its
    /// files are `files`, under the top folder `nginx`.
    fn chart(version: &str, files: &[(&str, &str)]) -> PackArchive {
        let download = read(&[DownloadPack {
            name: "nginx",
            version,
            kind: PackType::Helm,
            private: false,
            archive: Some(chart_archive("nginx", files)),
            manifests: &[],
        }]);
        download.pack("nginx").unwrap().archive().cloned().unwrap()
    }

    #[test]
    fn two_builds_of_one_chart_differ_only_by_the_line_version_of_chart_yaml() {
        let values = ("values.yaml", "replicaCount: 1\n");
        let file = |version: &str, app: Option<&str>| chart_file("nginx", version, app);
        let a = chart(
            "15.0.0-rc.1",
            &[("Chart.yaml", &file("15.0.0-rc.1", Some("1.27.0"))), values],
        );
        let b = chart(
            "15.0.0",
            &[("Chart.yaml", &file("15.0.0", Some("1.27.0"))), values],
        );
        let paths: Vec<&str> = a.files().map(|(path, _)| path).collect();
        assert_eq!(paths, ["Chart.yaml", "values.yaml"]);
        assert!(a.changed_files(&b).is_empty(), "{a:?} {b:?}");
        assert_eq!(a, b);
        // each other line of Chart.yaml counts: the version of the app, a comment, and the version
        // of a dependency, which has an indent
        let top = file("15.0.0", Some("1.27.0"));
        let dependency =
            |version: &str| format!("{top}dependencies:\n- name: common\n  version: {version}\n");
        for (one, other) in [
            (top.clone(), file("15.0.0", Some("1.28.0"))),
            (top.clone(), file("15.0.0", None)),
            (top.clone(), format!("{top}# the chart of the web tier\n")),
            (dependency("2.0.0"), dependency("2.1.0")),
        ] {
            let one = chart("15.0.0", &[("Chart.yaml", &one), values]);
            let other = chart("15.0.0", &[("Chart.yaml", &other), values]);
            assert_eq!(one.changed_files(&other), ["Chart.yaml"], "{other:?}");
        }
        // the Chart.yaml of a chart in the chart keeps its version
        let sub = |version: &str| {
            let sub = chart_file("common", version, None);
            chart(
                "15.0.0",
                &[("Chart.yaml", &top), ("charts/common/Chart.yaml", &sub)],
            )
        };
        assert_eq!(
            sub("2.0.0").changed_files(&sub("2.1.0")),
            ["charts/common/Chart.yaml"]
        );
    }

    #[test]
    fn a_pack_with_no_archive_in_the_download_is_listed_with_none_and_a_manifest_pack_is_not_listed()
     {
        let none = |name: &'static str, kind: PackType, private: bool| DownloadPack {
            name,
            version: "1.0.0",
            kind,
            private,
            archive: None,
            manifests: &[],
        };
        let download = read(&[
            none("private-pack", PackType::Spectro, true),
            none("private-chart", PackType::Helm, true),
            none("zarf-package", PackType::Oci, false),
            DownloadPack {
                manifests: &[("extra-rules", "kind: ConfigMap\n")],
                ..none("platform-rules", PackType::Manifest, false)
            },
            DownloadPack {
                manifests: &[("extra", "kind: ConfigMap\n")],
                ..built("kubernetes", "1.36.2", "kubernetes-0123456789ab", &[])
            },
        ]);
        for name in ["private-pack", "private-chart", "zarf-package"] {
            assert_eq!(
                download.pack(name).unwrap().file,
                PackFile::NoArchive,
                "{name}"
            );
        }
        assert!(download.pack("platform-rules").is_none());
        // a manifest of a pack with an archive is no file of the archive
        let names: Vec<&str> = download.packs().map(|(name, _)| name).collect();
        assert_eq!(
            names,
            [
                "kubernetes",
                "private-chart",
                "private-pack",
                "zarf-package"
            ]
        );
        let k8s = download.pack("kubernetes").unwrap().archive().unwrap();
        assert_eq!(k8s.files().count(), 1);
    }

    #[test]
    fn a_file_that_is_not_a_download_is_refused_with_what_strata_cannot_read() {
        assert!(!refusal(b"{\"Reader\":{}}").is_empty());
        let no_metadata = tgz(&[(format!("{UID}/spc.json"), b"{}".to_vec())]);
        assert_eq!(refusal(&no_metadata), "no metadata.json at the top");
        let bad = tgz(&[(format!("{UID}/metadata.json"), b"no json".to_vec())]);
        assert!(refusal(&bad).starts_with(&format!("{UID}/metadata.json is not JSON")));
        let no_list = tgz(&[(
            format!("{UID}/metadata.json"),
            br#"{"profiles":[]}"#.to_vec(),
        )]);
        assert_eq!(
            refusal(&no_list),
            "metadata.json names 0 lists of packs, and not one"
        );
        // the list names an archive that the download does not hold
        let list = format!("profiles/{UID}/packs-metadata.json");
        let missing = tgz(&[
            (
                format!("{UID}/metadata.json"),
                json_bytes(&serde_json::json!({"profiles": [{"packsMetaFilePath": list}]})),
            ),
            (
                format!("{UID}/{list}"),
                json_bytes(&serde_json::json!({"packs": {"kubernetes":
                {"name": "kubernetes", "version": "1.36.2", "type": PackType::Spectro,
                    "filePath": "packs/kubernetes/kubernetes-1.36.2.tar.gz"}}})),
            ),
        ]);
        assert_eq!(
            refusal(&missing),
            format!(
                "{list} names packs/kubernetes/kubernetes-1.36.2.tar.gz, which the download does not hold"
            )
        );
    }

    /// The chart `nginx` of a Helm registry whose file in the download is `file`.
    fn chart_file_of(file: &[u8]) -> DownloadPack<'static> {
        DownloadPack {
            name: "nginx",
            version: "25.1.5",
            kind: PackType::Helm,
            private: false,
            archive: Some(file.to_vec()),
            manifests: &[],
        }
    }

    /// The criterion of #828: the download holds the error answer of the host of a chart in place
    /// of the chart. Strata keeps the other packs with their archives, and gives the chart no
    /// archive and a reason.
    #[test]
    fn a_pack_file_that_is_not_an_archive_gets_a_reason_and_the_other_packs_keep_their_archives() {
        let download = read(&[
            built(
                "kubernetes",
                "1.36.2",
                "kubernetes-0123456789ab",
                &[("values.yaml", "a: 1\n")],
            ),
            chart_file_of(written::CHART_HOST_REFUSAL.as_bytes()),
        ]);
        let k8s = download.pack("kubernetes").unwrap().archive().unwrap();
        let paths: Vec<&str> = k8s.files().map(|(path, _)| path).collect();
        assert_eq!(paths, ["pack.json", "values.yaml"]);
        let nginx = download.pack("nginx").unwrap();
        assert_eq!(nginx.version, "25.1.5");
        assert_eq!(nginx.archive(), None);
        assert_eq!(
            nginx.file,
            PackFile::NotReadable(
                "263 bytes of text: <?xml version=\"1.0\" encoding=\"UTF-8\"?> \
                 <Error><Code>AccessDenied</Code><Message>Access Denied</Message><RequestId>4V6Q3Z..."
                    .to_string()
            )
        );
    }

    /// A file that is not text gets its size and the error of the read. A file with a control
    /// character other than white space is not text, so no escape of a terminal reaches the reason.
    #[test]
    fn a_pack_file_that_is_not_text_gets_its_size_and_the_error_of_the_read() {
        for file in [&b"\xff\xfe\x00\x01 not an archive"[..], b"\x1b[2J<Error/>"] {
            let download = read(&[chart_file_of(file)]);
            let PackFile::NotReadable(reason) = &download.pack("nginx").unwrap().file else {
                panic!("{download:?}");
            };
            let size = format!("{} bytes: ", file.len());
            assert!(reason.starts_with(&size), "{reason}");
            assert!(reason.len() > size.len(), "{reason}");
            assert!(!reason.chars().any(char::is_control), "{reason:?}");
        }
        let download = read(&[chart_file_of(b"x")]);
        let PackFile::NotReadable(reason) = &download.pack("nginx").unwrap().file else {
            panic!("{download:?}");
        };
        assert_eq!(reason, "1 byte of text: x");
    }

    #[test]
    fn the_text_of_a_reason_is_one_line_of_120_characters_at_most_with_no_control_character() {
        assert_eq!(one_line(" a\n\tb\u{1b} c\r\n"), "a b c");
        assert_eq!(one_line("\n \t"), "");
        // a cut falls between two characters, also in a character of more than one byte
        let long = "é".repeat(REASON_TEXT + 1);
        let line = one_line(&long);
        assert_eq!(line, format!("{}...", "é".repeat(REASON_TEXT)));
        assert_eq!(one_line(&"é".repeat(REASON_TEXT)), "é".repeat(REASON_TEXT));
    }

    /// The budget covers the files of each pack archive too. A pack archive that holds more bytes
    /// than the budget has left refuses the whole download, and gets no reason.
    #[test]
    fn a_pack_archive_over_the_budget_refuses_the_whole_download() {
        let large = "a".repeat(100_000);
        let file = profile_download(
            UID,
            "aws-stack",
            &[built(
                "kubernetes",
                "1.36.2",
                "kubernetes-0123456789ab",
                &[("values.yaml", &large)],
            )],
        );
        assert!(file.len() < 50_000, "the files compress well");
        let message = read_download(&file, 50_000).unwrap_err();
        assert_eq!(
            message,
            "packs/kubernetes/kubernetes-1.36.2.tar.gz: the files hold more than 1024 MiB"
        );
        assert!(read_download(&file, UNPACKED_LIMIT).is_ok());
    }

    #[test]
    fn the_budget_refuses_more_bytes_than_it_has() {
        let mut budget = Budget(10);
        assert!(budget.take(6).is_ok());
        assert_eq!(budget.take(5), Err(Fault::Limit));
        assert_eq!(
            Fault::Limit.to_string(),
            "the files hold more than 1024 MiB"
        );
    }

    fn json_bytes(v: &serde_json::Value) -> Vec<u8> {
        v.to_string().into_bytes()
    }
}
