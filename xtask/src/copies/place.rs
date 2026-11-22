//! The place of each string literal of a file: the class that says what the literal does there.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::go::struct_fields;
use super::go_lex::STRUCT;
use super::token::{Delim, Token, split};
use super::{Lang, holds_line};

/// The names of a field or of a parameter that holds the name of a case of a test.
const CASE_NAMES: [&str; 4] = ["name", "case", "desc", "title"];

/// The names of a parameter or of a field that holds the label of a failure, such as `where` of
/// `testkit.HoldsAll`.
pub const LABEL_NAMES: [&str; 6] = ["where", "label", "what", "context", "msg", "message"];

/// The methods of Go that fail or log a test with a message. Each argument is a label.
const GO_FAILURES: [&str; 8] = [
    "Error", "Errorf", "Fatal", "Fatalf", "Log", "Logf", "Skip", "Skipf",
];

/// The packages of Go whose functions of [`GO_FAILURES`] build an error or a log line, and fail
/// no test.
const GO_NOT_TESTS: [&str; 3] = ["fmt", "errors", "log"];

/// The macros and methods of Rust whose arguments are each a label.
const RUST_FAILURES: [&str; 5] = [
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    "expect",
];

/// The macros of Rust whose arguments after the first are a label.
const RUST_ASSERTS: [&str; 2] = ["assert!", "debug_assert!"];

/// The macros of Rust that compare their first two arguments. The arguments after them are a label.
const RUST_COMPARES: [&str; 4] = [
    "assert_eq!",
    "assert_ne!",
    "debug_assert_eq!",
    "debug_assert_ne!",
];

/// The keywords that stop the read of an operand, with the class of a literal after each one. A
/// literal after `case` is a case of a `switch`, and a literal after `return` is its value.
const KEYWORDS: [(&str, Option<Class>); 11] = [
    ("case", Some(Class::Switch)),
    ("return", Some(Class::Return)),
    ("if", None),
    ("else", None),
    ("let", None),
    ("match", None),
    ("const", None),
    ("var", None),
    ("static", None),
    ("range", None),
    ("default", None),
];

/// The two keywords of a Rust `for` loop: `for PATTERN in ROWS`.
const FOR_IN: [&str; 2] = ["for", "in"];

/// True for a keyword of [`KEYWORDS`] or of [`FOR_IN`]: no operand holds it.
fn is_keyword(word: &str) -> bool {
    KEYWORDS.iter().any(|(keyword, _)| *keyword == word) || FOR_IN.contains(&word)
}

/// The class of a literal after the keyword `word`, or `None` for another word.
fn after_keyword(word: &str) -> Option<Class> {
    KEYWORDS
        .iter()
        .find(|(keyword, _)| *keyword == word)
        .and_then(|(_, class)| class.clone())
}

/// The class of the place of a string literal.
///
/// [`Class::is_finding`] is false for the input of a test, the name of a case, the label of a
/// failure, and the tag of a field. A value of a closed set there is no copy.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// A JSON or a JS input of a test, or a key of a map literal of a test.
    Json,
    /// A YAML file of rules of a test.
    Rules,
    /// A shell script of a test, such as a stand-in for a command.
    Script,
    /// The name of a case: an argument of `t.Run`, or a field such as `name` of a row.
    Case,
    /// The label of a failure: an argument of `t.Errorf` or of `panic!`, the message of
    /// `assert!`, or an argument for a parameter such as `where`.
    Label,
    /// The tag of a field of a Go struct.
    Tag,
    /// An operand of `==` or of `!=`, or of `assert_eq!`.
    Compare,
    /// A case of a `switch`, or a pattern of a `match`.
    Switch,
    /// The value of a declaration or of an assignment.
    Value,
    /// The value of a `return`.
    Return,
    /// An argument of the call of a function or of a macro.
    Call(String),
    /// A field of a struct literal.
    Field(String),
    /// An element of a row of a table or of a list, with the name of its field when the scan
    /// knows it.
    Row(Option<String>),
    /// Another place.
    Other,
}

