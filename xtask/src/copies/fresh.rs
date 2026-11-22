//! The consts and the functions that a change adds with a value or a body that another line of
//! the same tree holds.
//!
//! A new declaration copies a value or a body at any place of its tree. It is a copy also when
//! neither of the two can name the other. Then the fix moves the value or the body to one place
//! that both can name.

use std::collections::{BTreeMap, BTreeSet};

use super::hits::{Added, Reported, counts, places};
use super::place::Class;
use super::source::{Source, Tree};
use super::{Func, Hit, Holder, Kind, Lang, Place, shown};

/// The least count of tokens of the body of a function that the rule of the helpers reads.
const SMALLEST_BODY: usize = 6;

/// The file of each path.
fn sources(tree: &Tree) -> BTreeMap<&str, &Source> {
    tree.files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect()
}

/// A literal of the tree that a new const can take the place of: its file, its line, and its
/// class.
type Copy<'a> = (&'a Source, usize, &'a Class);

/// The literals of each tree in a place of a finding, by their value. A literal that declares a
/// value, or that the rule of the literals found, is out.
fn copies<'a>(
    tree: &'a Tree,
    reported: &Reported,
) -> BTreeMap<(Lang, &'a str, &'a str), Vec<Copy<'a>>> {
    let declared: BTreeSet<(&str, usize, &str)> = tree
        .holders
        .iter()
        .map(|h| (h.path.as_str(), h.line, h.value.as_str()))
        .collect();
    let mut out: BTreeMap<_, Vec<Copy>> = BTreeMap::new();
    for file in &tree.files {
        for literal in file.literals.iter().filter(|l| l.class.is_finding()) {
            let place = (file.path.as_str(), literal.line, literal.value.as_str());
            if declared.contains(&place) || reported.contains(&(file.path.clone(), literal.line)) {
                continue;
            }
            let key = (file.lang, file.root.as_str(), literal.value.as_str());
            out.entry(key)
                .or_default()
                .push((file, literal.line, &literal.class));
        }
    }
    out
}

/// The consts that the change adds with a value that another declaration or another literal of
/// the same tree holds. A text of a closed set that is no const is out. Two values of two closed
/// sets can have one text, such as two sets that both hold `newer`.
pub fn new_consts(tree: &Tree, added: &Added, reported: &Reported) -> Vec<Hit> {
    let sources = sources(tree);
    let copies = copies(tree, reported);
    let mut out = Vec::new();
    for holder in &tree.holders {
        let Some(&file) = sources.get(holder.path.as_str()) else {
            continue;
        };
        if matches!(holder.kind, Kind::Text(_))
            || !added(&holder.path).contains(&holder.line)
            || !counts(&holder.value)
        {
            continue;
        }
        let same: Vec<&Holder> = tree
            .holders
            .iter()
            .filter(|other| {
                let Some(&other_file) = sources.get(other.path.as_str()) else {
                    return false;
                };
                other.value == holder.value
                    && (other_file.lang, other_file.root.as_str()) == (file.lang, file.root.as_str())
                    && (other.path != holder.path || other.line != holder.line)
                    && !matches!((other.kind.set(), holder.kind.set()), (Some(a), Some(b)) if a != b)
            })
            .collect();
        let lines: Vec<String> = copies
            .get(&(file.lang, file.root.as_str(), holder.value.as_str()))
            .into_iter()
            .flatten()
            .map(|(other, line, class)| format!("{}:{line} [{class}]", other.path))
            .collect();
        let mut found = Vec::new();
        if !same.is_empty() {
            found.push(format!("the value of {}", places(&same)));
        }
        if !lines.is_empty() {
            found.push(format!("the literal at {}", lines.join(", ")));
        }
        if !found.is_empty() {
            out.push(Hit {
                path: holder.path.clone(),
                line: holder.line,
                place: Place::Const,
                what: format!("{} = {}", holder.name, shown(&holder.value)),
                copies: found.join("; "),
            });
        }
    }
    out
}

/// A function of the tree that the rule of the helpers reads, and its file.
type Helper<'a> = (&'a Source, &'a Func);

/// The key of the body of a helper: two helpers with one key do the same job.
fn body_key<'a>((file, f): &Helper<'a>) -> (Lang, &'a str, &'a str, usize) {
    (file.lang, &file.root, &f.body, f.params.len())
}

/// The functions that the change adds with the body of another function of the same tree, with
/// the names of the arguments in place. A method of Go and a function of a trait impl are out.
pub fn new_helpers(tree: &Tree, added: &Added) -> Vec<Hit> {
    let helpers: Vec<Helper> = tree
        .files
        .iter()
        .flat_map(|file| {
            file.funcs
                .iter()
                .filter(|f| !f.method && f.size >= SMALLEST_BODY)
                .map(move |f| (file, f))
        })
        .collect();
    let mut bodies: BTreeMap<_, Vec<Helper>> = BTreeMap::new();
    for helper in &helpers {
        bodies.entry(body_key(helper)).or_default().push(*helper);
    }
    let mut out = Vec::new();
    for helper in helpers
        .iter()
        .filter(|(file, f)| added(&file.path).contains(&f.line))
    {
        let (file, func) = *helper;
        let others: Vec<String> = bodies[&body_key(helper)]
            .iter()
            .filter(|(other, f)| other.path != file.path || f.line != func.line)
            .map(|(other, f)| format!("{} ({}:{})", f.name, other.path, f.line))
            .collect();
        if !others.is_empty() {
            out.push(Hit {
                path: file.path.clone(),
                line: func.line,
                place: Place::Helper,
                what: func.name.clone(),
                copies: format!("the body of {}", others.join(", ")),
            });
        }
    }
    out
}
