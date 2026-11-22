//! What a Go file declares, for the scan: its package, its imports, its consts, its functions, and
//! the fields of its structs.

use super::go_lex::{self, MAP, STRUCT};
use super::token::{Delim, Token, leaves, split, text_of};
use super::{Func, Scope};

/// The read of one kind of declaration of the top level. It takes the file, the line of the
/// keyword, and the tokens after the keyword.
type ReadDeclaration = fn(&mut GoFile, usize, &[Token]);

/// The keyword of each declaration of the top level that the read takes, its read, and whether
/// the code of the file keeps it. An import is no code: the walk of the literals skips it.
const DECLARATIONS: [(&str, ReadDeclaration, bool); 6] = [
    ("package", read_package, true),
    ("import", read_imports, false),
    ("const", read_consts, true),
    ("var", read_vars, true),
    ("type", read_types, true),
    ("func", read_func, true),
];

/// A const of a Go file whose value is one string literal.
#[derive(Debug, PartialEq, Eq)]
pub struct GoConst {
    pub name: String,
    /// The named type of the const, such as `Kind` or `docs.Kind`. An untyped const has none.
    pub typ: Option<String>,
    pub value: String,
    /// The line of the literal of the value.
    pub line: usize,
}

/// What a Go file declares, and its code without its imports.
#[derive(Debug, Default)]
pub struct GoFile {
    /// The name of the package clause.
    pub package: String,
    /// The path of each import.
    pub imports: Vec<String>,
    /// Each const of the top level whose value is one string literal.
    pub consts: Vec<GoConst>,
    /// Each text of a row of a map table of the top level, such as
    /// `eventKinds = map[Kind]eventKind{...}`. Its name is `eventKinds[Key]`, and its type is the
    /// name of the table.
    pub tables: Vec<GoConst>,
    /// Each function of the top level, with its methods.
    pub funcs: Vec<Func>,
    /// Each variable of the top level that names a function, such as `mustHold = testkit.HoldsAll`:
    /// its name, and the name of the function.
    pub aliases: Vec<(String, String)>,
    /// Each struct type of the top level: its name, and the names of its fields in order.
    pub structs: Vec<(String, Vec<String>)>,
    /// The tokens of the file, without the import declarations.
    pub code: Vec<Token>,
}

/// Read the Go source `text`. The error names the line that the read of the tokens stopped at.
pub fn read(text: &str) -> Result<GoFile, String> {
    let tokens = go_lex::tokens(text)?;
    let mut file = GoFile::default();
    for declaration in split(&tokens, ";") {
        let Some(first) = declaration.first() else {
            continue;
        };
        let reader = DECLARATIONS
            .iter()
            .find(|(keyword, _, _)| first.is_word(keyword));
        if let Some((_, read_one, keep)) = reader {
            read_one(&mut file, first.line(), &declaration[1..]);
            if !keep {
                continue;
            }
        }
        file.code.extend(declaration.iter().cloned());
        file.code.push(Token::Mark {
            text: ";".to_string(),
            line: declaration.last().map_or(first.line(), Token::line),
        });
    }
    Ok(file)
}

/// The specs of a declaration: the one spec after the keyword, or each spec of its group.
fn each_spec(rest: &[Token]) -> Vec<&[Token]> {
    match rest {
        [group] => match group.group(Delim::Paren) {
            Some(specs) => split(specs, ";")
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect(),
            None => vec![rest],
        },
        _ => vec![rest],
    }
}