impl Class {
    /// False for a place where a value of a closed set is no copy.
    pub fn is_finding(&self) -> bool {
        !matches!(
            self,
            Class::Json | Class::Rules | Class::Script | Class::Case | Class::Label | Class::Tag
        )
    }
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Class::Json => f.write_str("json"),
            Class::Rules => f.write_str("rules"),
            Class::Script => f.write_str("script"),
            Class::Case => f.write_str("case"),
            Class::Label => f.write_str("label"),
            Class::Tag => f.write_str("tag"),
            Class::Compare => f.write_str("compare"),
            Class::Switch => f.write_str("switch"),
            Class::Value => f.write_str("value"),
            Class::Return => f.write_str("return"),
            Class::Call(name) => write!(f, "call:{name}"),
            Class::Field(name) => write!(f, "field:{name}"),
            Class::Row(Some(name)) => write!(f, "row:{name}"),
            Class::Row(None) => f.write_str("row"),
            Class::Other => f.write_str("other"),
        }
    }
}

/// What the walk of one file knows of its language and of its package.
pub struct Context<'a> {
    pub lang: Lang,
    /// For each function of the tree by name, the positions of its parameters of [`LABEL_NAMES`].
    pub labels: &'a BTreeMap<String, BTreeSet<usize>>,
    /// The fields of each struct type of the package of the file, by name. Go only.
    pub structs: &'a BTreeMap<String, Vec<String>>,
    /// The first and the last line of each part of the file that is test code.
    pub tests: &'a [(usize, usize)],
}

/// A string literal of a file, and the class of its place.
#[derive(Debug, PartialEq, Eq)]
pub struct Literal {
    pub value: String,
    pub line: usize,
    pub last: usize,
    pub class: Class,
}

/// A group on the path from the root of a file to a literal.
struct Frame<'a> {
    delim: Option<Delim>,
    tokens: &'a [Token],
    /// The position of the group in the tokens of the frame before it.
    at: usize,
}

/// Each string literal of `tokens`, with the class of its place.
pub fn literals(tokens: &[Token], context: &Context) -> Vec<Literal> {
    let mut out = Vec::new();
    let mut frames = vec![Frame {
        delim: None,
        tokens,
        at: 0,
    }];
    walk(&mut frames, context, &mut out);
    out
}

fn walk<'a>(frames: &mut Vec<Frame<'a>>, context: &Context, out: &mut Vec<Literal>) {
    let Some(tokens) = frames.last().map(|frame| frame.tokens) else {
        return;
    };
    for (at, token) in tokens.iter().enumerate() {
        match token {
            Token::Text { value, line, last } => out.push(Literal {
                value: value.clone(),
                line: *line,
                last: *last,
                class: classify(frames, at, context, value, *line),
            }),
            Token::Group { delim, tokens, .. } => {
                frames.push(Frame {
                    delim: Some(*delim),
                    tokens,
                    at,
                });
                walk(frames, context, out);
                frames.pop();
            }
            _ => {}
        }
    }
}

fn classify(frames: &[Frame], at: usize, context: &Context, value: &str, line: usize) -> Class {
    let test = holds_line(context.tests, line);
    if test && is_json(value) {
        return Class::Json;
    }
    if test && is_rules(value) {
        return Class::Rules;
    }
    if test && is_script(value) {
        return Class::Script;
    }
    if test && is_key(frames, at) {
        return Class::Json;
    }
    if context.lang == Lang::Go && is_tag(frames) {
        return Class::Tag;
    }
    if let Some(class) = around(frames, at, context, test) {
        return class;
    }
    near(frames, at, context)
}

/// True for the text of a JSON document or of a JS object: a key in quotes, or a start such as
/// `{"` or `[{`. A format such as `{name} drift` is no JSON.
fn is_json(value: &str) -> bool {
    let start = value.trim_start();
    let opens = |open: char| {
        start.strip_prefix(open).is_some_and(|rest| {
            rest.is_empty()
                || rest.starts_with(['"', '}', ']', '{', '['])
                || rest.starts_with(char::is_whitespace)
        })
    };
    value.contains("\":") || value.contains("[{") || opens('{') || opens('[')
}

