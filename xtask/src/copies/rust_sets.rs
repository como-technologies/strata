//! The texts of the closed sets of a Rust file. They come from the serde names of an enum, the
//! table of a macro of closed sets, and a function that matches each variant to a text.

use syn::meta::ParseNestedMeta;
use syn::{Attribute, Block, Expr, ItemEnum, Lit, LitStr, Pat, Stmt};

use super::Scope;
use super::token::{Delim, Token, split};

/// A text of a closed set: the name of its variant, such as `Tier::Running`, the text, and the line
/// of its declaration.
pub type SetText = (String, String, usize);

/// The serde settings of an enum or of a variant that change its names.
#[derive(Default)]
struct Serde {
    rename_all: Option<String>,
    rename: Option<String>,
    aliases: Vec<String>,
    /// `untagged` or `skip`: serde writes no name for it.
    nameless: bool,
}

/// Read the serde settings of `attrs`. A setting that changes no name is read and dropped.
fn serde_of(attrs: &[Attribute]) -> syn::Result<Serde> {
    let mut serde = Serde::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        attr.parse_nested_meta(|meta| {
            let text = |meta: &ParseNestedMeta| -> syn::Result<String> {
                Ok(meta.value()?.parse::<LitStr>()?.value())
            };
            if meta.path.is_ident("rename_all") && meta.input.peek(syn::Token![=]) {
                serde.rename_all = Some(text(&meta)?);
            } else if meta.path.is_ident("rename") && meta.input.peek(syn::Token![=]) {
                serde.rename = Some(text(&meta)?);
            } else if meta.path.is_ident("alias") {
                serde.aliases.push(text(&meta)?);
            } else if ["untagged", "skip", "skip_serializing", "skip_deserializing"]
                .iter()
                .any(|name| meta.path.is_ident(name))
            {
                serde.nameless = true;
            } else {
                drop_meta(&meta)?;
            }
            Ok(())
        })?;
    }
    Ok(serde)
}

/// Read a serde setting that changes no name: `key`, `key = value`, or `key(...)`.
fn drop_meta(meta: &ParseNestedMeta) -> syn::Result<()> {
    if meta.input.peek(syn::Token![=]) {
        meta.value()?.parse::<Expr>()?;
    } else if meta.input.peek(syn::token::Paren) {
        meta.parse_nested_meta(|inner| drop_meta(&inner))?;
    }
    Ok(())
}

/// True when the enum derives `Serialize` or `Deserialize`.
fn derives_serde(attrs: &[Attribute]) -> bool {
    let mut serde = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("derive")) {
        // a derive that does not parse derives nothing that the scan reads
        let _ = attr.parse_nested_meta(|meta| {
            let last = meta.path.segments.last().map(|s| s.ident.to_string());
            serde |= last.is_some_and(|name| name == "Serialize" || name == "Deserialize");
            Ok(())
        });
    }
    serde
}

/// The serde names of each variant of an enum that derives serde, with its aliases. An enum whose
/// serde attribute does not parse gives no name, and so does such a variant.
pub fn serde_names(item: &ItemEnum) -> Vec<SetText> {
    if !derives_serde(&item.attrs) {
        return Vec::new();
    }
    let Ok(container) = serde_of(&item.attrs) else {
        return Vec::new();
    };
    if container.nameless {
        return Vec::new();
    }
    let mut out = Vec::new();
    for variant in &item.variants {
        let Ok(own) = serde_of(&variant.attrs) else {
            continue;
        };
        if own.nameless {
            continue;
        }
        let name = format!("{}::{}", item.ident, variant.ident);
        let line = variant.ident.span().start().line;
        let text = own
            .rename
            .clone()
            .unwrap_or_else(|| rename(&variant.ident.to_string(), container.rename_all.as_deref()));
        out.push((name.clone(), text, line));
        out.extend(own.aliases.into_iter().map(|a| (name.clone(), a, line)));
    }
    out
}

