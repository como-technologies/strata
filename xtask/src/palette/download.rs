//! The download of a profile version, `GET /v1/clusterprofiles/{uid}/spc/download`, as Palette
//! serves it: a tar archive that gzip compresses, with the archive of each pack.
//!
//! The server builds the file at each request from the documents of the tenant. So an edit in
//! place changes the download too, as in Palette. The archive of a pack holds `pack.json`, with the
//! name, the version, and the layer of the pin. It also holds `values.yaml`, the default values of
//! the pack version, when the pack listing of its registry gives them. A copy of a version has the
//! values of the version that it copies. So two builds of one content differ only by the version
//! in `pack.json`.
//!
//! The download holds no archive of a pack of a private pack registry or Helm registry, or of a
//! manifest pack. It holds none of a pack of an OCI registry of Helm charts or of Zarf packages.
//! The list of the packs names such a pack with an empty `filePath`, and a manifest pack only by
//! its manifests. A pack of an OCI registry of packs has an archive, also when the registry is
//! private, and its top folder has the name and the version of the pack.
//!
//! A request that accepts JSON alone gets `{"Reader":{}}`, as for the export: see `super::reader`.
//! Each other request gets the file.
//!
//! The server does not model the limit of Palette, one download of a version in 10 seconds. With
//! the limit, a step of the book waits for the step before it. A live check shows the limit.

use std::path::Path;

use serde_json::Value;
use strata::closed_set::ClosedSet;
use strata::model::oci_provider::OciProvider;
use strata::model::pack_type::PackType;

use super::{Request, file_for};
use crate::tenant::{REGISTRY_LISTING, downloaded_profile, manifests_route, pack_listing};

#[allow(dead_code)]
#[path = "../../../tests/common/download.rs"]
mod written;

use written::{DownloadPack, archive_top, pack_archive, profile_download};

/// The media type of the file of a download. The stand-in models Palette, and not the client that it
/// tests, so it keeps its own copy of the type.
pub(super) const FILE_TYPE: &str = "application/octet-stream";

/// True when `target` asks for the download of a profile version.
pub(super) fn is_download(target: &str) -> bool {
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    downloaded_profile(path.trim_start_matches('/')).is_some()
}

