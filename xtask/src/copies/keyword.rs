//! The keywords of Go and of Rust that the scan reads. Each table of the scan names a keyword by
//! its variant, so the text of each keyword is in one place.

/// A keyword of Go or of Rust that the scan reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Keyword {
    Case,
    Chan,
    Const,
    Default,
    Defer,
    Else,
    For,
    Func,
    Go,
    Goto,
    If,
    Import,
    In,
    Interface,
    Let,
    Map,
    Match,
    Package,
    Range,
    Return,
    Select,
    SelfValue,
    Static,
    Struct,
    Super,
    Switch,
    Type,
    Var,
}

impl Keyword {
    /// The text of the keyword in the code.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Keyword::Case => "case",
            Keyword::Chan => "chan",
            Keyword::Const => "const",
            Keyword::Default => "default",
            Keyword::Defer => "defer",
            Keyword::Else => "else",
            Keyword::For => "for",
            Keyword::Func => "func",
            Keyword::Go => "go",
            Keyword::Goto => "goto",
            Keyword::If => "if",
            Keyword::Import => "import",
            Keyword::In => "in",
            Keyword::Interface => "interface",
            Keyword::Let => "let",
            Keyword::Map => "map",
            Keyword::Match => "match",
            Keyword::Package => "package",
            Keyword::Range => "range",
            Keyword::Return => "return",
            Keyword::Select => "select",
            Keyword::SelfValue => "self",
            Keyword::Static => "static",
            Keyword::Struct => "struct",
            Keyword::Super => "super",
            Keyword::Switch => "switch",
            Keyword::Type => "type",
            Keyword::Var => "var",
        }
    }
}
