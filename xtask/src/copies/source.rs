//! The read of the tree of a commit: each Rust and Go file with its literals and its functions,
//! and each value that a declaration holds.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use super::go::GoFile;
use super::place::{Context, LABEL_NAMES, Literal, literals};
use super::reach::{Reach, rust_reach};
use super::rust::RustFile;
use super::token::Token;
use super::{Func, Holder, Kind, LANGS, Lang, Scope, go, is_go_test, rust};
use crate::git_diff::{git, git_bytes};

/// The directory of the integration tests of a Rust package, and the name of the file of the
/// unit tests of a module.
const TESTS: &str = "tests";

/// The directory of the helpers that the integration tests of a Rust package share.
const SHARED_TESTS: &str = "tests/common";

/// True when the path `path` is under a directory `dir` of the repository or of a package.
fn under(path: &str, dir: &str) -> bool {
    path.starts_with(&format!("{dir}/")) || path.contains(&format!("/{dir}/"))
}

/// The lines of a file that is test code from its first line to its last.
const WHOLE: (usize, usize) = (1, usize::MAX);

/// A Rust or Go file of the tree, as the check reads it.
#[derive(Debug)]
pub struct Source {
    pub path: String,
    pub lang: Lang,
    /// The root of its package tree: the directory of its `Cargo.toml` or of its `go.mod`, with a
    /// `/` at its end, or empty for the root of the repository.
    pub root: String,
    /// The first and the last line of each part that is test code.
    pub tests: Vec<(usize, usize)>,
    /// True for test code that the tests of other files share: a Go package that only tests
    /// import, or a file of `tests/common/`.
    pub shared: bool,
    pub literals: Vec<Literal>,
    /// Each function, and the files that can call it.
    pub funcs: Vec<(Func, Reach)>,
}

impl Source {
    pub fn in_test(&self, line: usize) -> bool {
        self.tests
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&line))
    }
}

/// The files of the tree of a commit, and each value that a declaration of the tree holds.
#[derive(Debug, Default)]
pub struct Tree {
    pub files: Vec<Source>,
    pub holders: Vec<Holder>,
}

/// A file that the read parsed, before the walk of its literals.
enum Parsed {
    Rust(RustFile, Vec<Token>),
    Go(GoFile),
}

/// Read each Rust and Go file of the tree of the commit `head`.
pub fn read_tree(repo: &Path, head: &str) -> Result<Tree, String> {
    let paths = git(repo, &["ls-tree", "-r", "--name-only", head])?;
    let mut roots: BTreeMap<Lang, Vec<String>> = BTreeMap::new();
    for path in paths.lines() {
        for (lang, _, file) in LANGS {
            if let Some(dir) = path
                .strip_suffix(file)
                .filter(|d| d.is_empty() || d.ends_with('/'))
            {
                roots.entry(lang).or_default().push(dir.to_string());
            }
        }
    }
    let mut parsed = Vec::new();
    let sources: Vec<&str> = paths
        .lines()
        .filter(|path| Lang::of(path).is_some())
        .collect();
    for (path, text) in archive(repo, head, &sources)? {
        let Some(lang) = Lang::of(&path) else {
            continue;
        };
        let Some(root) = root_of(&path, roots.get(&lang).map_or(&[][..], Vec::as_slice)) else {
            continue;
        };
        let file = match lang {
            Lang::Rust => {
                rust::read(&text).and_then(|file| Ok(Parsed::Rust(file, rust::tokens(&text)?)))
            }
            Lang::Go => go::read(&text).map(Parsed::Go),
        }
        .map_err(|e| format!("{path}: {e}"))?;
        parsed.push((path, root, file));
    }
    let modules = go_modules(
        repo,
        head,
        roots.get(&Lang::Go).map_or(&[][..], Vec::as_slice),
    )?;
    Ok(build(parsed, &roots, &modules))
}