/// The status and the body of the answer to `request`, when it asks for the download of a profile
/// version that the tenant holds. `None` for another request, and for a version that the tenant
/// does not hold: the server then answers 404. The server answers a request that accepts JSON
/// before it calls this function.
pub(super) fn download(root: &Path, request: &Request) -> Option<(u16, Vec<u8>)> {
    let path = request
        .target
        .split_once('?')
        .map_or(request.target.as_str(), |(path, _)| path);
    let profile_path = downloaded_profile(path.trim_start_matches('/'))?;
    let read = |route: &str| -> Option<Value> {
        let file = file_for(root, request.project.as_deref(), route)?;
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    };
    let profile = read(profile_path)?;
    let uid = profile["metadata"]["uid"].as_str().unwrap_or_default();
    let name = profile["metadata"]["name"].as_str().unwrap_or_default();
    let manifests = read(&manifests_route(uid)).unwrap_or_default();
    let private: Vec<Value> = read(REGISTRY_LISTING)
        .and_then(|listing| listing["items"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r["isPrivate"] == true)
        .map(|r| r["uid"].clone())
        .collect();
    let pins = profile["spec"]["published"]["packs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let built: Vec<Built> = pins
        .iter()
        .filter_map(|pin| Built::of(pin, &manifests, &private, &read))
        .collect();
    let listed: Vec<Vec<(&str, &str)>> = built
        .iter()
        .map(|b| {
            b.manifests
                .iter()
                .map(|(name, content)| (name.as_str(), content.as_str()))
                .collect()
        })
        .collect();
    let packs: Vec<DownloadPack> = built
        .iter()
        .zip(&listed)
        .map(|(b, manifests)| b.pack(manifests))
        .collect();
    Some((200, profile_download(uid, name, &packs)))
}

/// One pack of the download, with the texts that its [`DownloadPack`] borrows.
struct Built {
    name: String,
    version: String,
    kind: PackType,
    private: bool,
    archive: Option<Vec<u8>>,
    manifests: Vec<(String, String)>,
}

impl Built {
    /// The pack of the pin `pin` of the published template. `manifests` is the packs/manifests
    /// document of the version, `private` holds the uid of each private registry, and `read` reads
    /// a document of the tenant. None for a pin of a type that is no pack type of Palette.
    fn of(
        pin: &Value,
        manifests: &Value,
        private: &[Value],
        read: &dyn Fn(&str) -> Option<Value>,
    ) -> Option<Built> {
        let text = |field: &str| pin[field].as_str().unwrap_or_default().to_string();
        let (name, version) = (text("name"), text("version"));
        let kind = PackType::parse(&text("type"))?;
        // Palette gives the archive of a pack of an OCI registry of packs also when the registry
        // is private, and the list marks that pack as not private
        let is_private = kind != PackType::Oci && private.contains(&pin["registryUid"]);
        // an OCI registry of Helm charts or of Zarf packages serves no archive in a download
        let provider = pin["annotations"]["ociRegistryProviderType"]
            .as_str()
            .and_then(OciProvider::parse);
        let no_archive_provider = matches!(provider, Some(OciProvider::Helm | OciProvider::Zarf));
        let has_archive = kind != PackType::Manifest
            && !is_private
            && !(kind == PackType::Oci && no_archive_provider);
        let archive = has_archive.then(|| {
            let listing = read(&pack_listing(&name));
            let values = listing
                .as_ref()
                .and_then(|l| l["items"].as_array())
                .into_iter()
                .flatten()
                .find(|item| item["metadata"]["uid"] == pin["packUid"])
                .and_then(|item| item["spec"]["values"].as_str());
            let files: Vec<(&str, &str)> = values.map(|v| ("values.yaml", v)).into_iter().collect();
            let top = archive_top(kind, &name, &version, &text("digest"));
            pack_archive(&name, &version, &text("layer"), &top, &files)
        });
        let manifests = manifests["spec"]["packs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["metadata"]["name"] == pin["name"])
            .flat_map(|p| {
                p["spec"]["manifests"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|m| {
                let field = |f: &str| m[f].as_str().unwrap_or_default().to_string();
                (field("name"), field("content"))
            })
            .collect();
        Some(Built {
            name,
            version,
            kind,
            private: is_private,
            archive,
            manifests,
        })
    }

    /// The pack of the download, borrowed from this one, with the name and the content of each of
    /// its `manifests`.
    fn pack<'a>(&'a self, manifests: &'a [(&'a str, &'a str)]) -> DownloadPack<'a> {
        DownloadPack {
            name: &self.name,
            version: &self.version,
            kind: self.kind,
            private: self.private,
            archive: self.archive.clone(),
            manifests,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use strata::api::{MemoryApi, Palette};

    use super::*;
    use crate::palette::served;
    use crate::tenant::download_route;

    /// The download of the version `u1` of the project `p1`, as the stand-in serves it to a
    /// request that accepts `accept`.
    fn answer(dir: &Path, uid: &str, accept: Option<&str>) -> (u16, Vec<u8>) {
        let request = Request {
            target: format!("/{}", download_route(uid)),
            project: Some("p1".to_string()),
            accept: accept.map(str::to_string),
            body: Vec::new(),
        };
        served(dir, &request).unwrap()
    }

    /// The top folders of the archive `path` in the download `file`.
    fn tops(file: &[u8], path: &str) -> Vec<String> {
        use std::io::Read;
        let mut download = tar::Archive::new(flate2::read::GzDecoder::new(file));
        let mut archive = Vec::new();
        for entry in download.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap().to_str() == Some(path) {
                entry.read_to_end(&mut archive).unwrap();
            }
        }
        let mut tops: Vec<String> = tar::Archive::new(flate2::read::GzDecoder::new(&archive[..]))
            .entries()
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path().unwrap().into_owned();
                path.iter().next().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        tops.dedup();
        tops
    }

    /// The stand-in serves a gzip tar at the route, and strata reads it. The pack of the Public
    /// Repo has its archive, with the default values of its listing. The pack of a private
    /// registry and the manifest pack have none. The pack of a private OCI registry of packs has
    /// an archive, whose top folder has the name and the version of the pack. A request that
    /// accepts JSON alone gets `{"Reader":{}}`, and not the file.
    #[test]
    fn the_download_route_serves_a_file_that_strata_reads() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("p1");
        let write = |route: &str, doc: Value| {
            let file = dir.join(format!("{route}.json"));
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, doc.to_string()).unwrap();
        };
        let pin = |name: &str, kind: &str, registry: &str, uid: &str| {
            json!({"name": name, "type": kind, "layer": "k8s", "version": "1.36.2", "tag": "1.36.2",
                "registryUid": registry, "packUid": uid, "digest": "sha256:0123456789abcdef"})
        };
        write(
            "v1/clusterprofiles/u1",
            json!({"metadata": {"uid": "u1", "name": "aws-stack"},
                "spec": {"published": {"packs": [
                    pin("kubernetes", "spectro", "reg-public", "pk-1"),
                    pin("private-pack", "spectro", "reg-private", "pk-2"),
                    pin("rules", "manifest", "", "pk-3"),
                    pin("argocd", "oci", "reg-private", "pk-4")]}}}),
        );
        write(
            "v1/clusterprofiles/u1/packs/manifests",
            json!({"spec": {"packs": [{"metadata": {"name": "rules"},
                "spec": {"manifests": [{"name": "extra", "content": "kind: ConfigMap\n"}]}}]}}),
        );
        write(
            REGISTRY_LISTING,
            json!({"items": [{"uid": "reg-public", "isPrivate": false},
                {"uid": "reg-private", "isPrivate": true}]}),
        );
        write(
            &pack_listing("kubernetes"),
            json!({"items": [{"metadata": {"uid": "pk-1"},
                "spec": {"version": "1.36.2", "values": "pack:\n  k8sHardening: true\n"}}]}),
        );
        let (status, file) = answer(temp.path(), "u1", Some(FILE_TYPE));
        assert_eq!(status, 200);
        let api = MemoryApi::new().with_file(format!("/{}", download_route("u1")), file.clone());
        let download = Palette::new(&api).profile_download("u1").unwrap();
        let k8s = download.pack("kubernetes").unwrap();
        assert_eq!(k8s.version, "1.36.2");
        let paths: Vec<&str> = k8s
            .archive
            .as_ref()
            .unwrap()
            .files()
            .map(|(path, _)| path)
            .collect();
        assert_eq!(paths, ["pack.json", "values.yaml"]);
        assert_eq!(download.pack("private-pack").unwrap().archive, None);
        assert!(download.pack("rules").is_none());
        assert!(download.pack("argocd").unwrap().archive.is_some());
        assert_eq!(
            tops(&file, "u1/profiles/u1/packs/argocd/argocd-1.36.2.tar.gz"),
            ["argocd-1.36.2"]
        );
        assert_eq!(
            tops(
                &file,
                "u1/profiles/u1/packs/kubernetes/kubernetes-1.36.2.tar.gz"
            ),
            ["kubernetes-0123456789ab"]
        );
        // a request that accepts JSON alone gets `{"Reader":{}}`, as for the export, and each
        // other request gets the file. A version that the tenant does not hold has no download
        assert_eq!(
            answer(temp.path(), "u1", Some("application/json")),
            (200, br#"{"Reader":{}}"#.to_vec())
        );
        let served_file = answer(temp.path(), "u1", Some(FILE_TYPE));
        assert_eq!(answer(temp.path(), "u1", Some("*/*")), served_file);
        assert_eq!(answer(temp.path(), "u1", None), served_file);
        assert_eq!(answer(temp.path(), "u2", Some(FILE_TYPE)).0, 404);
        assert_eq!(answer(temp.path(), "u2", Some("application/json")).0, 404);
        assert!(is_download(&format!("/{}", download_route("u1"))));
        assert!(!is_download("/v1/clusterprofiles/u1/export"));
    }
}
