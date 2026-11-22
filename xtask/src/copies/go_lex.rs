//! The tokens of a Go file, as "Lexical elements" of the Go specification gives them.

use super::keyword::Keyword;
use super::token::{Delim, Token};

/// The keywords of Go after which the end of a line gives no `;`. `break`, `continue`,
/// `fallthrough`, and `return` give one, as an identifier does.
const NO_SEMICOLON_AFTER: [Keyword; 21] = [
    Keyword::Case,
    Keyword::Chan,
    Keyword::Const,
    Keyword::Default,
    Keyword::Defer,
    Keyword::Else,
    Keyword::For,
    Keyword::Func,
    Keyword::Go,
    Keyword::Goto,
    Keyword::If,
    Keyword::Import,
    Keyword::Interface,
    Keyword::Map,
    Keyword::Package,
    Keyword::Range,
    Keyword::Select,
    Keyword::Struct,
    Keyword::Switch,
    Keyword::Type,
    Keyword::Var,
];

/// The operators of Go with more than one character, the longest first. A read takes the longest
/// operator that the text holds.
const OPERATORS: [&str; 25] = [
    "<<=", ">>=", "&^=", "...", "&&", "||", "<-", "++", "--", "==", "!=", "<=", ">=", ":=", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "&^",
];

/// A group that the read has opened and not closed yet.
struct Open {
    delim: Option<Delim>,
    tokens: Vec<Token>,
    line: usize,
}

/// The state of the read of one file.
struct Lexer {
    chars: Vec<char>,
    at: usize,
    line: usize,
    open: Vec<Open>,
}

/// The tokens of the Go source `text`, in a tree of brackets. Each line that ends where the
/// compiler adds a `;` gives the mark `;`, as "Semicolons" of the Go specification says.
///
/// The error names the line of a bracket that does not close, or of a text that does not end.
pub fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let mut lexer = Lexer {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
        open: vec![Open {
            delim: None,
            tokens: Vec::new(),
            line: 1,
        }],
    };
    while let Some(c) = lexer.peek(0) {
        lexer.next_token(c)?;
    }
    lexer.end_of_line();
    match lexer.open.pop() {
        Some(root) if lexer.open.is_empty() => Ok(root.tokens),
        Some(group) => Err(format!("line {}: a bracket does not close", group.line)),
        None => Err("no tokens".to_string()),
    }
}

impl Lexer {
    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn push(&mut self, token: Token) {
        if let Some(group) = self.open.last_mut() {
            group.tokens.push(token);
        }
    }