/// The path and the text of each file of `paths` in the tree of `head`.
fn archive(repo: &Path, head: &str, paths: &[&str]) -> Result<Vec<(String, String)>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = vec!["archive", "--format=tar", head, "--"];
    args.extend(paths);
    let bytes = git_bytes(repo, &args)?;
    let mut out = Vec::new();
    let mut tar = tar::Archive::new(bytes.as_slice());
    for entry in tar.entries().map_err(|e| format!("git archive: {e}"))? {
        let mut entry = entry.map_err(|e| format!("git archive: {e}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| format!("git archive: {e}"))?
            .to_string_lossy()
            .into_owned();
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|e| format!("{path}: {e}"))?;
        out.push((path, text));
    }
    Ok(out)
}

/// The deepest root of `roots` that holds `path`.
fn root_of(path: &str, roots: &[String]) -> Option<String> {
    roots
        .iter()
        .filter(|root| path.starts_with(root.as_str()))
        .max_by_key(|root| root.len())
        .cloned()
}

/// The path of the module of each `go.mod`, by the root of the module.
fn go_modules(
    repo: &Path,
    head: &str,
    roots: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for root in roots {
        let text = git(
            repo,
            &["show", &format!("{head}:{root}{}", Lang::Go.root_file())],
        )?;
        if let Some(module) = text.lines().find_map(|line| line.strip_prefix("module ")) {
            out.insert(root.clone(), module.trim().to_string());
        }
    }
    Ok(out)
}

/// The directory of a path, with a `/` at its end, or empty at the root.
pub fn dir_of(path: &str) -> &str {
    path.rfind('/').map_or("", |at| &path[..=at])
}

