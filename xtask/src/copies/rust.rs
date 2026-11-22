//! What a Rust file declares, for the scan, and the tokens of the file.
//!
//! The texts of a closed set come from three places:
//!
//! - the serde names of an enum;
//! - the table of `impl_closed_set!` or of `impl_texts!`;
//! - a function whose body matches each variant to a text, such as `Error::kind`.
//!
//! A const gives its string, or each string of its array. A string of a const array is a const.

use std::collections::BTreeMap;
use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    Block, Expr, ImplItemConst, ImplItemFn, ItemConst, ItemEnum, ItemFn, ItemImpl, ItemMacro,
    ItemStruct, Lit, Pat, ReturnType, Signature, Type, Visibility,
};

use super::keyword::Keyword;
use super::rust_sets::{macro_table, match_table, serde_names};
use super::token::{Delim, Token, leaves, text_of};
use super::{Func, Scope};
use crate::files::TestLines;

/// The operators of Rust with more than one character. A read joins the marks of an operator.
const OPERATORS: [&str; 24] = [
    "==", "!=", "<=", ">=", "&&", "||", "=>", "->", "::", "..", "..=", "...", "+=", "-=", "*=",
    "/=", "%=", "^=", "&=", "|=", "<<", ">>", "<<=", ">>=",
];

/// The macros whose table gives the texts of a closed set: `Variant => "text"`. True for a macro
/// that implements a trait, so its texts reach where its type reaches.
const TABLE_MACROS: [(&str, bool); 2] = [("impl_closed_set", true), ("impl_texts", false)];

/// A value that a Rust file declares: a text of a closed set, or a string const.
#[derive(Debug, PartialEq, Eq)]
pub struct RustValue {
    /// The name in the code, such as `Tier::Running` or `SCHEMA`.
    pub name: String,
    pub value: String,
    /// The line of the declaration: the literal of the value, or the variant of a serde name.
    pub line: usize,
    /// True for a text of a closed set, false for a const or a string of a const array.
    pub closed: bool,
    pub scope: Scope,
}

/// What a Rust file declares.
#[derive(Debug, Default)]
pub struct RustFile {
    pub values: Vec<RustValue>,
    pub funcs: Vec<Func>,
    /// The lines of each item of test code.
    pub tests: TestLines,
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

/// Read what the Rust source `text` declares.
///
/// The error is the error of the parse. It is also the error of a table of `impl_texts!` or of
/// `impl_closed_set!` whose visibility does not parse, such as `pub(in 3)`.
pub fn read(text: &str) -> Result<RustFile, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let mut types = Types::default();
    types.visit_file(&file);
    let mut reader = Reader {
        text,
        file: RustFile {
            tests: TestLines::of(&file),
            ..RustFile::default()
        },
        types: types.0,
        trait_impl: false,
        self_type: None,
        fault: None,
    };
    reader.visit_file(&file);
    match reader.fault {
        Some(fault) => Err(fault),
        None => Ok(reader.file),
    }
}

/// The name of the closed set of the name of a text, such as `Tier` of `Tier::Running`.
pub fn set_of(name: &str) -> &str {
    name.split("::").next().unwrap_or(name)
}

/// The scope of each enum and each struct of a file, by its name.
#[derive(Default)]
struct Types(BTreeMap<String, Scope>);

impl<'ast> Visit<'ast> for Types {
    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.0.insert(item.ident.to_string(), scope_of(&item.vis));
    }

    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        self.0.insert(item.ident.to_string(), scope_of(&item.vis));
    }
}

/// The walk of the items of one file.
struct Reader<'a> {
    text: &'a str,
    file: RustFile,
    /// The scope of each type that the file declares, by its name.
    types: BTreeMap<String, Scope>,
    /// True inside an `impl Trait for Type`.
    trait_impl: bool,
    /// The name of the type of the `impl` around the item, for `Self`.
    self_type: Option<String>,
    /// The first item that the read cannot read.
    fault: Option<String>,
}

