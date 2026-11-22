use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use super::place::{Class, Context, Literal, literals};
use super::rust_sets::rename;
use super::token::Token;
use super::*;
use crate::git::isolate_git;
use crate::process::CommandExt;
use crate::variants::one_of_each;

/// The lines of a file that is test code from its first line to its last.
const ALL_TEST: [(usize, usize); 1] = [(1, usize::MAX)];

/// The tokens of a source of `lang`.
fn tokens_of(lang: Lang, code: &str) -> Vec<Token> {
    match lang {
        Lang::Go => go::read(code).unwrap().code,
        Lang::Rust => rust::tokens(code).unwrap(),
    }
}

/// The literals of a source of `lang`, with the labels and the structs that the test gives. The
/// whole source is test code when `test` is true.
fn classes_of(
    lang: Lang,
    code: &str,
    test: bool,
    labels: &BTreeMap<String, BTreeSet<usize>>,
) -> Vec<Literal> {
    let structs = match lang {
        Lang::Go => go::read(code).unwrap().structs.into_iter().collect(),
        Lang::Rust => BTreeMap::new(),
    };
    let context = Context {
        lang,
        labels,
        structs: &structs,
        tests: if test { &ALL_TEST } else { &[] },
    };
    literals(&tokens_of(lang, code), &context)
}

/// The class of the literal of `code` whose value holds the word `mark`.
fn class_of_mark(lang: Lang, code: &str, test: bool) -> Class {
    let labels = BTreeMap::from([("check".to_string(), BTreeSet::from([1]))]);
    let found = classes_of(lang, code, test, &labels);
    let marked: Vec<&Literal> = found.iter().filter(|l| l.value.contains("mark")).collect();
    assert_eq!(marked.len(), 1, "{code}: {found:?}");
    marked[0].class.clone()
}

/// A source where the literal that holds `mark` has the class `class`: its language, whether
/// it is test code, and the code. The `match` has no `_` arm, so a new class needs a sample.
fn sample(class: &Class) -> (Lang, bool, &'static str) {
    match class {
        Class::Json => (
            Lang::Go,
            true,
            "package p\nvar x = \"{\\\"kind\\\": \\\"mark\\\"}\"\n",
        ),
        Class::Rules => (
            Lang::Rust,
            true,
            "const R: &str = \"rules:\\n  - id: mark\\n\";\n",
        ),
        Class::Script => (
            Lang::Go,
            true,
            "package p\nvar s = \"#!/bin/sh\\necho mark\\n\"\n",
        ),
        Class::Case => (
            Lang::Go,
            true,
            "package p\nfunc T(t *testing.T) {\n\tt.Run(\"mark\", nil)\n}\n",
        ),
        Class::Label => (
            Lang::Rust,
            true,
            "fn t() {\n    assert!(ok(), \"mark\");\n}\n",
        ),
        Class::Tag => (
            Lang::Go,
            false,
            "package p\ntype S struct {\n\tA string `json:\"mark\"`\n}\n",
        ),
        Class::Compare => (
            Lang::Go,
            false,
            "package p\nfunc f(k string) bool {\n\treturn k == \"mark\"\n}\n",
        ),
        Class::Switch => (
            Lang::Rust,
            false,
            "fn f(k: &str) {\n    match k {\n        \"mark\" => {}\n        _ => {}\n    }\n}\n",
        ),
        Class::Value => (
            Lang::Go,
            false,
            "package p\nfunc f() {\n\tx := \"mark\"\n\t_ = x\n}\n",
        ),
        Class::Return => (
            Lang::Rust,
            false,
            "fn f() -> &'static str {\n    \"mark\"\n}\n",
        ),
        Class::Call(_) => (Lang::Go, false, "package p\nfunc f() {\n\tg(\"mark\")\n}\n"),
        Class::Field(_) => (Lang::Go, false, "package p\nvar s = S{Kind: \"mark\"}\n"),
        Class::Row(_) => (
            Lang::Go,
            false,
            "package p\ntype entry struct {\n\tkind string\n}\nvar rows = []entry{{\"mark\"}}\n",
        ),
        Class::Other => (
            Lang::Rust,
            false,
            "fn f() {\n    if \"mark\".is_empty() {}\n}\n",
        ),
    }
}

