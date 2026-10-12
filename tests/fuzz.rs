//! Fuzzing: random and hostile input through every entry point that reads a ref, a file, a
//! directory, or an answer from Palette.
//!
//! One property holds everywhere: strata answers, or it errors with a kind and exit `2`. It never
//! panics, and it never leaves a thread unwound. The generator is deterministic, so a failure
//! repeats: each test prints its seed, and `STRATA_FUZZ_SEED=<n> cargo test fuzz::` runs that
//! seed alone. The default seed is fixed, so CI runs the same cases every time.

use crate::common;

use std::process::Command;

use serde_json::{Value, json};
use strata::api::{MemoryApi, Palette};
use strata::ops;
use strata::progress::Progress;
use strata::reference::Ref;
use strata::snapshot::Snapshot;

use common::{
    CLUSTER_LISTING, PROFILE_LISTING, PROJECT_LISTING, REGISTRY_LISTING, UID_B,
    cluster_manifests_route, cluster_ref, cluster_route, cluster_variables_route, listing,
    manifests_route, profile_route, strata_bin, text, variables_route,
};

/// xorshift64*: a deterministic source of cases, so a failure has a seed that reproduces it.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }

    fn chance(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

/// The seed this run fuzzes from.
fn seed() -> u64 {
    std::env::var("STRATA_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5747_4154_4152_3031)
}

/// Pieces that have broken parsers before: delimiters strata's refs use, JSON and YAML punctuation,
/// control characters, text that changes length when it is lowercased, and text that is not UTF-8
/// friendly.
const PIECES: &[&str] = &[
    "",
    "a",
    "0",
    "@",
    ":",
    "/",
    "\\",
    "..",
    ".",
    "-",
    "_",
    "*",
    "?",
    "[",
    "]",
    "{",
    "}",
    "\"",
    "'",
    "\n",
    "\r\n",
    "\t",
    " ",
    "\0",
    "%s",
    "%d",
    "{{",
    "}}",
    "---",
    "- ",
    ": ",
    "#",
    "!",
    "&",
    "null",
    "true",
    "~",
    "1.0.0",
    "latest",
    "cluster:",
    "pack:",
    "aws-stack",
    ".json",
    "İ",
    "ß",
    "ẞ",
    "é",
    "日本語",
    "🙂",
    "\u{feff}",
    "\u{202e}",
    "../../etc/passwd",
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
];

/// A string built from the hostile pieces.
fn junk_string(rng: &mut Rng) -> String {
    let n = rng.below(6);
    (0..n).map(|_| *rng.pick(PIECES)).collect()
}

/// A JSON value that is sometimes shaped like a Palette answer and sometimes not: the keys strata
/// points at, at the depths it points at them, with values of the wrong type as often as the right
/// one.
fn junk_json(rng: &mut Rng, depth: usize) -> Value {
    const KEYS: &[&str] = &[
        "metadata",
        "spec",
        "status",
        "items",
        "packs",
        "name",
        "uid",
        "version",
        "layer",
        "tag",
        "values",
        "manifests",
        "published",
        "draft",
        "variables",
        "digest",
        "packUid",
        "registryUid",
        "cloudType",
        "type",
        "inUseClusterUids",
        "inUseClusters",
        "clusterProfileTemplates",
        "listmeta",
        "count",
        "annotations",
        "projectUid",
        "state",
        "profileVersion",
        "packSummary",
        "packRef",
    ];
    if depth == 0 || rng.chance(3) {
        return match rng.below(7) {
            0 => Value::Null,
            1 => json!(rng.chance(2)),
            2 => json!(rng.next() as i64),
            3 => json!(f64::from_bits(rng.next()).is_finite()),
            4 => Value::String(junk_string(rng)),
            5 => json!([]),
            _ => json!({}),
        };
    }
    if rng.chance(2) {
        let n = rng.below(4);
        return Value::Array((0..n).map(|_| junk_json(rng, depth - 1)).collect());
    }
    let n = rng.below(6);
    let mut o = serde_json::Map::new();
    for _ in 0..n {
        let key = if rng.chance(4) {
            junk_string(rng)
        } else {
            rng.pick(KEYS).to_string()
        };
        o.insert(key, junk_json(rng, depth - 1));
    }
    Value::Object(o)
}

/// An error is an answer: it carries a kind and exits 2. A panic is not.
fn is_answer<T>(r: &strata::Result<T>) -> bool {
    match r {
        Ok(_) => true,
        Err(e) => {
            let json = e.to_json();
            !e.kind().is_empty() && e.exit_code() == 2 && json.get("kind").is_some()
        }
    }
}

#[test]
fn ref_parse_answers_for_any_string() {
    let mut rng = Rng::new(seed());
    for case in 0..20_000 {
        let text = junk_string(&mut rng);
        // parse_with decides the file question itself, so no case touches the filesystem
        let r = Ref::parse_with(&text, |_| false);
        // whatever it decided, saying it again must not lose the ref
        let said = r.to_string();
        assert!(
            !said.is_empty() || text.is_empty(),
            "seed {} case {case}: {text:?} printed as nothing",
            seed()
        );
    }
}

#[test]
fn a_record_of_junk_is_refused_and_never_read_as_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("record.json");
    let mut rng = Rng::new(seed().wrapping_add(1));
    for case in 0..5_000 {
        let text = match rng.below(3) {
            0 => junk_string(&mut rng),
            1 => junk_json(&mut rng, 4).to_string(),
            _ => {
                // a document that says it is a snapshot, with junk under it
                let mut v = junk_json(&mut rng, 4);
                if let Some(o) = v.as_object_mut() {
                    o.insert("schema".into(), json!("strata-snapshot/1"));
                }
                v.to_string()
            }
        };
        std::fs::write(&path, &text).unwrap();
        assert!(
            is_answer(&Snapshot::load(&path)),
            "seed {} case {case}: {text:?}",
            seed()
        );
    }
}

#[test]
fn a_path_that_is_not_a_file_is_an_answer() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no/such/record.json");
    assert!(is_answer(&Snapshot::load(&missing)), "a missing path");
    assert!(
        is_answer(&Snapshot::load(dir.path())),
        "a directory where a file is named"
    );
    let empty = dir.path().join("empty.json");
    std::fs::write(&empty, "").unwrap();
    assert!(is_answer(&Snapshot::load(&empty)), "an empty file");
    let nul = dir.path().join("nul.json");
    std::fs::write(&nul, [0u8, 159, 146, 150]).unwrap();
    assert!(is_answer(&Snapshot::load(&nul)), "bytes that are not UTF-8");
}