/// The paths of an import declaration: `"p"`, `name "p"`, or a group of such specs.
fn import_paths(rest: &[Token]) -> Vec<String> {
    each_spec(rest)
        .into_iter()
        .filter_map(|spec| match spec.last() {
            Some(Token::Text { value, .. }) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

/// The consts of one spec, such as `A, B Kind = "a", "b"`, whose values are one string literal
/// each. A spec with no `=` repeats the expression before it, and gives no const here.
fn consts(spec: &[Token]) -> Vec<GoConst> {
    let Some(equals) = spec.iter().position(|t| t.is_mark("=")) else {
        return Vec::new();
    };
    let (left, values) = (&spec[..equals], &spec[equals + 1..]);
    let names = leading_names(left);
    // the names and their commas come first, and the type takes the rest of the left side
    let typ = type_name(
        left.get((names.len() * 2).saturating_sub(1)..)
            .unwrap_or(&[]),
    );
    names
        .iter()
        .zip(split(values, ","))
        .filter_map(|(name, value)| match value {
            [Token::Text { value, line, .. }] => Some(GoConst {
                name: name.to_string(),
                typ: typ.clone(),
                value: value.clone(),
                line: *line,
            }),
            _ => None,
        })
        .collect()
}

/// The name of a type such as `Kind` or `docs.Kind`, or `None` for no type or another form.
fn type_name(tokens: &[Token]) -> Option<String> {
    match tokens {
        [Token::Word { text, .. }] => Some(text.clone()),
        [package, dot, name] if dot.is_mark(".") => {
            Some(format!("{}.{}", package.word()?, name.word()?))
        }
        _ => None,
    }
}

/// The name and the function of a spec `name = function` or `name = package.function`.
fn alias(spec: &[Token]) -> Option<(String, String)> {
    match spec {
        [name, equals, rest @ ..] if equals.is_mark("=") => {
            let function = match rest {
                [function] => function.word()?,
                [package, dot, function] if dot.is_mark(".") && package.word().is_some() => {
                    function.word()?
                }
                _ => return None,
            };
            Some((name.word()?.to_string(), function.to_string()))
        }
        _ => None,
    }
}

/// The name and the fields of a spec `Name struct { ... }`.
fn named_struct(spec: &[Token]) -> Option<(String, Vec<String>)> {
    match spec {
        [name, keyword, body] if keyword.is_word(STRUCT) => Some((
            name.word()?.to_string(),
            struct_fields(body.group(Delim::Brace)?),
        )),
        _ => None,
    }
}

/// The names of the fields of a struct type, in order, from the tokens between its braces. An
/// embedded field takes the name of its type.
pub fn struct_fields(body: &[Token]) -> Vec<String> {
    let mut fields = Vec::new();
    for field in split(body, ";") {
        let embedded = match field {
            [] => continue,
            [only] => only.word(),
            [package, dot, ..] if package.word().is_some() && dot.is_mark(".") => {
                field.iter().filter_map(Token::word).nth(1)
            }
            [star, ..] if star.is_mark("*") => field.iter().rev().find_map(Token::word),
            _ => None,
        };
        match embedded {
            Some(name) => fields.push(name.to_string()),
            None => fields.extend(leading_names(field).into_iter().map(str::to_string)),
        }
    }
    fields
}

/// The function of a declaration `func name(...) ... { ... }`, or of a method. `line` is the line
/// of the keyword. A function with no body gives none.
fn func(line: usize, rest: &[Token]) -> Option<Func> {
    let (method, rest) = match rest.first()?.group(Delim::Paren) {
        Some(_) => (true, &rest[1..]),
        None => (false, rest),
    };
    let name = rest.first()?.word()?.to_string();
    let scope = if exported(&name) {
        Scope::Package
    } else {
        Scope::Module
    };
    let params = rest
        .iter()
        .skip(1)
        .find_map(|t| t.group(Delim::Paren))
        .map(param_names)
        .unwrap_or_default();
    let Token::Group {
        delim: Delim::Brace,
        tokens: body,
        last,
        ..
    } = rest.last()?
    else {
        return None;
    };
    Some(Func {
        name,
        body: text_of(body, &params),
        size: leaves(body),
        params,
        method,
        scope,
        line,
        last: *last,
    })
}

/// True for a Go name that starts with a capital letter: other packages can name it.
pub fn exported(name: &str) -> bool {
    name.starts_with(char::is_uppercase)
}

/// The names of the parameters of a function, in order: the first word of each part between two
/// commas. `where, text string` gives `where` and `text`.
fn param_names(params: &[Token]) -> Vec<String> {
    split(params, ",")
        .into_iter()
        .filter_map(|part| part.first().and_then(Token::word))
        .map(str::to_string)
        .collect()
}

/// The names at the start of a field or of a spec: `a, b T` gives `a` and `b`.
fn leading_names(tokens: &[Token]) -> Vec<&str> {
    let mut names = Vec::new();
    let mut at = 0;
    while let Some(name) = tokens.get(at).and_then(Token::word) {
        names.push(name);
        if !tokens.get(at + 1).is_some_and(|t| t.is_mark(",")) {
            break;
        }
        at += 2;
    }
    names
}

/// The texts of the rows of a table `name = map[K]V{Key: "text", Other: {"text", ...}}`. The name
/// of each text is `name[Key]`. A spec of another form gives none.
fn table(spec: &[Token]) -> Vec<GoConst> {
    let [name, equals, keyword, .., rows] = spec else {
        return Vec::new();
    };
    let (Some(name), Some(rows)) = (name.word(), rows.group(Delim::Brace)) else {
        return Vec::new();
    };
    if !equals.is_mark("=") || !keyword.is_word(MAP) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for row in split(rows, ",") {
        let Some(colon) = row.iter().position(|t| t.is_mark(":")) else {
            continue;
        };
        // a key `pkg.Key` takes its last word
        let Some(key) = row[..colon].iter().rev().find_map(Token::word) else {
            continue;
        };
        let value = &row[colon + 1..];
        let elements = match value {
            [group] => match group.group(Delim::Brace) {
                Some(inner) => split(inner, ","),
                None => vec![value],
            },
            _ => vec![value],
        };
        for element in elements {
            // an element `"text"`, or a field `reason: "text"`
            if let [Token::Text { value, line, .. }] | [_, _, Token::Text { value, line, .. }] =
                element
            {
                out.push(GoConst {
                    name: format!("{name}[{key}]"),
                    typ: Some(name.to_string()),
                    value: value.clone(),
                    line: *line,
                });
            }
        }
    }
    out
}

fn read_package(file: &mut GoFile, _: usize, rest: &[Token]) {
    file.package = rest.first().and_then(Token::word).unwrap_or("").to_string();
}

fn read_imports(file: &mut GoFile, _: usize, rest: &[Token]) {
    file.imports.extend(import_paths(rest));
}

fn read_consts(file: &mut GoFile, _: usize, rest: &[Token]) {
    file.consts
        .extend(each_spec(rest).into_iter().flat_map(consts));
}

fn read_vars(file: &mut GoFile, _: usize, rest: &[Token]) {
    file.aliases
        .extend(each_spec(rest).into_iter().filter_map(alias));
    file.tables
        .extend(each_spec(rest).into_iter().flat_map(table));
}

fn read_types(file: &mut GoFile, _: usize, rest: &[Token]) {
    file.structs
        .extend(each_spec(rest).into_iter().filter_map(named_struct));
}

fn read_func(file: &mut GoFile, line: usize, rest: &[Token]) {
    file.funcs.extend(func(line, rest));
}