/// The criterion of #929: the scan names the class of each place, in Go and in Rust.
#[test]
fn the_scan_gives_each_class_to_the_place_of_its_sample() {
    let classes = one_of_each![
        Class::Json,
        Class::Rules,
        Class::Script,
        Class::Case,
        Class::Label,
        Class::Tag,
        Class::Compare,
        Class::Switch,
        Class::Value,
        Class::Return,
        Class::Call("g".to_string()),
        Class::Field("Kind".to_string()),
        Class::Row(Some("kind".to_string())),
        Class::Other,
    ];
    for class in classes {
        let (lang, test, code) = sample(&class);
        assert_eq!(class_of_mark(lang, code, test), class, "{code}");
    }
}

/// The criterion of #929: a JSON input of a test, a rule file, a case name, and the label of a
/// failure are no findings. A script of a test and the tag of a field are none either.
#[test]
fn only_the_input_of_a_test_a_case_a_label_and_a_tag_are_no_findings() {
    let not_findings = [
        Class::Json,
        Class::Rules,
        Class::Script,
        Class::Case,
        Class::Label,
        Class::Tag,
    ];
    let findings = [
        Class::Compare,
        Class::Switch,
        Class::Value,
        Class::Return,
        Class::Call("g".to_string()),
        Class::Field("Kind".to_string()),
        Class::Row(None),
        Class::Other,
    ];
    assert!(not_findings.iter().all(|c| !c.is_finding()));
    assert!(findings.iter().all(Class::is_finding));
}

/// The input of a test is no input in other code: a JSON text of the code is a value there.
#[test]
fn a_json_text_is_an_input_only_in_test_code() {
    let (lang, _, code) = sample(&Class::Json);
    assert_eq!(class_of_mark(lang, code, false), Class::Value);
}

#[test]
fn the_label_of_a_failure_is_each_argument_of_a_failure_and_of_a_parameter_for_a_label() {
    for (lang, code) in [
        (
            Lang::Go,
            "package p\nfunc T(t *testing.T) {\n\tt.Fatalf(\"%s\", f(\"mark\"))\n}\n",
        ),
        (
            Lang::Go,
            "package p\nfunc T(t *testing.T) {\n\tcheck(t, \"mark\", page)\n}\n",
        ),
        (
            Lang::Rust,
            "fn t() {\n    assert_eq!(a, b, \"mark {}\", c);\n}\n",
        ),
        (Lang::Rust, "fn t() {\n    let x = y.expect(\"mark\");\n}\n"),
        (Lang::Rust, "fn t() {\n    panic!(\"mark\");\n}\n"),
    ] {
        assert_eq!(class_of_mark(lang, code, true), Class::Label, "{code}");
    }
    // `fmt.Errorf` builds an error and fails no test, and the operands of `assert_eq!` are compared
    for (lang, code, class) in [
        (
            Lang::Go,
            "package p\nfunc f() error {\n\treturn fmt.Errorf(\"mark\")\n}\n",
            Class::Call("Errorf".to_string()),
        ),
        (
            Lang::Go,
            "package p\nfunc T(t *testing.T) {\n\tcheck(t, page, \"mark\")\n}\n",
            Class::Call("check".to_string()),
        ),
        (
            Lang::Rust,
            "fn t() {\n    assert_eq!(a, \"mark\");\n}\n",
            Class::Compare,
        ),
    ] {
        assert_eq!(class_of_mark(lang, code, true), class, "{code}");
    }
}

