//! The tokens of a source file, in a tree of brackets.
//!
//! The scans of Rust and of Go both give this tree. So one walk reads the place of each string
//! literal in the two languages.

/// The bracket of a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delim {
    /// `( )`
    Paren,
    /// `[ ]`
    Bracket,
    /// `{ }`
    Brace,
}

/// A token of a source file. A line number starts at 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// An identifier or a keyword.
    Word { text: String, line: usize },
    /// A string literal: the value that the compiler reads, and its first and last line.
    Text {
        value: String,
        line: usize,
        last: usize,
    },
    /// An operator or another mark, such as `==`, `.`, or `,`. Go gives a `;` at the end of a
    /// line where the compiler adds one.
    Mark { text: String, line: usize },
    /// The tokens between two brackets, and the lines of the two brackets.
    Group {
        delim: Delim,
        tokens: Vec<Token>,
        line: usize,
        last: usize,
    },
    /// A number, a character, or a lifetime, as the source writes it.
    Other { text: String, line: usize },
}

impl Token {
    /// The first line of the token.
    pub fn line(&self) -> usize {
        match self {
            Token::Word { line, .. }
            | Token::Text { line, .. }
            | Token::Mark { line, .. }
            | Token::Group { line, .. }
            | Token::Other { line, .. } => *line,
        }
    }

    /// The text of a word, or `None` for another token.
    pub fn word(&self) -> Option<&str> {
        match self {
            Token::Word { text, .. } => Some(text),
            _ => None,
        }
    }

    /// True for the mark `mark`.
    pub fn is_mark(&self, mark: &str) -> bool {
        matches!(self, Token::Mark { text, .. } if text == mark)
    }

    /// True for the word `word`.
    pub fn is_word(&self, word: &str) -> bool {
        self.word() == Some(word)
    }

    /// The tokens of a group with the bracket `delim`, or `None` for another token.
    pub fn group(&self, delim: Delim) -> Option<&[Token]> {
        match self {
            Token::Group {
                delim: d, tokens, ..
            } if *d == delim => Some(tokens),
            _ => None,
        }
    }
}

/// The parts of `tokens` between the marks `mark`, such as the arguments of a call between two
/// commas. A mark inside a group does not divide, because the group is one token.
pub fn split<'a>(tokens: &'a [Token], mark: &str) -> Vec<&'a [Token]> {
    tokens.split(|token| token.is_mark(mark)).collect()
}

/// The count of the tokens that are no group, at each depth of `tokens`.
pub fn leaves(tokens: &[Token]) -> usize {
    tokens
        .iter()
        .map(|token| match token {
            Token::Group { tokens, .. } => leaves(tokens),
            _ => 1,
        })
        .sum()
}

/// The tokens as one line of text, with `$N` in place of the name at the position `N` of `names`.
/// So two bodies that differ only in the names of their arguments give the same text.
pub fn text_of(tokens: &[Token], names: &[String]) -> String {
    let mut out = String::new();
    for token in tokens {
        match token {
            Token::Word { text, .. } => match names.iter().position(|name| name == text) {
                Some(index) => out.push_str(&format!("${index} ")),
                None => out.push_str(&format!("{text} ")),
            },
            Token::Text { value, .. } => out.push_str(&format!("{value:?} ")),
            Token::Mark { text, .. } | Token::Other { text, .. } => {
                out.push_str(&format!("{text} "));
            }
            Token::Group { delim, tokens, .. } => {
                let (open, close) = match delim {
                    Delim::Paren => ("(", ")"),
                    Delim::Bracket => ("[", "]"),
                    Delim::Brace => ("{", "}"),
                };
                out.push_str(&format!("{open} {}{close} ", text_of(tokens, names)));
            }
        }
    }
    out
}
