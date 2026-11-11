//! The files of the corpus: the write of the seeds and of the regression cases, and the checks of
//! the files.
//!
//! `just fuzz-seed` writes each seed that the corpus does not hold. `just docs-check` fails when
//! the corpus does not hold a seed that the writer gives. It also fails for an output of the book
//! that is not current. So a change that makes the corpus stale fails in CI.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::support::fingerprint;

use super::{Case, SEED_LIMIT, TARGETS, Target, regressions};

/// The directory of the targets in this package, which bolero reads.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/__fuzz__")
}

/// The directory under `root` for the target `test` and the kind of input `kind`. The kind is
/// `corpus` for the inputs that a fuzzer grows from, and `crashes` for a regression case.
fn directory(root: &Path, test: &str, kind: &str) -> PathBuf {
    root.join(test.replace("::", "__")).join(kind)
}

/// The inputs of the target `test` in `kind`.
fn inputs(test: &str, kind: &str) -> Vec<Vec<u8>> {
    fs::read_dir(directory(&root(), test, kind))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| fs::read(entry.path()).expect("an input reads"))
        .collect()
}

/// Files that the writer gives: the path of each file, and its bytes. Two seeds with the same
/// bytes are one file.
type Files = BTreeMap<PathBuf, Vec<u8>>;

/// Each file that the writer gives under `root` and that `root` does not hold with the same bytes.
///
/// The writer gives each seed of `targets` in `corpus/`, with the name that [`fingerprint`] gives
/// its bytes, and each case of `cases` in `crashes/`. A seed of more than four times
/// [`SEED_LIMIT`] bytes is not kept. A file of `root` that the writer does not give is not
/// examined, so a file that a fuzzer added stays.
fn stale(root: &Path, targets: &[Target], cases: Vec<Case>) -> Files {
    let mut given = Files::new();
    for target in targets {
        let dir = directory(root, target.test, "corpus");
        for bytes in (target.seeds)() {
            if bytes.len() <= SEED_LIMIT * 4 {
                given.insert(dir.join(format!("{:016x}", fingerprint(&bytes))), bytes);
            }
        }
    }
    for (test, name, bytes) in cases {
        given.insert(directory(root, test, "crashes").join(name), bytes);
    }
    given.retain(|path, bytes| fs::read(path).ok().as_ref() != Some(bytes));
    given
}

/// Write each file of `files`, and make its directory.
fn write(files: &Files) {
    for (path, bytes) in files {
        let dir = path
            .parent()
            .expect("a file of the corpus is in a directory");
        fs::create_dir_all(dir).expect("the directory of the corpus is made");
        fs::write(path, bytes).expect("a seed is written");
    }
}

/// Write the seeds and the regression cases that the corpus does not hold.
#[test]
#[ignore = "writes the files of the corpus: run it with `just fuzz-seed`"]
fn write_the_seed_corpus() {
    write(&stale(&root(), &TARGETS, regressions::cases()));
}

/// The corpus holds each seed and each regression case that `just fuzz-seed` writes. A change that
/// makes a seed stale fails here, with the path of each file to write. A change of a schema, of a
/// case, of a file of `examples/`, or of an output of the book can make a seed stale.
#[test]
#[ignore = "reads the files that `just examples` builds: run it with `just docs-check`"]
fn the_corpus_holds_each_seed_that_the_writer_gives() {
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stale: Vec<String> = stale(&root(), &TARGETS, regressions::cases())
        .keys()
        .map(|path| {
            let path = path.strip_prefix(package).unwrap_or(path);
            format!("  fuzz/{}", path.display())
        })
        .collect();
    assert!(
        stale.is_empty(),
        "the corpus does not hold each seed that `just fuzz-seed` writes. Run the recipe, and \
         commit these files:\n{}",
        stale.join("\n")
    );
}

/// The check of the corpus names a seed that the corpus does not hold, and a seed that has other
/// bytes. A write of the stale files makes the corpus current, and a file of a fuzzer stays.
#[test]
fn the_check_of_the_corpus_names_each_stale_seed_and_the_write_makes_the_corpus_current() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let target = TARGETS
        .iter()
        .find(|t| t.test.starts_with("reference::the_text_of_a_ref"))
        .expect("the target of a ref");
    let targets = std::slice::from_ref(target);
    let case = || vec![(target.test, "a-case", b"pack:@1".to_vec())];
    let seeds: BTreeSet<Vec<u8>> = (target.seeds)().into_iter().collect();

    let empty = stale(root, targets, case());
    assert_eq!(
        empty.len(),
        seeds.len() + 1,
        "an empty corpus holds no seed"
    );
    write(&empty);
    assert!(
        stale(root, targets, case()).is_empty(),
        "the write makes the corpus current"
    );

    let fuzzer = directory(root, target.test, "corpus").join("a-file-of-a-fuzzer");
    fs::write(&fuzzer, b"tag:").expect("a file of a fuzzer");
    let mut paths = empty.keys();
    let changed = paths.next().expect("a seed");
    fs::write(changed, b"other bytes").expect("a seed changes");
    let removed = paths.next().expect("a second seed");
    fs::remove_file(removed).expect("a seed goes");
    let found = stale(root, targets, case());
    assert_eq!(found.keys().collect::<Vec<_>>(), [changed, removed]);

    write(&found);
    assert!(stale(root, targets, case()).is_empty());
    assert!(
        fuzzer.is_file(),
        "a file that the writer does not give stays"
    );
}

/// The seeds of a target reach the code that it fuzzes, and the regression cases pass. A corpus
/// that a schema change made useless, or a directory that bolero does not read, fails here.
#[test]
fn the_seeds_of_each_target_reach_its_code_and_the_regression_cases_pass() {
    for target in &TARGETS {
        let corpus = inputs(target.test, "corpus");
        assert!(
            !corpus.is_empty(),
            "{} has no seed: run `just fuzz-seed`",
            target.test
        );
        let reached = corpus.iter().filter(|bytes| (target.run)(bytes)).count();
        assert!(
            reached * 3 >= corpus.len(),
            "only {reached} of {} seeds of {} reach its code",
            corpus.len(),
            target.test
        );
        for case in inputs(target.test, "crashes") {
            (target.run)(&case);
        }
    }
}