#[test]
fn a_row_takes_the_name_of_its_field_and_a_field_name_is_a_case() {
    for (lang, code, class) in [
        (
            Lang::Go,
            "package p\nvar cases = []struct {\n\tname, want string\n\tn    int\n}{\n\t{\"mark\", \"x\", 1},\n}\n",
            Class::Case,
        ),
        (
            Lang::Go,
            "package p\nvar cases = []struct {\n\tname, want string\n}{\n\t{\"x\", \"mark\"},\n}\n",
            Class::Row(Some("want".to_string())),
        ),
        (
            Lang::Go,
            "package p\nvar cases = map[string]struct{ where string }{\n\t\"k\": {\"mark\"},\n}\n",
            Class::Label,
        ),
        (
            Lang::Go,
            "package p\nvar words = []string{\"mark\"}\n",
            Class::Row(None),
        ),
        (
            Lang::Rust,
            "fn t() {\n    for (name, want) in [(\"mark\", 1)] {}\n}\n",
            Class::Case,
        ),
        (
            Lang::Rust,
            "fn t() {\n    for (path, want) in &[(\"mark\", 1)] {}\n}\n",
            Class::Row(Some("path".to_string())),
        ),
        (
            Lang::Rust,
            "fn t() {\n    for word in [\"mark\"] {}\n}\n",
            Class::Row(Some("word".to_string())),
        ),
        (
            Lang::Rust,
            "fn t() {\n    let a = [\"mark\"];\n}\n",
            Class::Row(None),
        ),
    ] {
        assert_eq!(class_of_mark(lang, code, true), class, "{code}");
    }
}

#[test]
fn the_key_of_a_map_of_a_test_is_json_and_a_rust_json_macro_is_json() {
    for (lang, code) in [
        (
            Lang::Go,
            "package p\nvar doc = map[string]any{\"mark\": 1}\n",
        ),
        (
            Lang::Rust,
            "fn t() {\n    let doc = json!({\"kind\": [\"mark\"]});\n}\n",
        ),
    ] {
        assert_eq!(class_of_mark(lang, code, true), Class::Json, "{code}");
    }
}

/// The values of `pairs`, each with one holder, as the rule of the words reads them.
fn values_of(pairs: &[&'static str]) -> Vec<Holder> {
    pairs
        .iter()
        .map(|value| Holder {
            name: format!("set.{value}"),
            value: value.to_string(),
            path: "set.go".to_string(),
            line: 1,
            kind: Kind::Const,
            lang: Lang::Go,
            reach: reach::Reach::Package {
                dir: String::new(),
                tests: false,
            },
        })
        .collect()
}

#[test]
fn a_plain_word_counts_alone_or_beside_a_value_that_is_no_plain_word() {
    let holders = values_of(&[
        "drift",
        "newer",
        "version",
        "DriftFound",
        "read-error",
        "--pin",
        "no answer",
    ]);
    let mut values: ByValue = BTreeMap::new();
    for holder in &holders {
        values
            .entry(holder.value.as_str())
            .or_default()
            .push(holder);
    }
    for (text, want) in [
        ("drift", vec!["drift"]),
        ("newer ", vec!["newer"]),
        ("drift DriftFound ", vec!["DriftFound", "drift"]),
        ("newer version", vec![]),
        ("the drift of a pack", vec![]),
        ("the cause read-error names %v", vec!["read-error"]),
        ("read-errors", vec![]),
        ("behind record-0.json --pin ", vec!["--pin"]),
        ("--pinned", vec![]),
        (" no answer ", vec!["no answer"]),
        ("reason=no answer", vec![]),
    ] {
        let found: Vec<&str> = hits::held(text, &values).into_iter().collect();
        assert_eq!(found, want, "{text:?}");
    }
}

#[test]
fn a_value_counts_from_three_characters_and_a_plain_word_from_two_letters() {
    for (value, counts) in [
        ("ok", true),
        ("a", false),
        ("--", false),
        ("v1", false),
        ("--pin", true),
        ("1.1.0", true),
        ("", false),
    ] {
        assert_eq!(hits::counts(value), counts, "{value:?}");
    }
}

#[test]
fn a_route_drops_its_method_and_the_names_of_its_segments() {
    for (value, want) in [
        ("/v1/things/{uid}", Some("/v1/things/{}")),
        (
            "GET /v1/things/{name}/parts/{uid}",
            Some("/v1/things/{}/parts/{}"),
        ),
        ("/v1/clusters/%s/x", Some("/v1/clusters/{}/x")),
        ("/healthz", None),
        ("v1/packs", None),
    ] {
        assert_eq!(routes::route(value).as_deref(), want, "{value}");
    }
}

#[test]
fn the_go_tokens_end_a_line_with_a_semicolon_where_the_compiler_adds_one() {
    let tokens = go_lex::tokens("x := f(\n\ta,\n)\nif y {\n}\n").unwrap();
    let marks: Vec<String> = tokens
        .iter()
        .map(|t| match t {
            Token::Word { text, .. } | Token::Mark { text, .. } => text.clone(),
            Token::Group { .. } => "()".to_string(),
            _ => "?".to_string(),
        })
        .collect();
    assert_eq!(marks, ["x", ":=", "f", "()", ";", "if", "y", "()", ";"]);
}

#[test]
fn the_go_tokens_decode_each_text_and_keep_its_lines() {
    let tokens =
        go_lex::tokens("a := \"x\\ty\\u00e9\\x41\\101\"\nb := `one\ntwo`\nc := '\\''\n").unwrap();
    let texts: Vec<(String, usize, usize)> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Text { value, line, last } => Some((value.clone(), *line, *last)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            ("x\tyéAA".to_string(), 1, 1),
            ("one\ntwo".to_string(), 2, 3)
        ]
    );
    assert!(go_lex::tokens("a := \"open\n").is_err());
    assert!(go_lex::tokens("f(]").is_err());
}