/// The Go packages of the tree: what each one imports, and whether only tests import it.
#[derive(Default)]
struct GoPackages {
    /// The directories of the packages that each package imports, through its other files than
    /// its tests, at any depth.
    imports: BTreeMap<String, BTreeSet<String>>,
    /// The packages that only test files import.
    test_only: BTreeSet<String>,
    /// The packages `main`, which no package can import.
    main: BTreeSet<String>,
    /// The fields of each struct type, by the directory of its package.
    structs: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

fn go_packages(
    parsed: &[(String, String, Parsed)],
    modules: &BTreeMap<String, String>,
) -> GoPackages {
    let mut packages = GoPackages::default();
    let mut direct: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut importers: BTreeMap<String, BTreeSet<bool>> = BTreeMap::new();
    for (path, root, file) in parsed {
        let Parsed::Go(file) = file else { continue };
        let dir = dir_of(path).to_string();
        let test = is_go_test(path);
        if file.package == "main" {
            packages.main.insert(dir.clone());
        }
        let structs = packages.structs.entry(dir.clone()).or_default();
        structs.extend(file.structs.iter().cloned());
        for import in &file.imports {
            let Some(module) = modules.get(root) else {
                continue;
            };
            let Some(rest) = import.strip_prefix(module.as_str()) else {
                continue;
            };
            let target = format!("{root}{}/", rest.trim_start_matches('/'));
            importers.entry(target.clone()).or_default().insert(test);
            if !test {
                direct.entry(dir.clone()).or_default().insert(target);
            }
        }
    }
    packages.test_only = importers
        .into_iter()
        .filter(|(_, tests)| !tests.contains(&false))
        .map(|(dir, _)| dir)
        .collect();
    for dir in direct.keys() {
        let mut seen = BTreeSet::new();
        let mut next: Vec<&String> = direct[dir].iter().collect();
        while let Some(target) = next.pop() {
            if seen.insert(target.clone()) {
                next.extend(direct.get(target).into_iter().flatten());
            }
        }
        packages.imports.insert(dir.clone(), seen);
    }
    packages
}

/// The positions of the parameters that hold a label, by the name of the function.
type Labels = BTreeMap<String, BTreeSet<usize>>;

/// The positions of the parameters of each function of a tree that hold a label, by the name of
/// the function. A Go variable that names a function takes the positions of that function.
fn labels_of(funcs: &[&Func], aliases: &[(String, String)]) -> Labels {
    let mut out = Labels::new();
    for func in funcs {
        for (at, param) in func.params.iter().enumerate() {
            if LABEL_NAMES.contains(&param.as_str()) {
                out.entry(func.name.clone()).or_default().insert(at);
            }
        }
    }
    for (alias, target) in aliases {
        if let Some(positions) = out.get(target).cloned() {
            out.entry(alias.clone()).or_default().extend(positions);
        }
    }
    out
}

/// The parts of a Rust file that are test code: the whole of a file of a `tests` directory or a
/// file `tests.rs`, or else its items for the tests.
fn rust_tests(path: &str, file: &RustFile) -> Vec<(usize, usize)> {
    let whole = under(path, TESTS) || path.ends_with(&format!("/{TESTS}.rs"));
    if whole {
        vec![WHOLE]
    } else {
        file.tests.clone()
    }
}

/// Build the tree from the parsed files: the holders of each file, then the literals of each
/// file with the labels of its tree.
fn build(
    parsed: Vec<(String, String, Parsed)>,
    roots: &BTreeMap<Lang, Vec<String>>,
    modules: &BTreeMap<String, String>,
) -> Tree {
    let packages = go_packages(&parsed, modules);
    let rust_roots = roots.get(&Lang::Rust).map_or(&[][..], Vec::as_slice);
    let mut tree = Tree::default();
    for (path, root, file) in &parsed {
        match file {
            Parsed::Rust(file, _) => tree
                .holders
                .extend(rust_holders(path, root, rust_roots, file)),
            Parsed::Go(file) => tree.holders.extend(go_holders(path, root, file, &packages)),
        }
    }
    let labels = tree_labels(&parsed);
    let walked: Vec<_> = parsed
        .iter()
        .map(|(path, root, file)| walk(path, root, file, &packages, &labels))
        .collect();
    for ((path, root, file), (tests, literals)) in parsed.into_iter().zip(walked) {
        let (lang, funcs, shared) = match file {
            Parsed::Rust(file, _) => {
                let reach = |f: &Func| {
                    let test = tests.iter().any(|(a, b)| (*a..=*b).contains(&f.line));
                    rust_reach(&path, &root, rust_roots, f.scope, test)
                };
                (
                    Lang::Rust,
                    with_reach(file.funcs, reach),
                    under(&path, SHARED_TESTS),
                )
            }
            Parsed::Go(file) => {
                let reach = |f: &Func| go_reach(&path, &root, f.scope == Scope::Package, &packages);
                let shared = packages.test_only.contains(dir_of(&path));
                (Lang::Go, with_reach(file.funcs, reach), shared)
            }
        };
        tree.files.push(Source {
            path,
            lang,
            root,
            tests,
            shared,
            literals,
            funcs,
        });
    }
    tree
}

/// Each function of `funcs`, with the files that `reach` gives it.
fn with_reach(funcs: Vec<Func>, reach: impl Fn(&Func) -> Reach) -> Vec<(Func, Reach)> {
    funcs
        .into_iter()
        .map(|func| {
            let reach = reach(&func);
            (func, reach)
        })
        .collect()
}

/// The labels of each package tree: the positions of the parameters of each function that hold
/// a label, by the language and the root of the tree.
fn tree_labels(parsed: &[(String, String, Parsed)]) -> BTreeMap<(Lang, &str), Labels> {
    let mut funcs: BTreeMap<(Lang, &str), Vec<&Func>> = BTreeMap::new();
    let mut aliases: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for (_, root, file) in parsed {
        match file {
            Parsed::Rust(file, _) => {
                funcs
                    .entry((Lang::Rust, root))
                    .or_default()
                    .extend(&file.funcs);
            }
            Parsed::Go(file) => {
                funcs
                    .entry((Lang::Go, root))
                    .or_default()
                    .extend(&file.funcs);
                aliases
                    .entry(root)
                    .or_default()
                    .extend(file.aliases.iter().cloned());
            }
        }
    }
    funcs
        .iter()
        .map(|(key, funcs)| {
            let aliases = aliases.get(key.1).map_or(&[][..], Vec::as_slice);
            (*key, labels_of(funcs, aliases))
        })
        .collect()
}

/// The test code of a parsed file, and its literals with the class of each place.
fn walk(
    path: &str,
    root: &str,
    file: &Parsed,
    packages: &GoPackages,
    labels: &BTreeMap<(Lang, &str), Labels>,
) -> (Vec<(usize, usize)>, Vec<Literal>) {
    let (lang, tests, tokens) = match file {
        Parsed::Rust(file, tokens) => (Lang::Rust, rust_tests(path, file), tokens),
        Parsed::Go(file) => (Lang::Go, go_tests(path, packages), &file.code),
    };
    let (no_labels, no_structs) = (BTreeMap::new(), BTreeMap::new());
    let context = Context {
        lang,
        labels: labels.get(&(lang, root)).unwrap_or(&no_labels),
        structs: packages.structs.get(dir_of(path)).unwrap_or(&no_structs),
        tests: &tests,
    };
    let literals = literals(tokens, &context);
    (tests, literals)
}

/// The holders of the values of a Rust file, with the reach of each one.
fn rust_holders(path: &str, root: &str, roots: &[String], file: &RustFile) -> Vec<Holder> {
    let whole = rust_tests(path, file) == [WHOLE];
    file.values
        .iter()
        .map(|value| Holder {
            name: value.name.clone(),
            value: value.value.clone(),
            path: path.to_string(),
            line: value.line,
            kind: if value.closed {
                // the name of a text of a closed set is `Set::Variant`
                let set = value.name.split("::").next().unwrap_or(&value.name);
                Kind::Text(set.to_string())
            } else {
                Kind::Const
            },
            lang: Lang::Rust,
            reach: rust_reach(path, root, roots, value.scope, value.test || whole),
        })
        .collect()
}

/// The test code of a Go file: a file `_test.go`, or each file of a package that only tests
/// import.
fn go_tests(path: &str, packages: &GoPackages) -> Vec<(usize, usize)> {
    if is_go_test(path) || packages.test_only.contains(dir_of(path)) {
        vec![WHOLE]
    } else {
        Vec::new()
    }
}

/// The reach of a Go declaration of the file `path` in the module at `root`. `exported` is true
/// for a name with a capital letter.
///
/// An exported name of a package that is not `main` reaches the module, apart from the packages
/// that its package imports: they cannot import it. The value of a test file or of a package that
/// only tests import reaches only test code.
fn go_reach(path: &str, root: &str, exported: bool, packages: &GoPackages) -> Reach {
    let dir = dir_of(path);
    let test_file = is_go_test(path);
    let tests = test_file || packages.test_only.contains(dir);
    if exported && !test_file && !packages.main.contains(dir) {
        Reach::Module {
            dir: root.to_string(),
            // a package that imports no package of the module has no such package
            apart: packages.imports.get(dir).cloned().unwrap_or_default(),
            tests,
        }
    } else {
        Reach::Package {
            dir: dir.to_string(),
            tests,
        }
    }
}

/// The holders of the consts and of the tables of a Go file, with the reach of each one.
fn go_holders(path: &str, root: &str, file: &GoFile, packages: &GoPackages) -> Vec<Holder> {
    let qualified = |name: &str| match name.contains('.') {
        true => name.to_string(),
        false => format!("{}.{name}", file.package),
    };
    let consts = file.consts.iter().map(|c| (c, false));
    let tables = file.tables.iter().map(|c| (c, true));
    consts
        .chain(tables)
        .map(|(c, table)| Holder {
            name: qualified(&c.name),
            value: c.value.clone(),
            path: path.to_string(),
            line: c.line,
            kind: match (&c.typ, table) {
                (Some(typ), true) => Kind::Text(qualified(typ)),
                (Some(typ), false) => Kind::Typed(qualified(typ)),
                (None, _) => Kind::Const,
            },
            lang: Lang::Go,
            reach: go_reach(path, root, c.name.starts_with(char::is_uppercase), packages),
        })
        .collect()
}