/// True for a YAML document of two lines or more with a key, such as a file of rules.
fn is_rules(value: &str) -> bool {
    value.lines().count() > 1
        && value.lines().any(|line| {
            let line = line.trim_start().trim_start_matches("- ");
            line.split_once(':').is_some_and(|(key, rest)| {
                !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    && (rest.is_empty() || rest.starts_with(' '))
            })
        })
}

/// True for a part of a shell script: a first line `#!`, the `then` of an `if`, or the end of an
/// `if` or of a `case`.
fn is_script(value: &str) -> bool {
    value.starts_with("#!")
        || ["; then", "\nfi", "esac"]
            .iter()
            .any(|mark| value.contains(mark))
}

/// True for the key of an element of a map literal: `"key": value`. A test builds a JSON document
/// with such keys.
fn is_key(frames: &[Frame], at: usize) -> bool {
    let Some(frame) = frames.last().filter(|f| f.delim == Some(Delim::Brace)) else {
        return false;
    };
    let starts = at == 0 || frame.tokens[at - 1].is_mark(",") || frame.tokens[at - 1].is_mark(";");
    starts && frame.tokens.get(at + 1).is_some_and(|t| t.is_mark(":"))
}

/// True when the innermost group is the body of a Go struct type: a literal there is a tag.
fn is_tag(frames: &[Frame]) -> bool {
    match frames {
        [.., parent, inner] if inner.delim == Some(Delim::Brace) => inner
            .at
            .checked_sub(1)
            .and_then(|before| parent.tokens.get(before))
            .is_some_and(|t| t.is_word(STRUCT)),
        _ => false,
    }
}

/// The name of the function or of the macro that a group of arguments follows: `name(` or
/// `name!(`. A keyword before the group names none.
fn callee(before: &[Token]) -> Option<String> {
    match before {
        [.., name, bang] if bang.is_mark("!") => Some(format!("{}!", name.word()?)),
        [.., name] => name.word().filter(|w| !is_keyword(w)).map(str::to_string),
        _ => None,
    }
}

/// The function or the macro that the group `frames[k]` gives its arguments to. A group of
/// another bracket is a call only of a macro: `Type{...}` of Go is no call.
fn call_of(frames: &[Frame], k: usize) -> Option<String> {
    let before = &frames.get(k.checked_sub(1)?)?.tokens[..frames.get(k)?.at];
    let name = callee(before)?;
    (frames[k].delim == Some(Delim::Paren) || name.ends_with('!')).then_some(name)
}

/// The word before the `.` of a method call, such as `t` of `t.Errorf(`.
fn receiver(before: &[Token]) -> Option<&str> {
    match before {
        [.., receiver, dot, _] if dot.is_mark(".") => receiver.word(),
        _ => None,
    }
}

/// The position of the argument at `at` among the arguments of `tokens`.
fn argument(tokens: &[Token], at: usize) -> usize {
    tokens[..at].iter().filter(|t| t.is_mark(",")).count()
}

/// The class that a call around the literal gives it: a label, a case, or a JSON input of a test.
/// The calls are read from the inside out.
fn around(frames: &[Frame], at: usize, context: &Context, test: bool) -> Option<Class> {
    for k in (1..frames.len()).rev() {
        let inside = if k + 1 == frames.len() {
            at
        } else {
            frames[k + 1].at
        };
        let Some(name) = call_of(frames, k) else {
            continue;
        };
        let before = &frames[k - 1].tokens[..frames[k].at];
        let arg = argument(frames[k].tokens, inside);
        if test && name == "json!" {
            return Some(Class::Json);
        }
        let label = match context.lang {
            Lang::Go => {
                GO_FAILURES.contains(&name.as_str())
                    && receiver(before).is_some_and(|r| !GO_NOT_TESTS.contains(&r))
            }
            Lang::Rust => {
                RUST_FAILURES.contains(&name.as_str())
                    || (RUST_ASSERTS.contains(&name.as_str()) && arg >= 1)
                    || (RUST_COMPARES.contains(&name.as_str()) && arg >= 2)
            }
        };
        if label
            || context
                .labels
                .get(&name)
                .is_some_and(|args| args.contains(&arg))
        {
            return Some(Class::Label);
        }
        if context.lang == Lang::Go && name == "Run" && arg == 0 && receiver(before).is_some() {
            return Some(Class::Case);
        }
    }
    None
}