#[test]
fn a_go_file_gives_its_consts_its_tables_its_structs_its_functions_and_its_aliases() {
    let file = go::read(
        "package model\n\nimport (\n\t\"fmt\"\n\tm \"example.com/m/names\"\n)\n\n\
         type Kind string\n\nconst (\n\tDrift Kind = \"drift\"\n\tOld        = \"old\"\n\tA, B = \"a\", \"b\"\n\tn = 1\n)\n\n\
         var reasons = map[Kind]reason{\n\tDrift: {\"DriftFound\", \"A drift.\"},\n\tm.Old: {\"OldCopy\", \"\"},\n}\n\n\
         var mustHold = testkit.HoldsAll\n\n\
         type entry struct {\n\tfirst, second string\n\tn    int `json:\"n\"`\n\t*Embedded\n}\n\n\
         func check(t *testing.T, at, text string, words ...string) {\n\tt.Log(at)\n}\n\n\
         func (r row) String() string { return r.name }\n",
    )
    .unwrap();
    assert_eq!(file.package, "model");
    assert_eq!(file.imports, ["fmt", "example.com/m/names"]);
    let consts: Vec<(&str, Option<&str>, &str)> = file
        .consts
        .iter()
        .map(|c| (c.name.as_str(), c.typ.as_deref(), c.value.as_str()))
        .collect();
    assert_eq!(
        consts,
        [
            ("Drift", Some("Kind"), "drift"),
            ("Old", None, "old"),
            ("A", None, "a"),
            ("B", None, "b")
        ]
    );
    let tables: Vec<(&str, &str)> = file
        .tables
        .iter()
        .map(|c| (c.name.as_str(), c.value.as_str()))
        .collect();
    assert_eq!(
        tables,
        [
            ("reasons[Drift]", "DriftFound"),
            ("reasons[Drift]", "A drift."),
            ("reasons[Old]", "OldCopy"),
            ("reasons[Old]", "")
        ]
    );
    assert_eq!(
        file.structs,
        [(
            "entry".to_string(),
            vec![
                "first".to_string(),
                "second".to_string(),
                "n".to_string(),
                "Embedded".to_string()
            ]
        )]
    );
    assert_eq!(
        file.aliases,
        [("mustHold".to_string(), "HoldsAll".to_string())]
    );
    let funcs: Vec<(&str, &[String], bool)> = file
        .funcs
        .iter()
        .map(|f| (f.name.as_str(), f.params.as_slice(), f.method))
        .collect();
    assert_eq!(funcs.len(), 2);
    assert_eq!(funcs[0].0, "check");
    assert_eq!(funcs[0].1, ["t", "at", "text", "words"]);
    assert!(!funcs[0].2 && funcs[1].2);
    assert!(file.code.iter().all(|t| !t.is_word("import")));
}

