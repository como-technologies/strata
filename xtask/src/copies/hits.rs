//! The rule of the literals. It finds each value of a closed set or of a const in the literals of
//! the added lines.

use std::collections::{BTreeMap, BTreeSet};

use super::place::word_char;
use super::source::{Source, Tree};
use super::{ByValue, Hit, Holder, Place, shown};

/// The least count of characters of a value with a mark, a capital, or a digit, such as `--pin`.
/// A shorter value is in too many texts.
const SHORTEST: usize = 3;

/// The least count of letters of a plain value, such as `ok`.
const SHORTEST_WORD: usize = 2;

/// The lines that the change adds to a file, by its path.
pub type Added<'a> = dyn Fn(&str) -> &'a BTreeSet<usize> + 'a;

/// The places of the literals that the rule of the literals found: their path and their line.
pub type Reported = BTreeSet<(String, usize)>;

/// True for a value of plain words of small letters, such as `drift` or `no answer`. A text of
/// the book or of a page holds such words, so they are a copy only alone: see [`held`].
fn is_plain(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_lowercase() || c == ' ')
}

/// True for a value that the check reads. A plain value needs [`SHORTEST_WORD`] letters or more.
/// Another value needs [`SHORTEST`] characters or more, with a letter or a digit.
pub fn counts(value: &str) -> bool {
    if is_plain(value) {
        value.len() >= SHORTEST_WORD
    } else {
        value.chars().count() >= SHORTEST && value.chars().any(char::is_alphanumeric)
    }
}

/// True when `value` stands in `text` as a whole: no character of a word touches an end of
/// `value` that is itself such a character. So `read-error` is in `the read-error names`, and
/// not in `read-errors`.
fn holds_word(text: &str, value: &str) -> bool {
    let first = value.chars().next().is_some_and(word_char);
    let last = value.chars().last().is_some_and(word_char);
    text.match_indices(value).any(|(at, _)| {
        let before = text[..at].chars().last();
        let after = text[at + value.len()..].chars().next();
        (!first || !before.is_some_and(word_char)) && (!last || !after.is_some_and(word_char))
    })
}

/// The values of `values` that the literal `text` holds. A value with a mark, a capital, or a
/// digit counts at any place of the literal. A plain value counts when the literal holds it alone,
/// with no more than spaces at its ends, such as `"newer "`. A plain word also counts beside one
/// word that is a value and no plain word, such as `"drift DriftFound "`. So a phrase of plain
/// words, such as `"newer version"`, is no copy.
pub fn held<'a>(text: &str, values: &ByValue<'a>) -> BTreeSet<&'a str> {
    let alone = text.trim();
    let mut out: BTreeSet<&'a str> = values
        .keys()
        .filter(|value| match is_plain(value) {
            true => alone == **value,
            false => holds_word(text, value),
        })
        .copied()
        .collect();
    let words: Vec<&str> = text.split_whitespace().collect();
    let pair = words.len() == 2 && words.iter().any(|w| !is_plain(w));
    if pair && words.iter().all(|w| values.contains_key(*w)) {
        out.extend(
            words
                .iter()
                .filter_map(|w| values.get_key_value(*w))
                .map(|(value, _)| *value)
                .filter(|value| is_plain(value)),
        );
    }
    out
}

/// The holders of each value that `file` reaches: in its test code when `test` is true, and in
/// its other code when it is false.
fn reached<'a>(tree: &'a Tree, file: &Source, test: bool) -> ByValue<'a> {
    let mut out: ByValue<'a> = BTreeMap::new();
    for holder in &tree.holders {
        if holder.lang == file.lang && counts(&holder.value) && holder.reach.holds(&file.path, test)
        {
            out.entry(holder.value.as_str()).or_default().push(holder);
        }
    }
    out
}

/// The names and the places of `holders`. Two lines of one file can declare one name, such as the
/// serde name of a variant and the text of its table. Then the name shows its first line.
pub fn places(holders: &[&Holder]) -> String {
    let mut first: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for holder in holders {
        let line = first
            .entry((&holder.name, &holder.path))
            .or_insert(holder.line);
        *line = (*line).min(holder.line);
    }
    let names: Vec<String> = first
        .into_iter()
        .map(|((name, path), line)| format!("{name} ({path}:{line})"))
        .collect();
    names.join(", ")
}

/// True when a line of the literal from `line` to `last` is in `added`.
pub fn touches(added: &BTreeSet<usize>, line: usize, last: usize) -> bool {
    added.range(line..=last).next().is_some()
}

/// The values of a closed set or of a const that the literals of the added lines hold. It gives
/// the findings, the places that are no findings, and the places of the findings.
pub fn literal_hits(tree: &Tree, added: &Added) -> (Vec<Hit>, Vec<Hit>, Reported) {
    let (mut findings, mut skipped, mut reported) = (Vec::new(), Vec::new(), Reported::new());
    for file in &tree.files {
        let lines = added(&file.path);
        if lines.is_empty() {
            continue;
        }
        let (in_tests, in_code) = (reached(tree, file, true), reached(tree, file, false));
        let declared: BTreeSet<(usize, &str)> = tree
            .holders
            .iter()
            .filter(|h| h.path == file.path)
            .map(|h| (h.line, h.value.as_str()))
            .collect();
        for literal in &file.literals {
            if !touches(lines, literal.line, literal.last)
                || declared.contains(&(literal.line, literal.value.as_str()))
            {
                continue;
            }
            let values = if file.tests.holds(literal.line) {
                &in_tests
            } else {
                &in_code
            };
            for value in held(&literal.value, values) {
                let hit = Hit {
                    path: file.path.clone(),
                    line: literal.line,
                    place: Place::Literal(literal.class.clone()),
                    what: shown(&literal.value),
                    copies: format!("{value} is the value of {}", places(&values[value])),
                };
                if literal.class.is_finding() {
                    reported.insert((file.path.clone(), literal.line));
                    findings.push(hit);
                } else {
                    skipped.push(hit);
                }
            }
        }
    }
    (findings, skipped, reported)
}