    /// Read the token that starts with `c`.
    fn next_token(&mut self, c: char) -> Result<(), String> {
        match c {
            '\n' => {
                self.end_of_line();
                self.line += 1;
                self.at += 1;
            }
            c if c.is_whitespace() => self.at += 1,
            '/' if self.peek(1) == Some('/') => {
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.at += 1;
                }
            }
            '/' if self.peek(1) == Some('*') => self.block_comment()?,
            '"' => self.interpreted()?,
            '`' => self.raw()?,
            '\'' => self.rune()?,
            '(' | '[' | '{' => self.open_group(c),
            ')' | ']' | '}' => self.close_group(c)?,
            c if c.is_ascii_digit() => self.number(),
            '.' if self.peek(1).is_some_and(|c| c.is_ascii_digit()) => self.number(),
            c if c.is_alphabetic() || c == '_' => self.word(),
            _ => self.operator(),
        }
        Ok(())
    }

    /// Give the mark `;` when the last token of the open group asks for one at the end of a line.
    fn end_of_line(&mut self) {
        let Some(last) = self.open.last().and_then(|group| group.tokens.last()) else {
            return;
        };
        let semicolon = match last {
            Token::Word { text, .. } => !NO_SEMICOLON_AFTER
                .iter()
                .any(|k| k.as_str() == text.as_str()),
            Token::Text { .. } | Token::Other { .. } | Token::Group { .. } => true,
            Token::Mark { text, .. } => text == "++" || text == "--",
        };
        if semicolon {
            let line = self.line;
            self.push(Token::Mark {
                text: ";".to_string(),
                line,
            });
        }
    }

    fn block_comment(&mut self) -> Result<(), String> {
        let first = self.line;
        self.at += 2;
        let mut lines = false;
        loop {
            match (self.peek(0), self.peek(1)) {
                (Some('*'), Some('/')) => break,
                (Some('\n'), _) => {
                    if !lines {
                        // a comment over two lines ends the line, as a newline does
                        self.end_of_line();
                    }
                    lines = true;
                    self.line += 1;
                }
                (Some(_), _) => {}
                (None, _) => return Err(format!("line {first}: a comment does not end")),
            }
            self.at += 1;
        }
        self.at += 2;
        Ok(())
    }

    /// Read a text in double quotes, and decode its escapes.
    fn interpreted(&mut self) -> Result<(), String> {
        let line = self.line;
        self.at += 1;
        let mut bytes = Vec::new();
        loop {
            match self.peek(0) {
                Some('"') => break,
                Some('\\') => self.escape(&mut bytes)?,
                Some('\n') | None => return Err(format!("line {line}: a text does not end")),
                Some(c) => {
                    let mut buffer = [0; 4];
                    bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                    self.at += 1;
                }
            }
        }
        self.at += 1;
        self.push(Token::Text {
            value: String::from_utf8_lossy(&bytes).into_owned(),
            line,
            last: line,
        });
        Ok(())
    }

    /// Decode the escape at the read position into `bytes`.
    fn escape(&mut self, bytes: &mut Vec<u8>) -> Result<(), String> {
        let line = self.line;
        let Some(kind) = self.peek(1) else {
            return Err(format!("line {line}: a text does not end"));
        };
        self.at += 2;
        let simple = match kind {
            'a' => Some(0x07),
            'b' => Some(0x08),
            'f' => Some(0x0c),
            'n' => Some(b'\n'),
            'r' => Some(b'\r'),
            't' => Some(b'\t'),
            'v' => Some(0x0b),
            '\\' | '\'' | '"' => Some(kind as u8),
            _ => None,
        };
        if let Some(byte) = simple {
            bytes.push(byte);
            return Ok(());
        }
        let (count, radix) = match kind {
            'x' => (2, 16),
            'u' => (4, 16),
            'U' => (8, 16),
            '0'..='7' => {
                // an octal escape has three digits, and the first one is `kind`
                self.at -= 1;
                (3, 8)
            }
            _ => return Err(format!("line {line}: the escape \\{kind} is not Go")),
        };
        let digits: String = (0..count).filter_map(|i| self.peek(i)).collect();
        self.at += count;
        let number = u32::from_str_radix(&digits, radix)
            .map_err(|e| format!("line {line}: the escape {digits}: {e}"))?;
        if matches!(kind, 'u' | 'U') {
            let c = char::from_u32(number).unwrap_or(char::REPLACEMENT_CHARACTER);
            let mut buffer = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        } else {
            bytes.push(u8::try_from(number).map_err(|e| format!("line {line}: {e}"))?);
        }
        Ok(())
    }

    /// Read a raw text in back quotes. Go drops each carriage return of it.
    fn raw(&mut self) -> Result<(), String> {
        let line = self.line;
        self.at += 1;
        let mut value = String::new();
        loop {
            match self.peek(0) {
                Some('`') => break,
                Some('\n') => {
                    value.push('\n');
                    self.line += 1;
                }
                Some('\r') => {}
                Some(c) => value.push(c),
                None => return Err(format!("line {line}: a raw text does not end")),
            }
            self.at += 1;
        }
        self.at += 1;
        let last = self.line;
        self.push(Token::Text { value, line, last });
        Ok(())
    }

    fn rune(&mut self) -> Result<(), String> {
        let line = self.line;
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek(0) {
                Some('\'') => break,
                Some('\\') => self.at += 2,
                Some('\n') | None => return Err(format!("line {line}: a rune does not end")),
                Some(_) => self.at += 1,
            }
        }
        self.at += 1;
        let text = self.chars[start..self.at].iter().collect();
        self.push(Token::Other { text, line });
        Ok(())
    }

    fn number(&mut self) {
        let start = self.at;
        let hex = self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'X'));
        while let Some(c) = self.peek(0) {
            let exponent = if hex {
                matches!(c, 'p' | 'P')
            } else {
                matches!(c, 'e' | 'E')
            };
            if exponent && matches!(self.peek(1), Some('+' | '-')) {
                self.at += 2;
            } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                self.at += 1;
            } else {
                break;
            }
        }
        let text = self.chars[start..self.at].iter().collect();
        let line = self.line;
        self.push(Token::Other { text, line });
    }

    fn word(&mut self) {
        let start = self.at;
        while self
            .peek(0)
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            self.at += 1;
        }
        let text = self.chars[start..self.at].iter().collect();
        let line = self.line;
        self.push(Token::Word { text, line });
    }

    fn operator(&mut self) {
        let rest: String = (0..3).filter_map(|i| self.peek(i)).collect();
        let text = OPERATORS
            .iter()
            .find(|op| rest.starts_with(*op))
            .map_or_else(|| rest.chars().take(1).collect(), |op| op.to_string());
        self.at += text.chars().count();
        let line = self.line;
        self.push(Token::Mark { text, line });
    }

    fn open_group(&mut self, c: char) {
        let delim = match c {
            '(' => Delim::Paren,
            '[' => Delim::Bracket,
            _ => Delim::Brace,
        };
        self.open.push(Open {
            delim: Some(delim),
            tokens: Vec::new(),
            line: self.line,
        });
        self.at += 1;
    }

    fn close_group(&mut self, c: char) -> Result<(), String> {
        let delim = match c {
            ')' => Delim::Paren,
            ']' => Delim::Bracket,
            _ => Delim::Brace,
        };
        let group = match self.open.pop() {
            Some(group) if group.delim == Some(delim) && !self.open.is_empty() => group,
            _ => {
                return Err(format!(
                    "line {}: the bracket {c} closes no group",
                    self.line
                ));
            }
        };
        self.at += 1;
        self.push(Token::Group {
            delim,
            tokens: group.tokens,
            line: group.line,
            last: self.line,
        });
        Ok(())
    }
}