#[test]
fn serde_writes_the_name_of_a_variant_by_each_rule_of_rename_all() {
    for (rule, want) in [
        (None, "InPlace"),
        (Some("lowercase"), "inplace"),
        (Some("UPPERCASE"), "INPLACE"),
        (Some("camelCase"), "inPlace"),
        (Some("snake_case"), "in_place"),
        (Some("SCREAMING_SNAKE_CASE"), "IN_PLACE"),
        (Some("kebab-case"), "in-place"),
        (Some("SCREAMING-KEBAB-CASE"), "IN-PLACE"),
    ] {
        assert_eq!(rename("InPlace", rule), want, "{rule:?}");
    }
}

#[test]
fn a_rust_file_gives_the_texts_of_its_closed_sets_and_its_consts_and_no_attribute_is_a_token() {
    let code = "\
/// A doc comment \"in quotes\".
#[derive(Debug, Serialize)]
#[serde(rename_all = \"kebab-case\", deny_unknown_fields)]
pub enum Tier {
    InPlace,
    #[serde(rename = \"new-node\", alias = \"node\")]
    NewNode,
    #[serde(skip)]
    Hidden,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Either { One, Two }

impl_texts!(pub(super) Format { Json => \"json-lines\", Text => \"text\" });

impl Fault {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Missing { .. } => \"not-found\",
            Fault::Bad | Fault::Worse => \"bad-input\",
        }
    }
}

const SCHEMA: &str = \"strata-x/1\";
pub const NAMES: [&str; 2] = [\"a-1\", \"b-2\"];
pub const LANGS: [(Lang, &str); 1] = [(Lang::Go, \"golang\")];

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Tier::InPlace => f.write_str(\"in place\"),
            Tier::NewNode => f.write_str(\"new node\"),
            Tier::Hidden => write!(f, \"hidden {}\", 1),
        }
    }
}

#[cfg(test)]
mod tests {
    const IN_TEST: &str = \"in-test\";
}
";
    let file = rust::read(code).unwrap();
    let values: Vec<(&str, &str, bool, Scope, bool)> = file
        .values
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str(), v.closed, v.scope, v.test))
        .collect();
    assert_eq!(
        values,
        [
            ("Tier::InPlace", "in-place", true, Scope::Package, false),
            ("Tier::NewNode", "new-node", true, Scope::Package, false),
            ("Tier::NewNode", "node", true, Scope::Package, false),
            ("Format::Json", "json-lines", true, Scope::Parent, false),
            ("Format::Text", "text", true, Scope::Parent, false),
            ("Fault::Missing", "not-found", true, Scope::Package, false),
            ("Fault::Bad", "bad-input", true, Scope::Package, false),
            ("Fault::Worse", "bad-input", true, Scope::Package, false),
            ("SCHEMA", "strata-x/1", false, Scope::Module, false),
            ("NAMES", "a-1", true, Scope::Package, false),
            ("NAMES", "b-2", true, Scope::Package, false),
            ("LANGS", "golang", true, Scope::Package, false),
            ("Tier::InPlace", "in place", true, Scope::Package, false),
            ("Tier::NewNode", "new node", true, Scope::Package, false),
            ("IN_TEST", "in-test", false, Scope::Module, true),
        ]
    );
    assert_eq!(file.tests.len(), 1);
    let texts: Vec<String> = rust::tokens(code)
        .unwrap()
        .iter()
        .flat_map(all_texts)
        .collect();
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("in quotes") || t == "kebab-case"),
        "{texts:?}"
    );
}

