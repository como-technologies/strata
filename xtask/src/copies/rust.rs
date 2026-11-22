//! What a Rust file declares, for the scan, and the tokens of the file.
//!
//! The texts of a closed set come from four places:
//!
//! - the serde names of an enum;
//! - the table of `impl_closed_set!` or of `impl_texts!`;
//! - a function whose body matches each variant to a text, such as `Error::kind`;
//! - the strings of a const array.

use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Block, Expr, ImplItemConst, ImplItemFn, ItemConst, ItemEnum, ItemFn, ItemImpl,
    ItemMacro, Lit, Pat, ReturnType, Signature, Type, Visibility,
};

use super::rust_sets::{macro_table, match_table, serde_names};
use super::token::{Delim, Token, leaves, text_of};
use super::{Func, Scope};
use crate::files::is_test_code;

/// The operators of Rust with more than one character. A read joins the marks of an operator.
const OPERATORS: [&str; 24] = [
    "==", "!=", "<=", ">=", "&&", "||", "=>", "->", "::", "..", "..=", "...", "+=", "-=", "*=",
    "/=", "%=", "^=", "&=", "|=", "<<", ">>", "<<=", ">>=",
];

/// The macros whose table gives the texts of a closed set: `Variant => "text"`.
const TABLE_MACROS: [&str; 2] = ["impl_closed_set", "impl_texts"];

/// A value that a Rust file declares: a text of a closed set, or a string const.
#[derive(Debug, PartialEq, Eq)]
pub struct RustValue {
    /// The name in the code, such as `Tier::Running` or `SCHEMA`.
    pub name: String,
    pub value: String,
    /// The line of the declaration: the literal of the value, or the variant of a serde name.
    pub line: usize,
    /// True for a text of a closed set, false for a const.
    pub closed: bool,
    pub scope: Scope,
    /// True when test code declares it.
    pub test: bool,
}

/// What a Rust file declares.
#[derive(Debug, Default)]
pub struct RustFile {
    pub values: Vec<RustValue>,
    pub funcs: Vec<Func>,
    /// The first and the last line of each item of test code.
    pub tests: Vec<(usize, usize)>,
}

/// The tokens of the Rust source `text`, in a tree of brackets. An attribute, also a doc
/// comment, gives no token.
pub fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let stream = TokenStream::from_str(text).map_err(|e| e.to_string())?;
    Ok(convert(stream.into_iter().collect()))
}

fn convert(trees: Vec<TokenTree>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut joint = false;
    let mut at = 0;
    while at < trees.len() {
        let skip = attribute_length(&trees[at..]);
        if skip > 0 {
            at += skip;
            joint = false;
            continue;
        }
        let tree = &trees[at];
        at += 1;
        let line = tree.span().start().line;
        let token = match tree {
            TokenTree::Group(group) => Token::Group {
                delim: match group.delimiter() {
                    Delimiter::Bracket => Delim::Bracket,
                    Delimiter::Brace => Delim::Brace,
                    Delimiter::Parenthesis | Delimiter::None => Delim::Paren,
                },
                tokens: convert(group.stream().into_iter().collect()),
                line,
                last: group.span_close().end().line,
            },
            TokenTree::Ident(ident) => Token::Word {
                text: ident.to_string(),
                line,
            },
            TokenTree::Punct(punct) => {
                let text = punct.as_char().to_string();
                if let (true, Some(Token::Mark { text: before, .. })) = (joint, out.last_mut()) {
                    let both = format!("{before}{text}");
                    if OPERATORS.contains(&both.as_str()) {
                        *before = both;
                        joint = punct.spacing() == Spacing::Joint;
                        continue;
                    }
                }
                joint = punct.spacing() == Spacing::Joint;
                out.push(Token::Mark { text, line });
                continue;
            }
            TokenTree::Literal(literal) => match Lit::new(literal.clone()) {
                Lit::Str(text) => Token::Text {
                    value: text.value(),
                    line,
                    last: tree.span().end().line,
                },
                _ => Token::Other {
                    text: literal.to_string(),
                    line,
                },
            },
        };
        joint = false;
        out.push(token);
    }
    out
}

