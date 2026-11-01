//! The parts that the targets share: the check of an answer, the documents of a profile, a
//! Palette that answers each request with one document, and the files of one input.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use strata::Result;
use strata::api::{PaletteApi, RawProfile};
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

/// The value of an answer of strata. An error is an answer when it has a kind, a JSON form with
/// that kind, and the exit code `2`: the person can read it. A panic is not an answer.
#[track_caller]
pub(crate) fn answer<T>(result: Result<T>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            assert!(!e.kind().is_empty(), "an error has a kind: {e}");
            assert_eq!(e.exit_code(), 2, "an error exits with 2: {e}");
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

/// The most requests that one document may cause. A read of a listing sends one request for each
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

/// The FNV-1a hash of `bytes`: a fixed rule that does not change with the Rust version, for the
/// name of a corpus file and for the choice of the fields of a mask.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}
