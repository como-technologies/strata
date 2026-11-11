//! The download of a profile version, as Palette writes it:
//! `GET /v1/clusterprofiles/{uid}/spc/download`.
//!
//! The crate reads the layout with consts that are not public. So this file writes the layout
//! again, in one place, as Palette writes it. The tests include it from `tests/common/mod.rs`. The
//! unit tests of `src/api/download.rs`, the targets of `fuzz/`, and the stand-in in `xtask/`
//! include it with `#[path]`. So no second writer of the layout exists.
//!
//! A download is a tar archive that gzip compresses. Each path starts with the uid of the profile:
//!
//! ```text
//! <uid>/metadata.json
//! <uid>/spc.json
//! <uid>/profiles/<uid>/packs-metadata.json
//! <uid>/profiles/<uid>/packs/<name>/<name>-<version>.tar.gz
//! <uid>/profiles/<uid>/packs/<name>/manifests/<manifest>.yaml
//! ```
//!
//! `metadata.json` names the list of the packs, and the list names the archive of each pack. A
//! pack archive is a tar archive that gzip compresses, with one top folder. For a pack of a pack
//! registry, the folder has the name of the pack and the first 12 hex digits of its digest. For a
//! pack of an OCI registry of packs, it has the name and the version of the pack.

use serde_json::{Value, json};

// The file that includes this one names `PackType` of the crate. A test of the crate names it as
// `crate::model::pack_type::PackType`, and each other package as `strata::model::...`.
use super::PackType;

/// One pack of a download.
pub struct DownloadPack<'a> {
    /// The pack name.
    pub name: &'a str,
    /// The version of the pack.
    pub version: &'a str,
    /// The type of the pack. A manifest pack has an entry for each of its manifests alone.
    pub kind: PackType,
    /// True for a pack of a private registry.
    pub private: bool,
    /// The bytes of the archive of the pack, from [`pack_archive`]. None when the download holds
    /// no archive of the pack: its list entry has an empty `filePath`.
    pub archive: Option<Vec<u8>>,
    /// The name and the content of each manifest of the pack.
    pub manifests: &'a [(&'a str, &'a str)],
}

/// The bytes of a tar archive that gzip compresses, with each of `files`: its path and its bytes.
/// Each entry has the mode `0644` and the time `0`, so one list of files gives one archive.
pub fn tgz(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let gz = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        tar.append_data(&mut header, path, bytes.as_slice())
            .expect("a tar entry writes to memory");
    }
    tar.into_inner()
        .and_then(|gz| gz.finish())
        .expect("a tar archive writes to memory")
}

/// The archive of the build of the pack `name` at `version`, as a pack registry serves it. The
/// top folder is `top`, such as `kubernetes-0123456789ab`. `pack.json` holds the name, the version,
/// and the layer `layer`, and each of `files` follows it: its path under the top folder and its
/// content.
pub fn pack_archive(
    name: &str,
    version: &str,
    layer: &str,
    top: &str,
    files: &[(&str, &str)],
) -> Vec<u8> {
    let pack = json!({"name": name, "version": version, "layer": layer});
    let mut entries = vec![(format!("{top}/pack.json"), pack.to_string().into_bytes())];
    entries.extend(
        files
            .iter()
            .map(|(path, content)| (format!("{top}/{path}"), content.as_bytes().to_vec())),
    );
    tgz(&entries)
}

/// The path of the archive of `pack` from the folder of the list of the packs, as Palette names it.
fn archive_path(pack: &DownloadPack) -> String {
    let file = format!("{}-{}", pack.name, pack.version);
    let file = if file.contains("--") {
        format!("pack-{file}")
    } else {
        file
    };
    let end = if pack.kind == PackType::Helm {
        "tgz"
    } else {
        "tar.gz"
    };
    format!("packs/{}/{file}.{end}", pack.name)
}

/// The list of the packs of a download, `packs-metadata.json`: an entry for each pack with a
/// registry, and one for each manifest of a pack. A manifest pack has the entries of its manifests
/// alone.
fn packs_metadata(packs: &[DownloadPack]) -> Value {
    let mut list = serde_json::Map::new();
    for pack in packs {
        if pack.kind != PackType::Manifest {
            let path = pack
                .archive
                .as_ref()
                .map_or(String::new(), |_| archive_path(pack));
            list.insert(
                pack.name.to_string(),
                json!({"name": pack.name, "version": pack.version, "type": pack.kind,
                    "isRegistryPrivate": pack.private, "filePath": path, "manifestDigest": ""}),
            );
        }
        for (manifest, _) in pack.manifests {
            list.insert(
                format!("{}-{manifest}", pack.name),
                json!({"name": pack.name, "manifestName": manifest, "type": PackType::Manifest,
                    "isRegistryPrivate": false,
                    "filePath": format!("packs/{}/manifests/{manifest}.yaml", pack.name),
                    "manifestDigest": "sha256:0000000000000000"}),
            );
        }
    }
    json!({"packs": list})
}

/// The download of the profile version `uid` with the name `name`, which holds `packs`. The file
/// `spc.json` holds the profile, as Palette writes it with the value of each macro: here only its
/// name and its uid.
pub fn profile_download(uid: &str, name: &str, packs: &[DownloadPack]) -> Vec<u8> {
    let list = format!("profiles/{uid}/packs-metadata.json");
    let metadata = json!({"hubbleInfo": {"apiEndpoint": "api.example.com"},
        "resource": {"uid": uid, "name": name, "kind": "clusterprofile"},
        "profiles": [{"name": name, "uid": uid, "cloudType": "aws", "version": 1,
            "type": "cluster", "packsMetaFilePath": list}]});
    let spc = json!({"metadata": {"name": name, "uid": uid}});
    let mut files = vec![
        (
            format!("{uid}/metadata.json"),
            metadata.to_string().into_bytes(),
        ),
        (format!("{uid}/spc.json"), spc.to_string().into_bytes()),
        (
            format!("{uid}/{list}"),
            packs_metadata(packs).to_string().into_bytes(),
        ),
    ];
    for pack in packs {
        if let Some(archive) = &pack.archive {
            files.push((
                format!("{uid}/profiles/{uid}/{}", archive_path(pack)),
                archive.clone(),
            ));
        }
        for (manifest, content) in pack.manifests {
            files.push((
                format!(
                    "{uid}/profiles/{uid}/packs/{}/manifests/{manifest}.yaml",
                    pack.name
                ),
                content.as_bytes().to_vec(),
            ));
        }
    }
    tgz(&files)
}

/// The top folder of the archive of the pack `name` of the type `kind`, at `version` and with the
/// digest `digest`, as Palette names it. For a pack of a pack registry, it holds the name and the
/// first 12 hex digits of the digest. A pack with no digest gets 12 zeros. For a pack of an OCI
/// registry of packs, it holds the name and the version.
pub fn archive_top(kind: PackType, name: &str, version: &str, digest: &str) -> String {
    if kind == PackType::Oci {
        return format!("{name}-{version}");
    }
    let hex = digest.strip_prefix("sha256:").unwrap_or(digest);
    let short: String = hex.chars().chain(std::iter::repeat('0')).take(12).collect();
    format!("{name}-{short}")
}
