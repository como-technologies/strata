//! The parts that the targets share are the check of an answer and the documents of a profile.
//! They also share a Palette that answers each request with one document or one error, and the
//! files of one input.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use strata::api::{Method, PaletteApi, RawCluster, RawProfile, ReadErrors};
use strata::{Error, Result};
use tempfile::TempDir;

use crate::answer::{Refusal, answer_fault};
use crate::routes::{
    PackFixture, UID_A, manifest, manifests_answer, no_variables, profile_document, template,
};

/// The uid of the profile that the targets build.
pub(crate) const UID: &str = UID_A;

/// The time that the targets stamp on a record.
pub(crate) const AT: &str = "2026-09-17T09:30:00Z";

/// The text of `bytes`. A target that reads text leaves an input that is not UTF-8.
pub(crate) fn utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
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

/// The value of an answer. An error is an answer when [`answer_fault`] finds no fault in it: the
/// person can read it. A panic is not an answer.
#[track_caller]
pub(crate) fn answer<T, E: Refusal>(result: std::result::Result<T, E>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(e) => {
            if let Some(fault) = answer_fault(&e) {
                panic!("{fault}");
            }
            None
        }
    }
}

/// The raw responses of a profile with one published pack. `values` is the values of the pack,
/// and `manifest_content` the content of its one manifest.
pub(crate) fn raw_profile(values: &str, manifest_content: &str) -> RawProfile {
    let pack = PackFixture::new("pack", "addon", "1.0.0", "p1", "r1", "sha256:0123", values);
    let manifests = vec![manifest("m", "m1", "d", manifest_content)];
    let published = template("aws", "add-on", json!([pack.pack_ref(&manifests)]));
    RawProfile {
        profile: profile_document(UID, "demo", "1.0.0", published, None),
        manifests: manifests_answer(vec![pack.pack_manifests(manifests)]),
        variables: no_variables(),
        variables_unavailable: false,
        project_uid: None,
        fetched_at: None,
        manifest_entities: None,
        resolved_values: None,
    }
}

/// The most requests that one document can cause. A read of a listing sends one request for each
/// page. A read of a document of thousands of items sends a few for each item. A loop that does
/// not stop sends more. With this limit, it is a panic that a fuzzer reports, and not a run that
/// never ends.
const REQUEST_LIMIT: usize = 200_000;

/// A Palette that answers each request, a GET or a POST, with one document, or with one HTTP
/// error. A target gives it a document of any shape, or a status and a body of any text. Strata
/// must read the answer or refuse it with a kind.
pub(crate) struct Everywhere {
    reply: Reply,
    asked: Cell<usize>,
}

/// What [`Everywhere`] answers to each request.
enum Reply {
    /// The document, with the status `200`.
    Document(Value),
    /// An HTTP error with this status and this body.
    Error { status: u16, body: String },
}

impl Everywhere {
    /// A Palette that answers with `document`.
    pub(crate) fn new(document: Value) -> Everywhere {
        Everywhere::replying(Reply::Document(document))
    }

    /// A Palette that answers with the HTTP error `status` and `body`.
    pub(crate) fn failing(status: u16, body: &str) -> Everywhere {
        Everywhere::replying(Reply::Error {
            status,
            body: body.to_string(),
        })
    }

    fn replying(reply: Reply) -> Everywhere {
        Everywhere {
            reply,
            asked: Cell::new(0),
        }
    }

    /// The answer to one request.
    fn respond(&self, method: Method, path: &str) -> Result<Value> {
        self.asked.set(self.asked.get() + 1);
        assert!(
            self.asked.get() <= REQUEST_LIMIT,
            "strata sent more than {REQUEST_LIMIT} requests for one document"
        );
        match &self.reply {
            Reply::Document(document) => Ok(document.clone()),
            Reply::Error { status, body } => Err(Error::Api {
                method,
                path: path.to_string(),
                status: *status,
                body: body.clone(),
                retry_after: None,
            }),
        }
    }
}

impl PaletteApi for Everywhere {
    fn get_json(&self, path: &str) -> Result<Value> {
        self.respond(Method::Get, path)
    }

    fn post_json(&self, path: &str, _body: &Value) -> Result<Value> {
        self.respond(Method::Post, path)
    }
}

thread_local! {
    /// The directory of the thread. A test of libtest runs in one thread, so two targets that run
    /// at once never share a directory.
    static SCRATCH: RefCell<Option<TempDir>> = const { RefCell::new(None) };
}

/// An empty directory for the files of one input. The directory is the same on each call in a
/// thread. A call empties it, and a target makes no directory for each of its thousands of
/// inputs. So a caller finishes with the files of one call before it makes the next call.
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

/// The first eight bytes of the SHA-256 digest of `bytes`, as a number. The number comes from a
/// fixed rule that does not change with the Rust version. It serves for the name of a corpus file
/// and for the choice of the fields of a mask.
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
/// files that `save_dir` writes for a profile that holds each document. So a file that a later
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