/// The count of the trees of an attribute at the start of `trees`: `#` and `[...]`, or `#`, `!`,
/// and `[...]`. Another start gives 0.
fn attribute_length(trees: &[TokenTree]) -> usize {
    let bracket = |tree: Option<&TokenTree>| match tree {
        Some(TokenTree::Group(group)) => group.delimiter() == Delimiter::Bracket,
        _ => false,
    };
    let punct = |tree: Option<&TokenTree>, c: char| match tree {
        Some(TokenTree::Punct(p)) => p.as_char() == c,
        _ => false,
    };
    if !punct(trees.first(), '#') {
        0
    } else if bracket(trees.get(1)) {
        2
    } else if punct(trees.get(1), '!') && bracket(trees.get(2)) {
        3
    } else {
        0
    }
}

/// Read what the Rust source `text` declares. The error is the error of the parse.
pub fn read(text: &str) -> Result<RustFile, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let mut reader = Reader {
        text,
        file: RustFile::default(),
        test: 0,
        trait_impl: false,
        self_type: None,
    };
    reader.visit_file(&file);
    Ok(reader.file)
}

/// The walk of the items of one file.
struct Reader<'a> {
    text: &'a str,
    file: RustFile,
    /// The depth of the items of test code around the item that the walk reads.
    test: usize,
    /// True inside an `impl Trait for Type`.
    trait_impl: bool,
    /// The name of the type of the `impl` around the item, for `Self`.
    self_type: Option<String>,
}

impl Reader<'_> {
    /// Note a value. A function of a trait impl reaches each file that names the type, so its texts
    /// reach the package.
    fn value(&mut self, name: String, value: String, line: usize, closed: bool, vis: &Visibility) {
        let scope = if self.trait_impl {
            Scope::Package
        } else {
            scope_of(vis)
        };
        self.file.values.push(RustValue {
            name,
            value,
            line,
            closed,
            scope,
            test: self.test > 0,
        });
    }

    /// Walk an item, and note its lines when it is test code.
    fn item<T: Spanned>(&mut self, attrs: &[Attribute], item: &T, walk: impl FnOnce(&mut Self)) {
        let test = is_test_code(attrs);
        if test {
            let span = item.span();
            self.file.tests.push((span.start().line, span.end().line));
            self.test += 1;
        }
        walk(self);
        if test {
            self.test -= 1;
        }
    }

    /// Note a function with its body, and the texts of a closed set when its body matches each
    /// variant to a text.
    fn func(&mut self, sig: &Signature, vis: &Visibility, block: &Block) {
        let params: Vec<String> = sig
            .inputs
            .iter()
            .filter_map(|input| match input {
                syn::FnArg::Typed(typed) => Some(match typed.pat.as_ref() {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                }),
                syn::FnArg::Receiver(_) => None,
            })
            .collect();
        let range = block.span().byte_range();
        // the file parsed, so its block gives tokens. A body with no tokens is too small for the
        // rule of the helpers.
        let body = self
            .text
            .get(range)
            .and_then(|text| tokens(text).ok())
            .and_then(|mut tokens| match tokens.pop() {
                Some(Token::Group { tokens, .. }) => Some(tokens),
                _ => None,
            })
            .unwrap_or_default();
        self.file.funcs.push(Func {
            name: sig.ident.to_string(),
            body: text_of(&body, &params),
            size: leaves(&body),
            params,
            method: self.trait_impl,
            scope: scope_of(vis),
            line: sig.fn_token.span.start().line,
            last: block.brace_token.span.close().end().line,
        });
        let owner = self.self_type.as_deref();
        for (name, text, line) in match_table(block, owner, returns_str(&sig.output)) {
            self.value(name, text, line, true, vis);
        }
    }
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.item(&item.attrs, item, |r| {
            r.func(&item.sig, &item.vis, &item.block);
            visit::visit_item_fn(r, item);
        });
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.item(&item.attrs, item, |r| {
            r.func(&item.sig, &item.vis, &item.block);
            visit::visit_impl_item_fn(r, item);
        });
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        self.item(&item.attrs, item, |r| {
            let (trait_impl, self_type) = (r.trait_impl, r.self_type.take());
            r.trait_impl = item.trait_.is_some();
            r.self_type = type_name(&item.self_ty);
            visit::visit_item_impl(r, item);
            (r.trait_impl, r.self_type) = (trait_impl, self_type);
        });
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        self.item(&item.attrs, item, |r| visit::visit_item_mod(r, item));
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.item(&item.attrs, item, |r| {
            for (name, text, line) in serde_names(item) {
                r.value(name, text, line, true, &item.vis);
            }
        });
    }

    fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
        let name = item.mac.path.segments.last().map(|s| s.ident.to_string());
        if name.is_some_and(|name| TABLE_MACROS.contains(&name.as_str())) {
            let tokens = convert(item.mac.tokens.clone().into_iter().collect());
            let (texts, scope) = macro_table(&tokens);
            for (name, value, line) in texts {
                self.file.values.push(RustValue {
                    name,
                    value,
                    line,
                    closed: true,
                    scope,
                    test: self.test > 0,
                });
            }
        }
    }

    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        self.item(&item.attrs, item, |r| {
            let (texts, list) = const_texts(&item.expr);
            for (text, line) in texts {
                r.value(item.ident.to_string(), text, line, list, &item.vis);
            }
            visit::visit_item_const(r, item);
        });
    }

    fn visit_impl_item_const(&mut self, item: &'ast ImplItemConst) {
        self.item(&item.attrs, item, |r| {
            let name = match &r.self_type {
                Some(owner) => format!("{owner}::{}", item.ident),
                None => item.ident.to_string(),
            };
            let (texts, list) = const_texts(&item.expr);
            for (text, line) in texts {
                r.value(name.clone(), text, line, list, &item.vis);
            }
        });
    }
}

