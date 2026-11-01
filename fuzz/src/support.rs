//! The parts that the targets share: the check of an answer, the documents of a profile, a
//! Palette that answers each request with one document, and the files of one input.

use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use strata::Result;
use strata::api::{PaletteApi, RawCluster, RawProfile, ReadErrors};
use tempfile::TempDir;

/// The uid of the profile that the targets build.
pub(crate) const UID: &str = "64f0000000000000000000a1";

/// The time that the targets stamp on a record.
pub(crate) const AT: &str = "2026-09-17T09:30:00Z";

/// The route of the document of a profile version.
pub(crate) fn profile_route() -> String {
    format!("/v1/clusterprofiles/{UID}")
}

/// The route of the pack manifests of a profile version.
pub(crate) fn manifests_route() -> String {
    format!("/v1/clusterprofiles/{UID}/packs/manifests")
}

/// The route of the variables of a profile version.
pub(crate) fn variables_route() -> String {
    format!("/v1/clusterprofiles/{UID}/variables")
}

/// The route of the document of a pack.
pub(crate) fn pack_route(pack_uid: &str) -> String {
    format!("/v1/packs/{pack_uid}")
}

/// The text of `bytes`. A target that reads text leaves an input that is not UTF-8.
pub(crate) fn utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

/// An error that a target checks, of strata or of strata-preflight.
pub(crate) trait Refusal: Display {
    /// The kind of the error.
    fn kind(&self) -> &'static str;

    /// The JSON form of the error.
    fn to_json(&self) -> Value;

    /// The exit code of the error, or `None` when the program gives no code for it.
    fn exit_code(&self) -> Option<u8>;
}

impl Refusal for strata::Error {
    fn kind(&self) -> &'static str {
        strata::Error::kind(self)
    }

    fn to_json(&self) -> Value {
        strata::Error::to_json(self)
    }

    fn exit_code(&self) -> Option<u8> {
        Some(strata::Error::exit_code(self))
    }
}

impl Refusal for strata_preflight::Error {
    fn kind(&self) -> &'static str {
        strata_preflight::Error::kind(self)
    }

    fn to_json(&self) -> Value {
        strata_preflight::Error::to_json(self)
    }

    fn exit_code(&self) -> Option<u8> {
        None
    }
}

/// The value of an answer. An error is an answer when it has a kind, a JSON form with that kind,
/// and the exit code `2` when it has a code: the person can read it. A panic is not an answer.
#[track_caller]
pub(crate) fn answer<T, E: Refusal>(result: std::result::Result<T, E>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            assert!(!e.kind().is_empty(), "an error has a kind: {e}");
            if let Some(code) = e.exit_code() {
                assert_eq!(code, 2, "an error exits with 2: {e}");
            }
            assert_eq!(
                e.to_json()["kind"],
                e.kind(),
                "the JSON form names the kind: {e}"
            );
            None
        }
    }
}

/// The raw responses of a profile with one published pack. `values` is the values of the pack,
/// and `manifest` the content of its one manifest.
pub(crate) fn raw_profile(values: &str, manifest: &str) -> RawProfile {
    RawProfile {
        profile: json!({
            "metadata": {"uid": UID, "name": "demo"},
            "spec": {"version": "1.0.0", "published": {"cloudType": "aws", "type": "add-on", "packs": [{
                "name": "pack", "layer": "addon", "type": "spectro", "tag": "1.0.0",
                "version": "1.0.0", "packUid": "p1", "registryUid": "r1",
                "digest": "sha256:0123", "values": values,
            }]}},
        }),
        manifests: json!({"spec": {"packs": [{
            "metadata": {"name": "pack", "uid": "p1"},
            "spec": {"manifests": [{"name": "m", "uid": "m1", "digest": "d", "content": manifest}]},
        }]}}),
        variables: json!({"variables": []}),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        manifest_entities: None,
        resolved_values: None,
    }
}

/// The most requests that one document can cause. A read of a listing sends one request for each
/// page, and a read of a document of thousands of items sends a few for each item. A loop that
/// does not stop sends more: with this limit it is a panic that a fuzzer reports, and not a run
/// that never ends.
const REQUEST_LIMIT: usize = 200_000;

/// A Palette that answers each request, a GET or a POST, with one document. A target gives it a
/// document of any shape, and strata must read it as an answer or refuse it with a kind.
pub(crate) struct Everywhere {
    document: Value,
    asked: Cell<usize>,
}

