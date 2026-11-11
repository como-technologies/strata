//! The download of a profile version: the archive of each pack of its published content.
//!
//! `GET /v1/clusterprofiles/{uid}/spc/download` gives a tar archive that gzip compresses. All its
//! paths start with one folder. In that folder, `metadata.json` names the list of the packs in
//! `profiles[].packsMetaFilePath`. The list maps the name of each pack to its `version`, its
//! `type`, and its `filePath`. `filePath` is the path of the archive of the pack from the folder of
//! the list.
//! The archive of a pack is a tar archive that gzip compresses, with one top folder. An empty
//! `filePath` means that the download holds no archive of the pack.
//!
//! Palette resolves the macros in the other files of the download, so they can hold the value of
//! a macro. Strata reads the two lists and the archives of the packs, and no other file. It keeps
//! a digest of each file of an archive, and not the file.

use std::collections::BTreeMap;
use std::io::Read;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Method;
use crate::content::canon::canonical_json;
use crate::error::{Error, Result};
use crate::json::{arr, object_at, opt_str, str_at};

/// The file at the top of a download that names the list of the packs of each profile.
const METADATA: &str = "metadata.json";

/// The name of the file of a list of the packs. The list of a profile is in the folder of that
/// profile.
const PACKS_METADATA: &str = "packs-metadata.json";

/// The file of a pack archive that names the pack and its version.
const PACK_FILE: &str = "pack.json";

/// The field of [`PACK_FILE`] that names the version. The build of a pack under another version
/// changes only this field: strata does not compare it.
const PACK_VERSION: &str = "version";

/// The ends of the name of an archive in a download: a pack archive and a Helm chart.
const ARCHIVE_ENDS: [&str; 2] = [".tar.gz", ".tgz"];

/// The most bytes that strata reads from the files of one download, after the gzip, with the files
/// of each pack archive in it. A larger download is an error, so a small file cannot fill the
/// memory.
const UNPACKED_LIMIT: u64 = 1024 * 1024 * 1024;

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
    /// The archive of the pack. None when the download holds no archive of the pack. Palette serves
    /// none for a pack of a private registry, or of an OCI registry of Helm charts or of Zarf
    /// packages.
    pub archive: Option<PackArchive>,
}

/// The files of the archive of one pack: the path of each file under its top folder, with a
/// digest of the file.
///
/// The digest of `pack.json` covers its JSON with no `version`. So two builds of one content under
/// two versions have the same files.
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
        read_download(file).map_err(|message| Error::NotArchive {
            method: Method::Get,
            path: path.to_string(),
            message,
        })
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
    fn take(&mut self, size: u64) -> std::result::Result<(), String> {
        self.0 = self.0.checked_sub(size).ok_or_else(|| {
            format!(
                "the files hold more than {} MiB",
                UNPACKED_LIMIT / 1024 / 1024
            )
        })?;
        Ok(())
    }
}

/// Read the download `file`: the two lists, and the archive of each pack.
///
/// The steps: unpack the lists and the archives, and no other file. Find the one `metadata.json`
/// at the top. Read the list of the packs that it names. Then read each archive that the list
/// names, from the folder of the list.
fn read_download(file: &[u8]) -> std::result::Result<ProfileDownload, String> {
    let mut budget = Budget(UNPACKED_LIMIT);
    let files = unpack(file, &mut budget, |path| {
        file_name(path) == METADATA
            || file_name(path) == PACKS_METADATA
            || ARCHIVE_ENDS.iter().any(|end| path.ends_with(end))
    })?;
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
        let archive = match str_at(entry, "/filePath") {
            "" => None,
            path => {
                let at = format!("{folder}/{path}");
                let Some(bytes) = files.get(&at) else {
                    return Err(format!(
                        "{list} names {path}, which the download does not hold"
                    ));
                };
                Some(pack_archive(bytes, &mut budget).map_err(|e| format!("{path}: {e}"))?)
            }
        };
        let version = str_at(entry, "/version").to_string();
        packs.insert(name.clone(), DownloadedPack { version, archive });
    }
    Ok(ProfileDownload { packs })
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
/// covers its JSON with no `version`, when the file is a JSON object.
fn pack_archive(gz: &[u8], budget: &mut Budget) -> std::result::Result<PackArchive, String> {
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
            let digest = if path == PACK_FILE {
                pack_file_digest(&bytes)
            } else {
                sha256(&bytes)
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
            fields.remove(PACK_VERSION);
            sha256(canonical_json(&Value::Object(fields)).as_bytes())
        }
        _ => sha256(bytes),
    }
}

/// `sha256:` and the hex digest of `bytes`.
fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
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
) -> std::result::Result<BTreeMap<String, Vec<u8>>, String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
    let mut files = BTreeMap::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|e| e.to_string())?;
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
            .map_err(|e| format!("{path}: {e}"))?;
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
    use super::written::{DownloadPack, pack_archive, profile_download, tgz};
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
        let (a, b) = (a.archive.as_ref().unwrap(), b.archive.as_ref().unwrap());
        // the top folder names the digest, so the paths are those under it
        let paths: Vec<&str> = a.files().map(|(path, _)| path).collect();
        assert_eq!(paths, ["README.md", "pack.json", "values.yaml"]);
        assert!(a.changed_files(b).is_empty(), "{a:?} {b:?}");
        assert_eq!(a, b);
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
            &a.pack("kubernetes").unwrap().archive,
            &b.pack("kubernetes").unwrap().archive,
        );
        let (a, b) = (a.as_ref().unwrap(), b.as_ref().unwrap());
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
        let a = a.pack("csi-aws-ebs").unwrap().archive.as_ref().unwrap();
        let b = b.pack("csi-aws-ebs").unwrap().archive.as_ref().unwrap();
        assert_eq!(a.changed_files(b), ["pack.json"]);
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
            assert_eq!(download.pack(name).unwrap().archive, None, "{name}");
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
        let k8s = download
            .pack("kubernetes")
            .unwrap()
            .archive
            .as_ref()
            .unwrap();
        assert_eq!(k8s.files().count(), 1);
    }

    #[test]
    fn a_file_that_is_not_a_download_is_refused_with_what_strata_could_not_read() {
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
        let mut good = read(&[built(
            "kubernetes",
            "1.36.2",
            "kubernetes-0123456789ab",
            &[],
        )]);
        good.packs.clear();
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

    #[test]
    fn an_archive_of_a_pack_that_is_not_gzip_is_refused_and_names_its_path() {
        let download = profile_download(
            UID,
            "aws-stack",
            &[DownloadPack {
                archive: Some(br#"{"code":"NotFound"}"#.to_vec()),
                ..built("kubernetes", "1.36.2", "kubernetes-0123456789ab", &[])
            }],
        );
        let message = refusal(&download);
        assert!(
            message.starts_with("packs/kubernetes/kubernetes-1.36.2.tar.gz: "),
            "{message}"
        );
    }

    #[test]
    fn the_budget_refuses_more_bytes_than_it_has() {
        let mut budget = Budget(10);
        assert!(budget.take(6).is_ok());
        assert_eq!(
            budget.take(5),
            Err("the files hold more than 1024 MiB".to_string())
        );
    }

    fn json_bytes(v: &serde_json::Value) -> Vec<u8> {
        v.to_string().into_bytes()
    }
}