/// Every path strata asks Palette for, answered with the same junk document.
fn api_of_junk(rng: &mut Rng, uid: &str) -> MemoryApi {
    let mut api = MemoryApi::new();
    let paths = [
        PROFILE_LISTING.to_string(),
        CLUSTER_LISTING.to_string(),
        PROJECT_LISTING.to_string(),
        REGISTRY_LISTING.to_string(),
        profile_route(uid),
        manifests_route(uid),
        variables_route(uid),
        cluster_route(uid),
        cluster_manifests_route(uid),
        cluster_variables_route(uid),
    ];
    for p in paths {
        api = api.with(p, junk_json(rng, 5));
    }
    api
}

/// Reading a profile is total: whatever Palette answers, strata points at the fields it knows and
/// defaults the rest, so a junk answer still gives a record. That is the tolerance the tool is
/// built on, so what this fuzzes is the rest of the promise: the record serializes, loads again,
/// and keeps its root.
#[test]
fn a_profile_of_junk_gives_a_record_that_round_trips() {
    let uid = UID_B;
    let mut rng = Rng::new(seed().wrapping_add(2));
    let mut reached = 0;
    for case in 0..2_000 {
        let api = api_of_junk(&mut rng, uid);
        let snap = ops::snapshot_for(
            &Ref::Uid(uid.to_string()),
            || Ok(Palette::new(&api)),
            "t0",
            &Progress::silent(),
        );
        assert!(is_answer(&snap), "seed {} case {case}", seed());
        if snap.is_ok() {
            reached += 1;
        }
        // a snapshot strata built must be one it can read again, and compare with itself
        if let Ok(s) = snap {
            let same = ops::compare(&s, &s, "t1");
            assert!(
                same.same,
                "seed {} case {case}: a record differs from itself",
                seed()
            );
            let text = s.to_json(false).expect("a snapshot serializes");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("record.json");
            std::fs::write(&path, &text).unwrap();
            let back = Snapshot::load(&path).expect("a snapshot strata wrote loads again");
            assert_eq!(
                back.root,
                s.root,
                "seed {} case {case}: root changed",
                seed()
            );
        }
    }
    println!("built a snapshot in {reached} of 2000 cases");
    assert!(
        reached > 50,
        "seed {}: only {reached} of 2000 cases reached a snapshot; the generator stopped \
         reaching the code it is meant to fuzz",
        seed()
    );
}