/// The class that the tokens near the literal give it, in its own group.
fn near(frames: &[Frame], at: usize, context: &Context) -> Class {
    let Some(frame) = frames.last() else {
        return Class::Other;
    };
    let tokens = frame.tokens;
    if frame.delim == Some(Delim::Brace)
        && let Some(field) = keyed(tokens, at)
    {
        return by_name(field, Class::Field(field.to_string()));
    }
    let left = edge(tokens, at, true);
    let right = edge(tokens, at, false);
    let mark = |token: Option<&Token>, marks: &[&str]| {
        token.is_some_and(|t| marks.iter().any(|m| t.is_mark(m)))
    };
    if mark(left, &["==", "!="]) || mark(right, &["==", "!="]) {
        return Class::Compare;
    }
    if let Some(class) = left.and_then(Token::word).and_then(after_keyword) {
        return class;
    }
    if mark(right, &["=>"]) {
        return Class::Switch;
    }
    if mark(left, &["=", ":="]) {
        return Class::Value;
    }
    // a pattern of a `match`, such as `Some("x") =>`
    let parent = frames.len().checked_sub(2).and_then(|k| frames.get(k));
    if parent.is_some_and(|p| p.tokens.get(frame.at + 1).is_some_and(|t| t.is_mark("=>"))) {
        return Class::Switch;
    }
    let call = call_of(frames, frames.len() - 1);
    let arg = argument(tokens, at);
    let rust_block = context.lang == Lang::Rust && frame.delim == Some(Delim::Brace);
    match call.as_deref() {
        Some(name) if RUST_COMPARES.contains(&name) && arg < 2 => Class::Compare,
        Some("matches!") if arg > 0 => Class::Switch,
        Some(name) if name != "vec!" => Class::Call(name.to_string()),
        // the last expression of a Rust block gives the value of the block
        _ if rust_block && (left.is_none() || mark(left, &[";"])) => {
            match tokens[at..].iter().any(|t| t.is_mark(";")) {
                true => Class::Other,
                false => Class::Return,
            }
        }
        _ if rust_block || frame.delim.is_none() => Class::Other,
        _ => match row_field(frames, at, context) {
            Some(field) => by_name(&field, Class::Row(Some(field.clone()))),
            None => Class::Row(None),
        },
    }
}

/// The class of a field or of an element named `name`: a case or a label for their names, and
/// `otherwise` for another name.
fn by_name(name: &str, otherwise: Class) -> Class {
    if CASE_NAMES.contains(&name) {
        Class::Case
    } else if LABEL_NAMES.contains(&name) {
        Class::Label
    } else {
        otherwise
    }
}

/// The name of the field of a struct literal whose value holds the token at `at`: `name: "x"`.
fn keyed(tokens: &[Token], at: usize) -> Option<&str> {
    let start = tokens[..at]
        .iter()
        .rposition(|t| t.is_mark(",") || t.is_mark(";"))
        .map_or(0, |i| i + 1);
    match &tokens[start..at] {
        [name, colon, ..] if colon.is_mark(":") => name.word(),
        _ => None,
    }
}