impl Everywhere {
    /// A Palette that answers with `document`.
    pub(crate) fn new(document: Value) -> Everywhere {
        Everywhere {
            document,
            asked: Cell::new(0),
        }
    }

    /// The answer to one request.
    fn respond(&self) -> Result<Value> {
        self.asked.set(self.asked.get() + 1);
        assert!(
            self.asked.get() <= REQUEST_LIMIT,
            "strata sent more than {REQUEST_LIMIT} requests for one document"
        );
        Ok(self.document.clone())
    }
}

impl PaletteApi for Everywhere {
    fn get_json(&self, _path: &str) -> Result<Value> {
        self.respond()
    }

    fn post_json(&self, _path: &str, _body: &Value) -> Result<Value> {
        self.respond()
    }
}

thread_local! {
    /// The directory of the thread. A test of libtest runs in one thread, so two targets that run
    /// at once never share a directory.
    static SCRATCH: RefCell<Option<TempDir>> = const { RefCell::new(None) };
}

/// An empty directory for the files of one input. The directory is the same on each call in a
/// thread: a call empties it, and a target makes no directory for each of its thousands of inputs.
/// So a caller finishes with the files of one call before it makes the next call.
pub(crate) fn scratch() -> PathBuf {
    SCRATCH.with(|cell| {
        let mut dir = cell.borrow_mut();
        let dir = dir.get_or_insert_with(|| TempDir::new().expect("a temporary directory"));
        for entry in fs::read_dir(dir.path()).expect("the temporary directory reads") {
            let path = entry.expect("an entry of the temporary directory").path();
            if path.is_dir() {
                fs::remove_dir_all(path).expect("a directory of the last input goes");
            } else {
                fs::remove_file(path).expect("a file of the last input goes");
            }
        }
        dir.path().to_path_buf()
    })
}

/// Write `bytes` to the file `name` of `dir`.
pub(crate) fn write(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes).expect("a file of the temporary directory writes");
}

/// The parts of an input that holds several files: the bytes between NUL bytes, in order. A JSON
/// text or a YAML text has no NUL byte, so each part is a whole file.
pub(crate) fn parts(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|&b| b == 0)
}

/// The first eight bytes of the SHA-256 digest of `bytes`, as a number: a fixed rule that does not
/// change with the Rust version, for the name of a corpus file and for the choice of the fields of
/// a mask.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let digest = Sha256::digest(bytes);
    let head = <[u8; 8]>::try_from(&digest[..8]).expect("a SHA-256 digest has 32 bytes");
    u64::from_be_bytes(head)
}

/// The names of the files that `save_dir` writes into an empty directory, sorted.
fn saved_files(save: impl FnOnce(&Path)) -> Vec<String> {
    let dir = TempDir::new().expect("a temporary directory");
    save(dir.path());
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .expect("the directory of a raw directory reads")
        .map(|entry| {
            entry
                .expect("an entry reads")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The files of a raw directory of a profile, in the order of the parts of an input. They are the
/// files that `save_dir` writes for a profile that holds each document, so a file that a later
/// version of strata adds is a part of each input.
pub(crate) fn profile_files() -> &'static [String] {
    static FILES: OnceLock<Vec<String>> = OnceLock::new();
    FILES.get_or_init(|| {
        let full = RawProfile {
            manifest_entities: Some(json!([])),
            resolved_values: Some(json!([])),
            ..raw_profile("", "")
        };
        saved_files(|dir| full.save_dir(dir, AT).expect("a raw directory saves"))
    })
}

/// The files of a raw directory of a cluster, in the order of the parts of an input. They are the
/// files that `save_dir` writes for a cluster that holds each document and each read error.
pub(crate) fn cluster_files() -> &'static [String] {
    static FILES: OnceLock<Vec<String>> = OnceLock::new();
    FILES.get_or_init(|| {
        let why = Some("refused".to_string());
        let full = RawCluster {
            cluster: json!({}),
            manifests: json!([]),
            variables: json!([]),
            variables_unavailable: false,
            project_uid: None,
            fetched_at: None,
            notifications: Some(json!([])),
            repave: Some(json!({})),
            resolved_values: Some(json!([])),
            cloud_config: Some(json!({})),
            rbacs: Some(json!([])),
            namespaces: Some(json!([])),
            errors: ReadErrors {
                notifications: why.clone(),
                repave: why.clone(),
                rbacs: why.clone(),
                namespaces: why,
            },
        };
        saved_files(|dir| full.save_dir(dir, AT).expect("a raw directory saves"))
    })
}