impl Reader<'_> {
    /// The scope of a value of an item with the visibility `vis`. A function or a const of a trait
    /// impl reaches each file that names its type, so its value reaches where the type reaches.
    fn scope(&self, vis: &Visibility) -> Scope {
        if self.trait_impl {
            self.type_scope(self.self_type.as_deref())
        } else {
            scope_of(vis)
        }
    }

    /// Where the type `name` reaches: the scope of its declaration in the file. The check cannot
    /// see the visibility of a type of another file, so such a type reaches the package.
    fn type_scope(&self, name: Option<&str>) -> Scope {
        name.and_then(|name| self.types.get(name))
            .copied()
            .unwrap_or(Scope::Package)
    }

    /// Note a value with the scope `scope`.
    fn value(&mut self, name: String, value: String, line: usize, closed: bool, scope: Scope) {
        self.file.values.push(RustValue {
            name,
            value,
            line,
            closed,
            scope,
        });
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
            line: sig.fn_token.span.start().line,
            last: block.brace_token.span.close().end().line,
        });
        let scope = self.scope(vis);
        let owner = self.self_type.as_deref();
        for (name, text, line) in match_table(block, owner, returns_str(&sig.output)) {
            self.value(name, text, line, true, scope);
        }
    }
}

impl<'ast> Visit<'ast> for Reader<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.func(&item.sig, &item.vis, &item.block);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.func(&item.sig, &item.vis, &item.block);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let (trait_impl, self_type) = (self.trait_impl, self.self_type.take());
        self.trait_impl = item.trait_.is_some();
        self.self_type = type_name(&item.self_ty);
        visit::visit_item_impl(self, item);
        (self.trait_impl, self.self_type) = (trait_impl, self_type);
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        let scope = self.scope(&item.vis);
        for (name, text, line) in serde_names(item) {
            self.value(name, text, line, true, scope);
        }
    }

    fn visit_item_macro(&mut self, item: &'ast ItemMacro) {
        let name = item.mac.path.segments.last().map(|s| s.ident.to_string());
        let Some((table, implements)) = TABLE_MACROS
            .into_iter()
            .find(|(table, _)| name.as_deref() == Some(*table))
        else {
            return;
        };
        let vis = match item.mac.parse_body_with(visibility) {
            Ok(vis) => vis,
            Err(e) => {
                // the compiler refuses such a call, so the check gives its error
                let line = e.span().start().line;
                self.fault
                    .get_or_insert_with(|| format!("line {line}: {table}!: {e}"));
                return;
            }
        };
        let tokens = convert(item.mac.tokens.clone().into_iter().collect());
        for (name, value, line) in macro_table(&tokens) {
            // the table of a macro that implements a trait reaches where its type reaches
            let scope = match implements {
                true => self.type_scope(Some(set_of(&name))),
                false => scope_of(&vis),
            };
            self.value(name, value, line, true, scope);
        }
    }

    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        let scope = self.scope(&item.vis);
        for (text, line) in const_texts(&item.expr) {
            self.value(item.ident.to_string(), text, line, false, scope);
        }
        visit::visit_item_const(self, item);
    }

    fn visit_impl_item_const(&mut self, item: &'ast ImplItemConst) {
        let name = match &self.self_type {
            Some(owner) => format!("{owner}::{}", item.ident),
            None => item.ident.to_string(),
        };
        let scope = self.scope(&item.vis);
        for (text, line) in const_texts(&item.expr) {
            self.value(name.clone(), text, line, false, scope);
        }
    }
}

/// The scope of an item with the visibility `vis`. `pub` reaches the package, and `pub(crate)` and
/// `pub(in path)` reach the crate. A private item reaches its module.
fn scope_of(vis: &Visibility) -> Scope {
    match vis {
        Visibility::Inherited => Scope::Module,
        Visibility::Restricted(r) if r.path.is_ident(Keyword::Super.as_str()) => Scope::Parent,
        Visibility::Restricted(r) if r.path.is_ident(Keyword::SelfValue.as_str()) => Scope::Module,
        Visibility::Restricted(_) => Scope::Crate,
        Visibility::Public(_) => Scope::Package,
    }
}

/// The visibility at the start of the tokens of a macro, such as `pub(super)` of
/// `impl_texts!(pub(super) Format { ... })`. A start with no `pub` gives `Inherited`.
fn visibility(input: ParseStream) -> syn::Result<Visibility> {
    let vis = input.parse()?;
    input.parse::<TokenStream>()?;
    Ok(vis)
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
/// of the array.
fn const_texts(expr: &Expr) -> Vec<(String, usize)> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(text) => vec![(text.value(), text.span().start().line)],
            _ => Vec::new(),
        },
        Expr::Array(array) => {
            // a row of a table, such as `(Lang::Go, "go")`, gives each string of the row
            let row = |e: &Expr| match e {
                Expr::Tuple(tuple) => tuple.elems.iter().flat_map(const_texts).collect(),
                Expr::Lit(_) => const_texts(e),
                _ => Vec::new(),
            };
            array.elems.iter().flat_map(row).collect()
        }
        Expr::Reference(reference) => const_texts(&reference.expr),
        _ => Vec::new(),
    }
}