/// The value of each string literal of a token, at each depth.
fn all_texts(token: &Token) -> Vec<String> {
    match token {
        Token::Text { value, .. } => vec![value.clone()],
        Token::Group { tokens, .. } => tokens.iter().flat_map(all_texts).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_rust_declaration_reaches_its_module_its_parent_or_its_package() {
    let roots = ["".to_string(), "xtask/".to_string()];
    let reach =
        |path: &str, scope: Scope, test: bool| reach::rust_reach(path, "", &roots, scope, test);
    for (from, scope, path, test, holds) in [
        ("src/a.rs", Scope::Module, "src/a.rs", false, true),
        ("src/a.rs", Scope::Module, "src/a/b.rs", false, true),
        ("src/a.rs", Scope::Module, "src/c.rs", false, false),
        ("src/a/mod.rs", Scope::Module, "src/a/b.rs", false, true),
        ("src/a/b.rs", Scope::Parent, "src/a/c.rs", false, true),
        ("src/a/b.rs", Scope::Parent, "src/d.rs", false, false),
        ("src/a.rs", Scope::Package, "tests/cli.rs", true, true),
        (
            "src/a.rs",
            Scope::Package,
            "xtask/src/main.rs",
            false,
            false,
        ),
    ] {
        assert_eq!(
            reach(from, scope, false).holds(path, test),
            holds,
            "{from} {scope:?} {path}"
        );
    }
    // a value of test code reaches only test code
    let in_test = reach("tests/common/mod.rs", Scope::Package, true);
    assert!(in_test.holds("tests/cli.rs", true));
    assert!(!in_test.holds("src/a.rs", false));
}

/// A git repository in a temporary directory. It goes when the test ends.
struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    fn new() -> Repo {
        let repo = Repo {
            dir: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) {
        let out = isolate_git(Command::new("git").arg("-C").arg(self.path()).args(args))
            .output_locked()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    /// Make the branch `name` at the commit of the work tree, and go to it.
    fn branch(&self, name: &str) {
        self.git(&["checkout", "--quiet", "-b", name]);
    }

    /// Go to the branch `name`.
    fn to(&self, name: &str) {
        self.git(&["checkout", "--quiet", name]);
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "."]);
        self.git(&["commit", "--quiet", "-m", message]);
    }

    /// The findings and the places that are no copy of the change from `main` to `HEAD`, each one
    /// as the report prints it.
    fn copies(&self) -> (Vec<String>, Vec<String>) {
        let found = check(self.path(), "main", "HEAD").unwrap();
        let lines = |hits: &[Hit]| hits.iter().map(Hit::to_string).collect::<Vec<_>>();
        (lines(&found.findings), lines(&found.skipped))
    }
}

/// The Go module of the tests of the check: a package `names` of closed sets, a package `use`
/// that imports it, and a package `low` that `names` imports.
fn go_module(repo: &Repo) {
    repo.write("go.mod", "module example.com/m\n\ngo 1.22\n");
    repo.write(
        "names/names.go",
        "package names\n\nimport \"example.com/m/low\"\n\n// Kind is a closed set.\ntype Kind string\n\n\
         const (\n\tDrift     Kind = \"drift\"\n\tReadError Kind = \"read-error\"\n)\n\n\
         const (\n\tFlagPin = \"--pin\"\n\ttitle   = \"Lost contact\"\n)\n\n\
         var reasons = map[Kind]string{\n\tDrift: \"DriftFound\",\n}\n\nvar _ = low.X\n",
    );
    repo.write(
        "low/low.go",
        "package low\n\n// X is a value.\nconst X = \"x\"\n",
    );
    repo.write(
        "use/use.go",
        "package use\n\nimport \"example.com/m/names\"\n\n// Routes lists the routes.\nfunc Routes() []string {\n\treturn []string{\"/v1/cluster\", names.FlagPin}\n}\n",
    );
}

/// The criterion of #929: each kind of finding of Go, and each place that is no finding.
#[test]
fn the_check_lists_each_value_of_go_that_an_added_line_copies_with_the_class_of_its_place() {
    let repo = Repo::new();
    go_module(&repo);
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "names/names_test.go",
        "package names\n\nimport \"testing\"\n\nfunc TestKinds(t *testing.T) {\n\
         \tif Kind(\"x\") == \"drift\" {\n\
         \t\tt.Errorf(\"read-error %v\", 1)\n\
         \t}\n\
         \tt.Run(\"drift\", func(t *testing.T) {})\n\
         \t_ = \"{\\\"kind\\\": \\\"read-error\\\"}\"\n\
         \twant := \"drift DriftFound \"\n\
         \t_ = []string{want, \"Lost contact\", \"newer version\"}\n\
         }\n",
    );
    repo.write(
        "use/use_test.go",
        "package use\n\nfunc pin() string {\n\treturn \"--pin\"\n}\n\nvar title = \"Lost contact\"\nvar route = \"/v1/cluster\"\n",
    );
    repo.write("low/low_test.go", "package low\n\nvar pin = \"--pin\"\n");
    repo.commit("change");

    let (findings, skipped) = repo.copies();
    assert_eq!(
        findings,
        [
            "names/names_test.go:6 [compare] \"drift\": drift is the value of names.Drift (names/names.go:9)",
            "names/names_test.go:11 [value] \"drift DriftFound \": DriftFound is the value of names.reasons[Drift] (names/names.go:19)",
            "names/names_test.go:11 [value] \"drift DriftFound \": drift is the value of names.Drift (names/names.go:9)",
            "names/names_test.go:12 [row] \"Lost contact\": Lost contact is the value of names.title (names/names.go:15)",
            "use/use_test.go:4 [return] \"--pin\": --pin is the value of names.FlagPin (names/names.go:14)",
            "use/use_test.go:8 [value] \"/v1/cluster\": the route is also at use/use.go:7",
        ]
    );
    assert_eq!(
        skipped,
        [
            "names/names_test.go:7 [label] \"read-error %v\": read-error is the value of names.ReadError (names/names.go:10)",
            "names/names_test.go:9 [case] \"drift\": drift is the value of names.Drift (names/names.go:9)",
            "names/names_test.go:10 [json] \"{\\\"kind\\\": \\\"read-error\\\"}\": read-error is the value of names.ReadError (names/names.go:10)",
        ]
    );
}