/// The scope of an item with the visibility `vis`: `pub` and `pub(crate)` reach the package, and a
/// private item reaches its module.
fn scope_of(vis: &Visibility) -> Scope {
    match vis {
        Visibility::Inherited => Scope::Module,
        Visibility::Restricted(r) if r.path.is_ident("super") => Scope::Parent,
        Visibility::Restricted(r) if r.path.is_ident("self") => Scope::Module,
        Visibility::Public(_) | Visibility::Restricted(_) => Scope::Package,
    }
}

/// The last name of a type path, such as `Error` of `crate::error::Error`.
fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// True for a function that returns `&str` or `&'static str`.
fn returns_str(output: &ReturnType) -> bool {
    match output {
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Reference(r) => type_name(&r.elem).is_some_and(|name| name == "str"),
            _ => false,
        },
        ReturnType::Default => false,
    }
}

/// The literals of a const: one string, or each string of an array or of a slice, also in a tuple
/// of the array. The flag is true for an array or a slice. Its strings are a closed set, such as
/// a list of names or a table of texts.
fn const_texts(expr: &Expr) -> (Vec<(String, usize)>, bool) {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(text) => (vec![(text.value(), text.span().start().line)], false),
            _ => (Vec::new(), false),
        },
        Expr::Array(array) => {
            // a row of a table, such as `(Lang::Go, "go")`, gives each string of the row
            let row = |e: &Expr| match e {
                Expr::Tuple(tuple) => tuple.elems.iter().flat_map(|e| const_texts(e).0).collect(),
                Expr::Lit(_) => const_texts(e).0,
                _ => Vec::new(),
            };
            (array.elems.iter().flat_map(row).collect(), true)
        }
        Expr::Reference(reference) => const_texts(&reference.expr),
        _ => (Vec::new(), false),
    }
}