/// The name that serde writes for the variant `name` under the rule `rule` of `rename_all`.
pub fn rename(name: &str, rule: Option<&str>) -> String {
    let words = || {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    };
    match rule {
        Some("lowercase") => name.to_lowercase(),
        Some("UPPERCASE") => name.to_uppercase(),
        Some("camelCase") => {
            let mut chars = name.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_lowercase().chain(chars).collect()
            })
        }
        Some("snake_case") => words(),
        Some("SCREAMING_SNAKE_CASE") => words().to_uppercase(),
        Some("kebab-case") => words().replace('_', "-"),
        Some("SCREAMING-KEBAB-CASE") => words().replace('_', "-").to_uppercase(),
        _ => name.to_string(),
    }
}

/// The texts of the table of `impl_closed_set!(Set { V => "text", ... })`, or of
/// `impl_texts!(vis Set { ... })`, with the scope of `as_str`. `tokens` are those of the macro.
pub fn macro_table(tokens: &[Token]) -> (Vec<SetText>, Scope) {
    let Some(at) = tokens.iter().position(|t| t.group(Delim::Brace).is_some()) else {
        return (Vec::new(), Scope::Package);
    };
    let set = tokens[..at]
        .iter()
        .rev()
        .find_map(Token::word)
        .unwrap_or("");
    let scope = match &tokens[..at] {
        [only] if only.word().is_some() => Scope::Package,
        [vis, group, _] if vis.is_word("pub") => match group.group(Delim::Paren) {
            Some([word]) if word.is_word("super") => Scope::Parent,
            Some([word]) if word.is_word("self") => Scope::Module,
            _ => Scope::Package,
        },
        [vis, _] if vis.is_word("pub") => Scope::Package,
        _ => Scope::Module,
    };
    let rows = tokens[at].group(Delim::Brace).unwrap_or(&[]);
    let texts = split(rows, ",")
        .into_iter()
        .filter_map(|row| match row {
            [variant, arrow, Token::Text { value, line, .. }] if arrow.is_mark("=>") => {
                Some((format!("{set}::{}", variant.word()?), value.clone(), *line))
            }
            _ => None,
        })
        .collect();
    (texts, scope)
}

/// The texts of a function whose body is one `match` that gives a text for a variant in two arms
/// or more. `Error::kind` and the `fmt` of a `Display` are such functions. An arm gives the text that it returns
/// when `returns_str` is true, or the text that it writes with `write_str`. `owner` is the type of
/// the `impl`, for `Self`. Another body gives none.
pub fn match_table(block: &Block, owner: Option<&str>, returns_str: bool) -> Vec<SetText> {
    let [Stmt::Expr(Expr::Match(table), None)] = block.stmts.as_slice() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for arm in &table.arms {
        let text = match arm.body.as_ref() {
            Expr::Lit(lit) if returns_str => &lit.lit,
            Expr::MethodCall(call) if call.method == "write_str" => match call.args.first() {
                Some(Expr::Lit(lit)) if call.args.len() == 1 => &lit.lit,
                _ => continue,
            },
            _ => continue,
        };
        let Lit::Str(text) = text else {
            continue;
        };
        let line = text.span().start().line;
        for name in variants(&arm.pat, owner) {
            out.push((name, text.value(), line));
        }
    }
    if out.len() < 2 { Vec::new() } else { out }
}

/// The variants that a pattern names, such as `Error::Api` of `Error::Api { .. }`.
fn variants(pat: &Pat, owner: Option<&str>) -> Vec<String> {
    let path = match pat {
        Pat::Path(p) => &p.path,
        Pat::Struct(p) => &p.path,
        Pat::TupleStruct(p) => &p.path,
        Pat::Or(or) => return or.cases.iter().flat_map(|p| variants(p, owner)).collect(),
        _ => return Vec::new(),
    };
    let names: Vec<String> = path
        .segments
        .iter()
        .map(|s| match (s.ident.to_string().as_str(), owner) {
            ("Self", Some(owner)) => owner.to_string(),
            (name, _) => name.to_string(),
        })
        .collect();
    let start = names.len().saturating_sub(2);
    vec![names[start..].join("::")]
}
