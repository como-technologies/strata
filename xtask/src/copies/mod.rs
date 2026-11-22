//! The check of the copies of a change: `xtask copies BASE [HEAD]`.
//!
//! A change must not copy a value of a closed set, a const, a route, or a helper. The fix session
//! and the verify of a pull request run this check, so they find the same copies. "The check of
//! the copies" in `docs/src/development/code.md` states what a finding is.
//!
//! The check reads the tree of `HEAD` and the lines that `git diff BASE...HEAD` adds in the Rust
//! and Go files. It lists the values of the closed sets and the string consts of each package
//! tree: the directory of a `Cargo.toml` or of a `go.mod`. A finding is one of two kinds:
//!
//! 1. A string literal on an added line holds a value of a closed set or of a const that its file
//!    reaches, or a route that another line of its tree holds.
//! 2. A const, a function, or a route that the diff adds has its value or its body in another line
//!    of its tree.
//!
//! The class of a place says what a literal does there. Some classes are no findings, such as a
//! JSON input of a test, the name of a case, and the label of a failure. The check lists them
//! apart, with their class.

mod fresh;
mod go;
mod go_lex;
mod hits;
mod place;
mod reach;
mod routes;
mod rust;
mod rust_sets;
mod source;
mod token;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use crate::git_diff;
use place::Class;
use reach::Reach;

/// Each language of the check: the extension of its files, and the file that makes a directory
/// the root of a package tree.
const LANGS: [(Lang, &str, &str); 2] =
    [(Lang::Rust, "rs", "Cargo.toml"), (Lang::Go, "go", "go.mod")];

/// The most characters of a literal that a line of the report shows.
const SHOWN: usize = 60;

/// The language of a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    Rust,
    Go,
}

impl Lang {
    /// The language of the file `path`, by its extension, or `None` for another file.
    pub fn of(path: &str) -> Option<Lang> {
        let (_, extension) = path.rsplit_once('.')?;
        LANGS
            .iter()
            .find(|(_, ext, _)| *ext == extension)
            .map(|(lang, _, _)| *lang)
    }

    /// The file that makes a directory the root of a package tree of the language.
    pub fn root_file(self) -> &'static str {
        LANGS
            .iter()
            .find(|(lang, _, _)| *lang == self)
            .map_or("", |(_, _, file)| file)
    }
}

/// The part of its package that a declaration reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Its module and the children of its module. A Go name with a small letter reaches its
    /// package.
    Module,
    /// The module above its module, and each child of that module: `pub(super)`.
    Parent,
    /// Its whole package tree: `pub` or `pub(crate)`, or a Go name with a capital letter.
    Package,
}

/// A function of a source file, for the rule of the helpers and for the labels of a call.
#[derive(Debug, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    /// The names of the parameters, in order, without `self`.
    pub params: Vec<String>,
    /// The tokens of the body as one line of text, with `$N` in place of the parameter `N`.
    pub body: String,
    /// The count of the tokens of the body.
    pub size: usize,
    /// True for a method of Go, or a function of an `impl Trait for Type` of Rust.
    pub method: bool,
    /// The line of the keyword `fn` or `func`, and the last line of the body.
    pub line: usize,
    pub last: usize,
}

/// What a declaration that holds a value is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A string const that is in no closed set.
    Const,
    /// A Go const of a named type, by the name of the type: a value of the closed set of the type.
    Typed(String),
    /// A text of the closed set that it names, and no const. The serde name of a variant is such a
    /// text. So is the text of a variant in a table or a `match`, or of a row of a Go table.
    Text(String),
}

impl Kind {
    /// The closed set of the value, or `None` for a const in no closed set.
    fn set(&self) -> Option<&str> {
        match self {
            Kind::Const => None,
            Kind::Typed(set) | Kind::Text(set) => Some(set),
        }
    }
}

/// A value that a declaration holds: a text of a closed set, or a string const.
#[derive(Debug, PartialEq, Eq)]
pub struct Holder {
    /// The name in the code, such as `model.EventDrift`, `Tier::Running`, or `SCHEMA`.
    pub name: String,
    pub value: String,
    pub path: String,
    pub line: usize,
    pub kind: Kind,
    pub lang: Lang,
    pub reach: Reach,
}

/// What a line of the report names: a literal in the place of its class, or a declaration that
/// the change adds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// A literal, and the class of its place.
    Literal(Class),
    /// A const that the change adds.
    Const,
    /// A function that the change adds.
    Helper,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Literal(class) => class.fmt(f),
            Place::Const => f.write_str("const"),
            Place::Helper => f.write_str("helper"),
        }
    }
}

/// One line of the report: a place of the change and what it holds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub place: Place,
    /// The literal, or the name and the value of a declaration.
    pub what: String,
    /// What the place copies, such as `drift is the value of model.EventDrift (...)`.
    pub copies: String,
}

impl fmt::Display for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} [{}] {}: {}",
            self.path, self.line, self.place, self.what, self.copies
        )
    }
}

/// What the check found in a change.
#[derive(Debug, Default)]
pub struct Found {
    /// The count of the added lines that the check read, and of their files.
    pub lines: usize,
    pub files: usize,
    /// Each copy that a line of the change adds.
    pub findings: Vec<Hit>,
    /// Each value of a closed set or of a const on an added line, in a place that is no finding.
    pub skipped: Vec<Hit>,
}

impl Found {
    /// The exit code of the check: 1 when the change adds a copy, and 0 when it adds none.
    pub fn exit_code(&self) -> u8 {
        u8::from(!self.findings.is_empty())
    }
}

/// Find the copies that the lines of `git diff base...head` add, in the tree of `head`.
///
/// `repo` is the root of the repository. The error names the git command or the file that failed.
pub fn check(repo: &Path, base: &str, head: &str) -> Result<Found, String> {
    let range = format!("{base}...{head}");
    let pathspecs: Vec<String> = LANGS.iter().map(|(_, ext, _)| format!("*.{ext}")).collect();
    let pathspecs: Vec<&str> = pathspecs.iter().map(String::as_str).collect();
    let added = git_diff::added(repo, &range, &pathspecs)?;
    let tree = source::read_tree(repo, head)?;
    let mut found = Found {
        lines: added.values().map(BTreeSet::len).sum(),
        files: added.len(),
        ..Found::default()
    };
    let none = BTreeSet::new();
    let added_of = |path: &str| added.get(path).unwrap_or(&none);
    let (copies, skipped, reported) = hits::literal_hits(&tree, &added_of);
    found.findings.extend(copies);
    found.skipped.extend(skipped);
    found
        .findings
        .extend(routes::routes(&tree, &added_of, &reported));
    found
        .findings
        .extend(fresh::new_consts(&tree, &added_of, &reported));
    found.findings.extend(fresh::new_helpers(&tree, &added_of));
    found.findings.sort();
    found.findings.dedup();
    found.skipped.sort();
    Ok(found)
}

/// A literal as a line of the report shows it: in quotes, and cut after [`SHOWN`] characters.
fn shown(value: &str) -> String {
    let cut: String = value.chars().take(SHOWN).collect();
    if cut.len() < value.len() {
        format!("{cut:?}…")
    } else {
        format!("{cut:?}")
    }
}

/// The holders of each value, by the value.
type ByValue<'a> = BTreeMap<&'a str, Vec<&'a Holder>>;