/// The token that ends the operand of the literal at `at`, to the left or to the right. A word,
/// a literal, a group, and the marks `+`, `.`, `::`, `&`, `|`, and `!` are parts of the operand.
fn edge(tokens: &[Token], at: usize, left: bool) -> Option<&Token> {
    let mut i = at;
    loop {
        i = if left { i.checked_sub(1)? } else { i + 1 };
        let token = tokens.get(i)?;
        let part = match token {
            Token::Word { text, .. } => !is_keyword(text),
            Token::Text { .. } | Token::Other { .. } | Token::Group { .. } => true,
            Token::Mark { text, .. } => matches!(text.as_str(), "+" | "." | "::" | "&" | "|" | "!"),
        };
        if !part {
            return Some(token);
        }
    }
}

/// The name of the field of the element at `at` of a row, when the scan knows it.
fn row_field(frames: &[Frame], at: usize, context: &Context) -> Option<String> {
    let frame = frames.last()?;
    let index = argument(frame.tokens, at);
    match context.lang {
        Lang::Go => go_fields(frames, context)?.get(index).cloned(),
        Lang::Rust => rust_names(frames, index),
    }
}

/// The fields of the Go composite literal of the innermost group: of its type, such as `row{...}`,
/// or of the element type of the table around it, such as `[]struct{...}{ {...} }`.
fn go_fields(frames: &[Frame], context: &Context) -> Option<Vec<String>> {
    let n = frames.len();
    let (frame, parent) = (frames.get(n - 1)?, frames.get(n.checked_sub(2)?)?);
    if frame.delim != Some(Delim::Brace) {
        return None;
    }
    if let Some(fields) = fields_of_type(&parent.tokens[..frame.at], context) {
        return Some(fields);
    }
    // an element of a table: the table follows its type, such as `[]row`
    let element = frame.at == 0
        || parent.tokens[frame.at - 1].is_mark(",")
        || parent.tokens[frame.at - 1].is_mark(":");
    let grand = frames.get(n.checked_sub(3)?)?;
    if parent.delim != Some(Delim::Brace) || !element {
        return None;
    }
    fields_of_type(&grand.tokens[..parent.at], context)
}

/// The fields of the type that `before` ends with: `struct {...}`, `Name`, `pkg.Name`, or the
/// element of `[]Name` or of `map[K]Name`. A type that the package does not declare gives none.
fn fields_of_type(before: &[Token], context: &Context) -> Option<Vec<String>> {
    match before {
        [.., keyword, body] if keyword.is_word(STRUCT) => {
            Some(struct_fields(body.group(Delim::Brace)?))
        }
        [.., name] => {
            let name = name.word().filter(|w| !is_keyword(w))?;
            context.structs.get(name).cloned()
        }
        _ => None,
    }
}

/// The name that the pattern of a Rust `for` gives the element at `index` of a row: `for (name,
/// want) in [("a", 1)]`, or `for name in ["a"]`.
fn rust_names(frames: &[Frame], index: usize) -> Option<String> {
    let n = frames.len();
    let frame = frames.get(n - 1)?;
    // a tuple of an array, or an element of the array itself
    let (array, outer, tuple) = match frame.delim {
        Some(Delim::Paren) => (
            frames.get(n.checked_sub(2)?)?,
            frames.get(n.checked_sub(3)?)?,
            true,
        ),
        Some(Delim::Bracket) => (frame, frames.get(n.checked_sub(2)?)?, false),
        _ => return None,
    };
    if array.delim != Some(Delim::Bracket) {
        return None;
    }
    let before = &outer.tokens[..array.at];
    let before = match before {
        [rest @ .., vec, bang] if vec.is_word("vec") && bang.is_mark("!") => rest,
        [rest @ .., and] if and.is_mark("&") => rest,
        _ => before,
    };
    let [.., keyword, pattern, within] = before else {
        return None;
    };
    let [for_word, in_word] = FOR_IN;
    if !keyword.is_word(for_word) || !within.is_word(in_word) {
        return None;
    }
    let last_word = |part: &[Token]| part.iter().rev().find_map(Token::word).map(str::to_string);
    match (pattern.group(Delim::Paren), tuple) {
        (Some(parts), true) => split(parts, ",").get(index).and_then(|p| last_word(p)),
        (None, false) => pattern.word().map(str::to_string),
        _ => None,
    }
}