/// The Rust package of the tests of the check: closed sets of the three forms, a const, a route,
/// and a helper.
fn rust_package(repo: &Repo) {
    repo.write("Cargo.toml", "[package]\nname = \"p\"\n");
    repo.write(
        "src/lib.rs",
        "/// A closed set.\n#[derive(Serialize)]\n#[serde(rename_all = \"kebab-case\")]\npub enum Tier {\n    InPlace,\n}\n\n\
         impl_texts!(pub Format { Json => \"json-lines\" });\n\n\
         impl Fault {\n    pub fn kind(&self) -> &'static str {\n        match self {\n            Fault::Missing => \"not-found\",\n            Fault::Bad => \"bad-input\",\n        }\n    }\n}\n\n\
         pub const SCHEMA: &str = \"strata-x/1\";\n\n\
         pub fn route(uid: &str) -> String {\n    format!(\"/v1/packs/{uid}\")\n}\n\n\
         pub fn twice(text: &str) -> String {\n    let mut out = String::from(text);\n    out.push_str(text);\n    out\n}\n",
    );
}

/// The criterion of #929: each kind of finding of Rust, the consts and the helpers that a change
/// adds, and each place that is no finding.
#[test]
fn the_check_lists_each_copy_of_rust_and_each_const_and_helper_that_the_change_adds_again() {
    let repo = Repo::new();
    rust_package(&repo);
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "tests/t.rs",
        "#[test]\nfn t() {\n    assert_eq!(tier(), \"in-place\");\n    assert!(ok(), \"in-place\");\n\
         \x20   let doc = json!({\"tier\": \"in-place\"});\n    let s = \"strata-x/1\";\n\
         \x20   match kind() {\n        \"not-found\" => {}\n        _ => {}\n    }\n\
         \x20   call(\"json-lines\");\n    let r = \"/v1/packs/{}\";\n}\n",
    );
    repo.write(
        "src/more.rs",
        "pub const AGAIN: &str = \"strata-x/1\";\n\npub fn double(word: &str) -> String {\n    let mut out = String::from(word);\n    out.push_str(word);\n    out\n}\n",
    );
    repo.commit("change");

    let (findings, skipped) = repo.copies();
    assert_eq!(
        findings,
        [
            "src/more.rs:1 [const] AGAIN = \"strata-x/1\": the value of SCHEMA (src/lib.rs:19)",
            "src/more.rs:3 [helper] double: the body of twice (src/lib.rs:25)",
            "tests/t.rs:3 [compare] \"in-place\": in-place is the value of Tier::InPlace (src/lib.rs:5)",
            "tests/t.rs:6 [value] \"strata-x/1\": strata-x/1 is the value of AGAIN (src/more.rs:1), SCHEMA (src/lib.rs:19)",
            "tests/t.rs:8 [switch] \"not-found\": not-found is the value of Fault::Missing (src/lib.rs:13)",
            "tests/t.rs:11 [call:call] \"json-lines\": json-lines is the value of Format::Json (src/lib.rs:8)",
            "tests/t.rs:12 [value] \"/v1/packs/{}\": the route is also at src/lib.rs:22",
        ]
    );
    assert_eq!(
        skipped,
        [
            "tests/t.rs:4 [label] \"in-place\": in-place is the value of Tier::InPlace (src/lib.rs:5)",
            "tests/t.rs:5 [json] \"in-place\": in-place is the value of Tier::InPlace (src/lib.rs:5)",
        ]
    );
}

