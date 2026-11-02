//! The property tests: five rules of strata that hold for any input, checked on documents that the
//! test builds from the bytes of bolero.
//!
//! Each test has a small decoder in place of a corpus. The decoder reads the bytes as choices, and
//! builds a document from them: a tree of YAML values, or a version. So any bytes give a document
//! that is valid, which a byte target seldom reaches, and the same bytes give the same document.
//! `just test` runs the tests with random bytes. `just fuzz` runs them under libFuzzer, which
//! finds the bytes that reach new code.

use serde_yaml::{Mapping, Value as Yaml};

use crate::canon::{canon_is_idempotent, record};
use crate::mask::{fields_of, mask_and_check};
use crate::record::{compares_equal, writes_and_reads_again};
use crate::reference::order_laws;

/// The bytes of an input, read as one choice after the other. A choice past the end of the bytes is
/// the first one, so a short input is still a document.
struct Choices<'a>(&'a [u8]);

impl Choices<'_> {
    /// A number below `n`, from the next byte.
    fn below(&mut self, n: usize) -> usize {
        let (&byte, rest) = self.0.split_first().unwrap_or((&0, &[]));
        self.0 = rest;
        usize::from(byte) % n.max(1)
    }

    /// One of `items`.
    fn pick<'a, T: ?Sized>(&mut self, items: &[&'a T]) -> &'a T {
        items[self.below(items.len())]
    }
}

/// The keys that a tree uses: the words that the mask and the canonical form treat apart, and the
/// words that are not like them.
const KEYS: [&str; 12] = [
    "a",
    "b",
    "c",
    "password",
    "adminPassword",
    "image",
    "replicas",
    "x.y",
    "é",
    "",
    "1",
    "null",
];

/// The text of a scalar of a tree.
const TEXTS: [&str; 12] = [
    "",
    "x",
    "s3cr3t",
    "1.0",
    "true",
    "~",
    "multi\nline",
    "{{ .spectro.var.db }}",
    "a: b",
    "'quoted'",
    "日本語 🙂",
    "  padded  ",
];

/// A tree of YAML values of at most `depth` levels: a mapping of unique keys, a sequence, or a
/// scalar.
fn tree(choices: &mut Choices, depth: usize) -> Yaml {
    match choices.below(if depth == 0 { 3 } else { 5 }) {
        0 => Yaml::String(choices.pick(&TEXTS).to_string()),
        1 => Yaml::Number(
            i64::try_from(choices.below(1000))
                .expect("a number below 1000 is an i64")
                .into(),
        ),
        2 => Yaml::Bool(choices.below(2) == 0),
        3 => Yaml::Sequence(
            (0..choices.below(4))
                .map(|_| tree(choices, depth - 1))
                .collect(),
        ),
        _ => {
            let mut map = Mapping::new();
            for _ in 0..choices.below(5) {
                let key = Yaml::String(choices.pick(&KEYS).to_string());
                let value = tree(choices, depth - 1);
                map.entry(key).or_insert(value);
            }
            Yaml::Mapping(map)
        }
    }
}

/// `tree` with the entries of each mapping in the opposite order.
fn reversed(tree: &Yaml) -> Yaml {
    match tree {
        Yaml::Mapping(map) => {
            let entries: Vec<_> = map.iter().collect();
            Yaml::Mapping(
                entries
                    .into_iter()
                    .rev()
                    .map(|(key, value)| (key.clone(), reversed(value)))
                    .collect(),
            )
        }
        Yaml::Sequence(items) => Yaml::Sequence(items.iter().map(reversed).collect()),
        other => other.clone(),
    }
}

/// The YAML text of `tree`.
fn yaml(tree: &Yaml) -> String {
    serde_yaml::to_string(tree).expect("a tree is written as YAML")
}

/// The YAML text that the choices of an input build.
fn values_of(bytes: &[u8]) -> String {
    yaml(&tree(&mut Choices(bytes), 3))
}

#[test]
fn the_root_does_not_change_when_the_order_of_the_keys_changes() {
    bolero::check!().for_each(|bytes| {
        let tree = tree(&mut Choices(bytes), 3);
        let (forward, backward) = (yaml(&tree), yaml(&reversed(&tree)));
        let root = |values: &str, manifest: &str| {
            record(values, manifest)
                .expect("a profile of one pack builds")
                .root
        };
        assert_eq!(
            root(&forward, ""),
            root(&backward, ""),
            "the values {forward:?} and {backward:?} are one document"
        );
        assert_eq!(root("", &forward), root("", &backward), "as a manifest");
    });
}

#[test]
fn the_comparison_of_two_records_of_one_profile_is_empty() {
    bolero::check!().for_each(|bytes| {
        let tree = tree(&mut Choices(bytes), 3);
        let (forward, backward) = (yaml(&tree), yaml(&reversed(&tree)));
        let build = |values: &str| record(values, values).expect("a profile of one pack builds");
        let first = build(&forward);
        // a second build of the same text, and a build of the text with its keys in another order
        compares_equal(&first, &build(&forward));
        compares_equal(&first, &build(&backward));
    });
}

#[test]
fn the_canonical_form_is_idempotent() {
    bolero::check!().for_each(|bytes| {
        assert!(canon_is_idempotent(&values_of(bytes)));
    });
}

#[test]
fn the_mask_is_idempotent_and_leaves_no_secret_in_clear() {
    bolero::check!().for_each(|bytes| {
        let values = values_of(bytes);
        mask_and_check(&values, &fields_of(&values));
    });
}

#[test]
fn the_version_order_is_a_total_preorder() {
    /// The pieces of a version: numbers with and without a zero in front, a letter, a separator,
    /// a word, and a digit that is not ASCII.
    const PIECES: [&str; 14] = [
        "0", "1", "2", "9", "10", "01", "a", "rc", ".", "-", "_", "LTS__", "é", "٣",
    ];
    bolero::check!().for_each(|bytes| {
        let mut choices = Choices(bytes);
        let mut version = || -> String {
            (0..choices.below(6))
                .map(|_| PIECES[choices.below(PIECES.len())])
                .collect()
        };
        let (a, b, c) = (version(), version(), version());
        order_laws(&a, &b, &c);
    });
}

#[test]
fn a_record_written_and_read_again_has_the_same_root() {
    bolero::check!().for_each(|bytes| {
        let values = values_of(bytes);
        let record = record(&values, &values).expect("a profile of one pack builds");
        writes_and_reads_again(&record);
    });
}
