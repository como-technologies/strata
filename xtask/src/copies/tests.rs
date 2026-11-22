use std::collections::{BTreeMap, BTreeSet};

use super::keyword::Keyword;
use super::place::{Class, Context, Literal, literals};
use super::rust_sets::rename;
use super::token::Token;
use super::*;
use crate::files::TestLines;
use crate::test_repo::Repo;
use crate::variants::one_of_each;

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
    let tests = if test {
        TestLines::whole()
    } else {
        TestLines::default()
    };
    let context = Context {
        lang,
        labels,
        structs: &structs,
        tests: &tests,
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

/// The criterion of #960. The index of a value in a test reads a key of a document. So does the
/// key of a map literal. A list of a test and an index of other code stay rows.
#[test]
fn the_key_of_a_map_and_the_index_of_a_value_of_a_test_are_json_and_a_rust_json_macro_is_json() {
    for (lang, code) in [
        (
            Lang::Go,
            "package p\nvar doc = map[string]any{\"mark\": 1}\n",
        ),
        (
            Lang::Rust,
            "fn t() {\n    let doc = json!({\"kind\": [\"mark\"]});\n}\n",
        ),
        (
            Lang::Rust,
            "fn t() {\n    let x = manifests[\"mark\"][0];\n}\n",
        ),
        (Lang::Rust, "fn t() {\n    let x = doc[0][\"mark\"];\n}\n"),
        (Lang::Go, "package p\nfunc f() {\n\tx := doc[\"mark\"]\n}\n"),
    ] {
        assert_eq!(class_of_mark(lang, code, true), Class::Json, "{code}");
    }
    for (code, test) in [
        ("fn t() {\n    f(&[\"mark\"]);\n}\n", true),
        ("fn t() {\n    let x = vec![\"mark\"];\n}\n", true),
        ("fn f() {\n    let x = doc[\"mark\"];\n}\n", false),
    ] {
        assert_eq!(
            class_of_mark(Lang::Rust, code, test),
            Class::Row(None),
            "{code}"
        );
    }
}

/// The values of `pairs` that the literal of each case holds, each value with one holder, as the
/// rule of the literals reads them. A case is a literal and the values that it holds.
fn assert_held(pairs: &[&'static str], cases: &[(&str, &[&str])]) {
    let holders = values_of(pairs);
    let mut values: ByValue = BTreeMap::new();
    for holder in &holders {
        values
            .entry(holder.value.as_str())
            .or_default()
            .push(holder);
    }
    for (text, want) in cases {
        let found: Vec<&str> = hits::held(text, &values).into_iter().collect();
        assert_eq!(found, *want, "{text:?}");
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
    assert_held(
        &[
            "drift",
            "newer",
            "version",
            "DriftFound",
            "read-error",
            "--pin",
            "no answer",
        ],
        &[
            ("drift", &["drift"]),
            ("newer ", &["newer"]),
            ("drift DriftFound ", &["DriftFound", "drift"]),
            ("newer version", &[]),
            ("the drift of a pack", &[]),
            ("the cause read-error names %v", &["read-error"]),
            ("read-errors", &[]),
            ("behind record-0.json --pin ", &["--pin"]),
            ("--pinned", &[]),
            (" no answer ", &["no answer"]),
            ("reason=no answer", &[]),
        ],
    );
}

/// The criterion of #960. A dot between two characters of a word joins them, as in a host name.
/// A scheme of a URL counts only as the whole literal.
#[test]
fn a_part_of_a_host_name_and_the_scheme_of_a_url_are_no_copy() {
    assert_held(
        &["k8s", "www.", "https://", "profile.json", "1.1.0"],
        &[
            ("registry.k8s.io/pause:3.10", &[]),
            ("Values=sigs.k8s.io/cluster-api", &[]),
            ("os and k8s.", &["k8s"]),
            ("www.example.com/@b", &[]),
            ("a www. b", &["www."]),
            ("https://example.com/#12", &[]),
            ("https://", &["https://"]),
            ("dir/profile.json", &["profile.json"]),
            ("raw.profile.json", &[]),
            ("pack@1.1.0", &["1.1.0"]),
            ("1.1.0.1", &[]),
        ],
    );
}

/// The criterion of #960. A route is a copy only of the same route. So the route of a pack diff
/// of the observer is no copy of the route of the packs of Palette.
#[test]
fn a_route_is_a_copy_only_of_the_same_route() {
    assert_held(
        &["/v1/packs", "/v1/"],
        &[
            ("/v1/packs", &["/v1/packs"]),
            ("GET /v1/packs", &["/v1/packs"]),
            ("/v1/packs/wilma@1.0.0/addon/prometheus-operator/diff", &[]),
            ("the route /v1/packs answers", &[]),
            ("/v1/", &["/v1/"]),
        ],
    );
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

/// The criterion of #960 for a method. A word of capital letters and a space can come before a
/// route, so the check needs no list of the methods.
#[test]
fn a_route_drops_its_method_and_the_names_of_its_segments() {
    for (value, want) in [
        ("/v1/things/{uid}", Some("/v1/things/{}")),
        (
            "GET /v1/things/{name}/parts/{uid}",
            Some("/v1/things/{}/parts/{}"),
        ),
        ("DELETE /v1/things/{uid}", Some("/v1/things/{}")),
        ("HEAD /v1/things", Some("/v1/things")),
        ("/v1/clusters/%s/x", Some("/v1/clusters/{}/x")),
        ("get /v1/things", None),
        ("the /v1/things", None),
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
    let keyword = Keyword::If.as_str();
    assert_eq!(marks, ["x", ":=", "f", "()", ";", keyword, "y", "()", ";"]);
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
    assert!(
        file.code
            .iter()
            .all(|t| !t.is_word(Keyword::Import.as_str()))
    );
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
        .map(|v| {
            let test = file.tests.holds(v.line);
            (v.name.as_str(), v.value.as_str(), v.closed, v.scope, test)
        })
        .collect();
    assert_eq!(
        values,
        [
            ("Tier::InPlace", "in-place", true, Scope::Package, false),
            ("Tier::NewNode", "new-node", true, Scope::Package, false),
            ("Tier::NewNode", "node", true, Scope::Package, false),
            ("Format::Json", "json-lines", true, Scope::Parent, false),
            ("Format::Text", "text", true, Scope::Parent, false),
            ("Fault::Missing", "not-found", true, Scope::Crate, false),
            ("Fault::Bad", "bad-input", true, Scope::Crate, false),
            ("Fault::Worse", "bad-input", true, Scope::Crate, false),
            ("SCHEMA", "strata-x/1", false, Scope::Module, false),
            ("NAMES", "a-1", false, Scope::Package, false),
            ("NAMES", "b-2", false, Scope::Package, false),
            ("LANGS", "golang", false, Scope::Package, false),
            ("Tier::InPlace", "in place", true, Scope::Package, false),
            ("Tier::NewNode", "new node", true, Scope::Package, false),
            ("IN_TEST", "in-test", false, Scope::Module, true),
        ]
    );
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

/// The criterion of #960. The compiler refuses a table whose visibility does not parse, so the read
/// gives the error of the parse, with its line.
#[test]
fn a_table_whose_visibility_does_not_parse_is_an_error_of_the_read() {
    let error =
        rust::read("fn a() {}\nimpl_texts!(pub(in 3) Format { Json => \"json-lines\" });\n")
            .unwrap_err();
    assert!(error.starts_with("line 2: impl_texts!: "), "{error}");
    assert!(
        rust::read("impl_texts!(pub(in crate::a) Format { Json => \"json-lines\" });\n").is_ok()
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
        // the integration tests are a crate of their own
        ("src/a.rs", Scope::Crate, "src/b/c.rs", true, true),
        ("src/a.rs", Scope::Crate, "tests/cli.rs", true, false),
        (
            "tests/common/mod.rs",
            Scope::Crate,
            "tests/cli.rs",
            true,
            true,
        ),
        (
            "tests/common/mod.rs",
            Scope::Package,
            "src/a.rs",
            true,
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
    // the integration tests of a package in a directory of the repository
    let nested = reach::rust_reach("xtask/src/a.rs", "xtask/", &roots, Scope::Crate, false);
    assert!(!nested.holds("xtask/tests/t.rs", true));
    assert!(nested.holds("xtask/src/b.rs", false));
}

/// The findings and the places that are no copy of the change from `main` to `HEAD` in `repo`, each
/// one as the report prints it.
fn copies(repo: &Repo) -> (Vec<String>, Vec<String>) {
    let found = check(repo.path(), "main", "HEAD").unwrap();
    let lines = |hits: &[Hit]| hits.iter().map(Hit::to_string).collect::<Vec<_>>();
    (lines(&found.findings), lines(&found.skipped))
}

/// The Go module of the tests of the check. It has a package `names` of closed sets, a package
/// `use` that imports it, and a package `low` that `names` imports.
fn go_module(repo: &Repo) {
    repo.write(Lang::Go.root_file(), "module example.com/m\n\ngo 1.22\n");
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

    let (findings, skipped) = copies(&repo);
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
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
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

/// The criterion of #929: each kind of finding of Rust, and each place that is no finding. The
/// change also adds a const and a helper that are copies.
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

    let (findings, skipped) = copies(&repo);
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

/// The criterion of #944. git writes the name `src/b é.rs` in quotes and with a tab at its end.
/// The check reads the file, and names it as the tree names it.
#[test]
fn the_check_reads_a_file_with_a_space_and_a_character_that_is_not_ascii_in_its_name() {
    let repo = Repo::new();
    rust_package(&repo);
    repo.commit("base");
    repo.branch("change");
    repo.write("src/b é.rs", "pub const AGAIN: &str = \"strata-x/1\";\n");
    repo.commit("change");

    let (findings, skipped) = copies(&repo);
    assert_eq!(
        findings,
        ["src/b é.rs:1 [const] AGAIN = \"strata-x/1\": the value of SCHEMA (src/lib.rs:19)"]
    );
    assert!(skipped.is_empty(), "{skipped:?}");
}

/// A value that the file of a literal cannot name is no copy. The test has an unexported const of
/// another package, and a const of a package that imports the package of the literal. It also has
/// a const of a test, for code that is no test.
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
/// no line. A line that the change does not add is no finding.
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

/// A new Go const and a new Go function with a copy in another line are findings. A variable
/// that names a function takes its labels: the argument for its parameter `where` is a label.
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
    let (findings, skipped) = copies(&repo);
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

/// A new const and a new function are copies of a private item of another module. Neither of the
/// two items can name the other. The test has them in Rust and in Go.
#[test]
fn a_new_const_and_helper_copy_a_private_item_of_another_module() {
    let repo = Repo::new();
    go_module(&repo);
    repo.write(
        "low/join.go",
        "package low\n\nfunc join(x, y string) string {\n\treturn strings.Join([]string{x, y}, \" \")\n}\n",
    );
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
    repo.write(
        "src/old.rs",
        "const MARK: &str = \"strata-y/2\";\n\nfn twice(text: &str) -> String {\n    let mut out = String::from(text);\n    out.push_str(text);\n    out\n}\n\n\
         fn mark() -> &'static str {\n    \"strata-y/2\"\n}\n",
    );
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "use/more.go",
        "package use\n\nconst heading = \"Lost contact\"\n\nfunc joined(a, b string) string {\n\treturn strings.Join([]string{a, b}, \" \")\n}\n",
    );
    repo.write(
        "src/new.rs",
        "const AGAIN: &str = \"strata-y/2\";\n\nfn double(word: &str) -> String {\n    let mut out = String::from(word);\n    out.push_str(word);\n    out\n}\n",
    );
    repo.commit("change");
    let (findings, _) = copies(&repo);
    assert_eq!(
        findings,
        [
            "src/new.rs:1 [const] AGAIN = \"strata-y/2\": the value of MARK (src/old.rs:1); the literal at src/old.rs:10 [return]",
            "src/new.rs:3 [helper] double: the body of twice (src/old.rs:3)",
            "use/more.go:3 [const] use.heading = \"Lost contact\": the value of names.title (names/names.go:15)",
            "use/more.go:5 [helper] joined: the body of join (low/join.go:3)",
        ]
    );
}

/// A sibling module returns four texts of a module. They come from `impl_texts!` with no
/// visibility, `impl_texts!` with `pub(self)`, a private const, and `impl_closed_set!`.
#[test]
fn a_table_with_no_visibility_reaches_its_module_and_a_table_of_a_trait_reaches_the_package() {
    let repo = Repo::new();
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
    repo.write(
        "src/a.rs",
        "impl_texts!(Pack { One => \"pack-one-x\" });\n\nimpl_texts!(pub(self) Other { Two => \"pack-two-x\" });\n\n\
         const THREE: &str = \"pack-three-x\";\n\nimpl_closed_set!(Kind { Four => \"pack-four-x\" });\n",
    );
    repo.commit("base");
    repo.branch("change");
    repo.write(
        "src/b.rs",
        "fn one() -> &'static str {\n    \"pack-one-x\"\n}\n\nfn two() -> &'static str {\n    \"pack-two-x\"\n}\n\n\
         fn three() -> &'static str {\n    \"pack-three-x\"\n}\n\nfn four() -> &'static str {\n    \"pack-four-x\"\n}\n",
    );
    repo.commit("change");
    let (findings, _) = copies(&repo);
    assert_eq!(
        findings,
        [
            "src/b.rs:14 [return] \"pack-four-x\": pack-four-x is the value of Kind::Four (src/a.rs:7)"
        ]
    );
}

/// The criterion of #960. The table of `impl_closed_set!` and the texts of a `Display` reach where
/// their type reaches. The enum `Hidden` is private, so a child module can name its texts, and a
/// sibling module cannot. The enum `Shown` is `pub`. The file does not declare `Elsewhere`, so its
/// texts reach the package.
#[test]
fn the_texts_of_a_trait_reach_where_their_type_reaches() {
    let repo = Repo::new();
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
    repo.write(
        "src/a.rs",
        "enum Hidden {\n    One,\n    Two,\n}\n\n\
         impl_closed_set!(Hidden { One => \"hidden-one-x\", Two => \"hidden-two-x\" });\n\n\
         impl fmt::Display for Hidden {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        match self {\n\
         \x20           Hidden::One => f.write_str(\"Hidden-One\"),\n            Hidden::Two => f.write_str(\"Hidden-Two\"),\n\
         \x20       }\n    }\n}\n\n\
         pub enum Shown {\n    Three,\n}\n\nimpl_closed_set!(Shown { Three => \"shown-three-x\" });\n\n\
         impl_closed_set!(Elsewhere { Four => \"elsewhere-four-x\" });\n",
    );
    repo.commit("base");
    repo.branch("change");
    let row = "[\"hidden-one-x\", \"Hidden-Two\", \"shown-three-x\", \"elsewhere-four-x\"]";
    repo.write(
        "src/a/c.rs",
        &format!("pub fn texts() -> [&'static str; 4] {{\n    {row}\n}}\n"),
    );
    repo.write(
        "src/b.rs",
        &format!("pub fn words() -> Vec<&'static str> {{\n    {row}.to_vec()\n}}\n"),
    );
    repo.commit("change");
    let (findings, _) = copies(&repo);
    let places: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding.split(": ").next())
        .collect();
    assert_eq!(
        places,
        [
            "src/a/c.rs:2 [row] \"Hidden-Two\"",
            "src/a/c.rs:2 [row] \"elsewhere-four-x\"",
            "src/a/c.rs:2 [row] \"hidden-one-x\"",
            "src/a/c.rs:2 [row] \"shown-three-x\"",
            "src/b.rs:2 [row] \"elsewhere-four-x\"",
            "src/b.rs:2 [row] \"shown-three-x\"",
        ],
        "{findings:#?}"
    );
}

/// The criterion of #960. The integration tests in `tests/` are a crate of their own. So they
/// cannot name a `pub(crate)` item of the library, and they can name a `pub` item. A unit test of
/// the library cannot name a `pub(crate)` item of the integration tests.
#[test]
fn a_pub_crate_item_of_the_library_does_not_reach_the_integration_tests() {
    let repo = Repo::new();
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
    repo.write(
        "src/lib.rs",
        "pub(crate) const INNER: &str = \"inner-value-x\";\n\npub const OUTER: &str = \"outer-value-x\";\n\n\
         pub(crate) fn word(status: Status) -> &'static str {\n    match status {\n\
         \x20       Status::Done => \"word-done-x\",\n        Status::Open => \"word-open-x\",\n    }\n}\n",
    );
    repo.write(
        "tests/common/mod.rs",
        "pub(crate) const SHARED: &str = \"shared-value-x\";\n",
    );
    repo.commit("base");
    repo.branch("change");
    let texts = "[\"inner-value-x\", \"outer-value-x\", \"word-done-x\", \"shared-value-x\"]";
    repo.write(
        "tests/t.rs",
        &format!("#[test]\nfn t() {{\n    let texts = {texts};\n}}\n"),
    );
    repo.write(
        "src/more.rs",
        &format!(
            "#[cfg(test)]\nmod tests {{\n    fn t() {{\n        let words = {texts};\n    }}\n}}\n"
        ),
    );
    repo.commit("change");
    let (findings, _) = copies(&repo);
    let places: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding.split(": ").next())
        .collect();
    assert_eq!(
        places,
        [
            "src/more.rs:4 [row] \"inner-value-x\"",
            "src/more.rs:4 [row] \"outer-value-x\"",
            "src/more.rs:4 [row] \"word-done-x\"",
            "tests/t.rs:3 [row] \"outer-value-x\"",
            "tests/t.rs:3 [row] \"shared-value-x\"",
        ],
        "{findings:#?}"
    );
}

/// The criterion of #960. git can hold a file name that is not UTF-8. The check gives an error
/// that names such a file when it reads the file, as `just comment-words` does. It reads no other
/// file, so another such name is out.
#[test]
fn a_file_name_that_is_not_utf_8_is_an_error_when_the_check_reads_the_file() {
    let listing = b"src/a.rs\0docs/caf\xe9.md\0go.mod\0";
    assert_eq!(
        source::tree_paths(listing).unwrap(),
        ["src/a.rs", Lang::Go.root_file()]
    );
    for name in [&b"src/caf\xe9.rs"[..], b"x\xe9/Cargo.toml", b"caf\xe9.go"] {
        let listing = [&b"src/a.rs\0"[..], name, b"\0"].concat();
        let error = source::tree_paths(&listing).unwrap_err();
        assert_eq!(
            error,
            format!(
                "git ls-tree: the file name {} is not UTF-8",
                name.escape_ascii()
            )
        );
    }
}

/// The criterion of #960 in a repository: the base holds a Rust file whose name is not UTF-8, and
/// the change adds another file. The diff does not name the file, and the tree does.
#[cfg(target_os = "linux")]
#[test]
fn the_check_names_a_rust_file_of_the_tree_whose_name_is_not_utf_8() {
    use std::os::unix::ffi::OsStrExt;

    let repo = Repo::new();
    rust_package(&repo);
    let name = std::ffi::OsStr::from_bytes(b"src/caf\xe9.rs");
    std::fs::write(repo.path().join(name), "pub const X: &str = \"x\";\n").unwrap();
    repo.commit("base");
    repo.branch("change");
    repo.write("src/more.rs", "pub const AGAIN: &str = \"strata-x/1\";\n");
    repo.commit("change");
    assert_eq!(
        check(repo.path(), "main", "HEAD").unwrap_err(),
        "git ls-tree: the file name src/caf\\xe9.rs is not UTF-8"
    );
}

/// Each string of a const array is a const. The change adds a string const and a table with the
/// value of an older literal, and two arrays with one value.
#[test]
fn each_string_of_a_const_array_is_a_const_that_the_check_reads() {
    let repo = Repo::new();
    repo.write(Lang::Rust.root_file(), "[package]\nname = \"p\"\n");
    repo.write(
        "src/size.rs",
        "pub fn count() -> usize {\n    files_under(\"src\", \"rs-ext\")\n}\n",
    );
    repo.commit("base");
    repo.branch("change");
    repo.write("src/one.rs", "pub const EXT: &str = \"rs-ext\";\n");
    repo.write(
        "src/langs.rs",
        "pub const LANGS: [(Lang, &str); 1] = [(Lang::Rust, \"rs-ext\")];\n\
         const DECLARATIONS: [&str; 2] = [\"package-kw\", \"func-kw\"];\n\
         const NO_SEMICOLON_AFTER: [&str; 2] = [\"func-kw\", \"struct-kw\"];\n",
    );
    repo.commit("change");
    let (findings, _) = copies(&repo);
    assert_eq!(
        findings,
        [
            "src/langs.rs:1 [const] LANGS = \"rs-ext\": the value of EXT (src/one.rs:1); the literal at src/size.rs:2 [call:files_under]",
            "src/langs.rs:2 [const] DECLARATIONS = \"func-kw\": the value of NO_SEMICOLON_AFTER (src/langs.rs:3)",
            "src/langs.rs:3 [const] NO_SEMICOLON_AFTER = \"func-kw\": the value of DECLARATIONS (src/langs.rs:2)",
            "src/one.rs:1 [const] EXT = \"rs-ext\": the value of LANGS (src/langs.rs:1); the literal at src/size.rs:2 [call:files_under]",
        ]
    );
}

#[test]
fn a_literal_in_the_pattern_of_an_arm_is_a_case_of_a_switch() {
    let code = "fn f(k: Option<&str>) {\n    match k {\n        Some(\"mark\") => {}\n        _ => {}\n    }\n}\n";
    assert_eq!(class_of_mark(Lang::Rust, code, false), Class::Switch);
}

#[test]
fn the_statement_of_a_go_block_gives_the_class_of_each_literal_after_the_first() {
    for (code, class) in [
        (
            "package p\nfunc f() {\n\ta, b := \"x\", \"mark\"\n}\n",
            Class::Value,
        ),
        (
            "package p\nfunc f() (string, string) {\n\treturn \"x\", \"mark\"\n}\n",
            Class::Return,
        ),
        (
            "package p\nfunc f(k string) {\n\tswitch k {\n\tcase \"x\", \"mark\":\n\t}\n}\n",
            Class::Switch,
        ),
        ("package p\nvar a, b = \"x\", \"mark\"\n", Class::Value),
    ] {
        assert_eq!(class_of_mark(Lang::Go, code, false), class, "{code}");
    }
}