#[test]
fn a_cluster_of_junk_snapshots_or_errors() {
    let uid = "64f1000000000000000000e1";
    let mut rng = Rng::new(seed().wrapping_add(3));
    for case in 0..2_000 {
        let api = api_of_junk(&mut rng, uid);
        let r = ops::snapshot_for(
            &cluster_ref(uid, None, false),
            || Ok(Palette::new(&api)),
            "t0",
            &Progress::silent(),
        );
        assert!(is_answer(&r), "seed {} case {case}", seed());
    }
}

#[test]
fn listing_and_consumers_of_junk_answer() {
    let uid = UID_B;
    let mut rng = Rng::new(seed().wrapping_add(4));
    for case in 0..1_000 {
        let api = api_of_junk(&mut rng, uid);
        let listed = listing(
            &api,
            &ops::ListOptions {
                applied: true,
                ..ops::ListOptions::default()
            },
        );
        assert!(is_answer(&listed), "seed {} case {case}: list", seed());

        let consumed = ops::consumers(
            &Ref::Uid(uid.to_string()),
            |_| Ok(Palette::new(&api)),
            "t1",
            &Progress::silent(),
        );
        assert!(
            is_answer(&consumed),
            "seed {} case {case}: consumers",
            seed()
        );
    }
}

#[test]
fn a_diff_of_two_junk_records_answers() {
    let uid = UID_B;
    let mut rng = Rng::new(seed().wrapping_add(5));
    let mut made: Vec<Snapshot> = Vec::new();
    while made.len() < 60 {
        let api = api_of_junk(&mut rng, uid);
        if let Ok(s) = ops::snapshot_for(
            &Ref::Uid(uid.to_string()),
            || Ok(Palette::new(&api)),
            "t0",
            &Progress::silent(),
        ) {
            made.push(s);
        }
    }
    for (i, a) in made.iter().enumerate() {
        for b in &made {
            let d = ops::compare(a, b, "t1");
            assert_eq!(
                d.same,
                a.root == b.root,
                "seed {} pair {i}: same must mean equal roots",
                seed()
            );
            // every renderer walks the whole document
            for format in [
                strata::render::Format::Terminal,
                strata::render::Format::Markdown,
                strata::render::Format::Json,
            ] {
                let out = strata::render::renderer(format, strata::color::Paint::off(), false)
                    .render(&d, a, b, true);
                assert!(out.is_ok(), "seed {} pair {i}: {format:?} render", seed());
            }
        }
    }
}

/// The binary itself, so the argument parsing, the file handling and the exit codes are fuzzed too.
/// A Rust panic exits 101 and prints `panicked at`; neither may ever happen.
#[test]
fn the_binary_never_panics_on_junk_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("junk.json");
    let mut rng = Rng::new(seed().wrapping_add(6));
    let commands = [
        "compare",
        "snapshot",
        "verify",
        "same-release",
        "consumers",
        "list",
        "packs",
        "candidate",
        "schema",
    ];
    let flags = [
        "--json",
        "--compact",
        "-q",
        "--no-color",
        "--full",
        "--applied",
        "--packs",
        "--project",
        "--cluster",
        "--pack",
        "-o",
        "--from-raw",
        "--from-candidate",
        "--raw-dir",
    ];
    for case in 0..500 {
        std::fs::write(&file, junk_json(&mut rng, 4).to_string()).unwrap();
        let mut args: Vec<String> = Vec::new();
        if rng.chance(2) {
            args.push(rng.pick(&commands).to_string());
        }
        for _ in 0..rng.below(4) {
            match rng.below(4) {
                0 => args.push(rng.pick(&flags).to_string()),
                // an argument cannot carry a nul byte: the OS refuses it before the binary
                // reads it, so that case belongs to the library tests above
                1 => args.push(junk_string(&mut rng).replace('\0', "")),
                2 => args.push(file.display().to_string()),
                _ => args.push(dir.path().display().to_string()),
            }
        }
        let out = Command::new(strata_bin())
            .args(&args)
            // an address nothing listens on: a request must fail as an error, not a panic
            .env("PALETTE_API", "http://127.0.0.1:1")
            .env("PALETTE_API_KEY", "fuzz")
            // the backoff has its own tests, with the wait injected; here it would only make
            // every case that reaches the network sleep
            .env("STRATA_RETRIES", "0")
            .env("STRATA_QUIET", "1")
            .env("NO_COLOR", "1")
            .output()
            .expect("the binary runs");
        let err = text(&out.stderr);
        assert!(
            !err.contains("panicked at"),
            "seed {} case {case}: {args:?} panicked:\n{err}",
            seed()
        );
        let code = out.status.code().unwrap_or(-1);
        assert!(
            (0..=2).contains(&code),
            "seed {} case {case}: {args:?} exited {code}\n{err}",
            seed()
        );
    }
}