/// A value that the file of a literal cannot name is no copy: an unexported const of another
/// package, a const of a package that imports the package of the literal, and a const of a test
/// for code that is no test.
#[test]
fn a_value_that_the_file_of_the_literal_cannot_name_is_no_copy() {
    let repo = Repo::new();
    go_module(&repo);
    repo.write(
        "names/names_test.go",
        "package names\n\nconst inTest = \"in-test-only\"\n",
    );
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "use/more.go",
        "package use\n\nvar title = \"Lost contact\"\n",
    );
    repo.write("low/more.go", "package low\n\nvar pin = \"--pin\"\n");
    repo.write(
        "names/more.go",
        "package names\n\nvar mark = \"in-test-only\"\n",
    );
    repo.commit("change");
    let found = check(repo.path(), "main", "HEAD").unwrap();
    assert!(found.findings.is_empty(), "{:?}", found.findings);
    assert_eq!(found.exit_code(), 0);
    assert_eq!((found.lines, found.files), (9, 3));
}

/// The check reads the diff from the merge base, so a commit of the base after the branch adds
/// no line, and a line that the change does not add is no finding.
#[test]
fn the_check_reads_only_the_lines_that_the_change_adds_after_the_merge_base() {
    let repo = Repo::new();
    go_module(&repo);
    repo.write("use/old_test.go", "package use\n\nvar old = \"--pin\"\n");
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "use/old_test.go",
        "package use\n\nvar old = \"--pin\"\n\nvar fresh = \"--pin\"\n",
    );
    repo.commit("change");
    repo.to("main");
    repo.write(
        "use/later_test.go",
        "package use\n\nvar later = \"--pin\"\n",
    );
    repo.commit("later on main");
    repo.to("change");

    let found = check(repo.path(), "main", "HEAD").unwrap();
    let lines: Vec<String> = found
        .findings
        .iter()
        .map(|h| format!("{}:{}", h.path, h.line))
        .collect();
    assert_eq!(lines, ["use/old_test.go:5"]);
    assert_eq!(found.exit_code(), 1);
    assert!(check(repo.path(), "no-such-ref", "HEAD").is_err());
}

/// A new Go const and a new Go function with a copy in another line, and the helper of a
/// variable that names a function: its parameter `where` is a label.
#[test]
fn a_new_go_const_and_helper_with_a_copy_are_findings_and_an_alias_keeps_the_labels() {
    let repo = Repo::new();
    go_module(&repo);
    repo.write(
        "use/check_test.go",
        "package use\n\nfunc holds(t *testing.T, where, text string) {\n\tif !strings.Contains(text, where) {\n\t\tt.Fatal(where)\n\t}\n}\n\nvar mustHold = holds\n",
    );
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "use/new_test.go",
        "package use\n\nconst pinFlag = \"--pin\"\n\nfunc has(t *testing.T, label, page string) {\n\tif !strings.Contains(page, label) {\n\t\tt.Fatal(label)\n\t}\n}\n\nfunc TestX(t *testing.T) {\n\tmustHold(t, \"--pin\", \"x\")\n}\n",
    );
    repo.commit("change");
    let (findings, skipped) = repo.copies();
    assert_eq!(
        findings,
        [
            "use/new_test.go:3 [const] use.pinFlag = \"--pin\": the value of names.FlagPin (names/names.go:14)",
            "use/new_test.go:5 [helper] has: the body of holds (use/check_test.go:3)",
        ]
    );
    assert_eq!(
        skipped,
        [
            "use/new_test.go:12 [label] \"--pin\": --pin is the value of names.FlagPin (names/names.go:14), use.pinFlag (use/new_test.go:3)",
        ]
    );
}
