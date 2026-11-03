//! The depth of a jq program, so that a program that nests too deep is refused before jaq reads it.
//!
//! jaq reads and compiles each level of a program with a call of its own, so a program of some
//! hundreds of levels overflows the stack of the thread. That is not an error, and it has no exit
//! code. [`depth`] counts the levels from the text, with no recursion of its own.

/// The most levels of nesting that a program can have. The deepest program of the catalog has
/// fewer than half of this number.
pub(super) const MAX_DEPTH: usize = 64;

/// What a word of jq does to the count of levels.
#[derive(Clone, Copy)]
enum Word {
    /// It nests the rest of the chain that it is in, as `|` does: `and`, `try`, `as`.
    Operator,
    /// It opens a part that `end` closes.
    If,
    /// It ends the condition or the branch of an `if`, and a new branch starts.
    Branch,
    /// It closes the part that `if` opened.
    End,
    /// It opens a part that `;` closes.
    Def,
}

/// Each word of jq that jaq reads with a call of its own, with what it does.
const WORDS: [(&str, Word); 14] = [
    ("and", Word::Operator),
    ("or", Word::Operator),
    ("try", Word::Operator),
    ("catch", Word::Operator),
    ("reduce", Word::Operator),
    ("foreach", Word::Operator),
    ("as", Word::Operator),
    ("label", Word::Operator),
    ("if", Word::If),
    ("then", Word::Branch),
    ("elif", Word::Branch),
    ("else", Word::Branch),
    ("end", Word::End),
    ("def", Word::Def),
];

/// A part of a program that [`depth`] has open.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    /// A bracket, or the expression in a `\( )` of a string. The program itself is one too.
    Bracket,
    /// An `if`, up to its `end`.
    If,
    /// A `def`, up to its `;`.
    Def,
    /// A string.
    Text,
}

/// The open parts of a program, and the levels that they make.
struct Levels {
    /// Each open part, with the operators met in it. The first part is the program, and nothing
    /// closes it.
    open: Vec<(Part, usize)>,
    /// The levels of the open parts: one for each part but the program and the strings, and one for
    /// each operator in them.
    level: usize,
    /// The most levels that `level` had.
    deepest: usize,
}

impl Levels {
    fn new() -> Self {
        Levels {
            open: vec![(Part::Bracket, 0)],
            level: 0,
            deepest: 0,
        }
    }

    fn in_text(&self) -> bool {
        matches!(self.open.last(), Some((Part::Text, _)))
    }

    /// The levels that a part makes with no operator: a string makes none.
    fn weight(part: Part) -> usize {
        usize::from(part != Part::Text)
    }

    fn count(&mut self, levels: usize) {
        self.level += levels;
        self.deepest = self.deepest.max(self.level);
    }

    fn open(&mut self, part: Part) {
        self.open.push((part, 0));
        self.count(Self::weight(part));
    }

    /// Close the last part when it is `part`. A closer that closes nothing is no level.
    fn close(&mut self, part: Part) {
        if self.open.len() > 1
            && self.open.last().is_some_and(|&(last, _)| last == part)
            && let Some((_, operators)) = self.open.pop()
        {
            self.level -= Self::weight(part) + operators;
        }
    }

    /// An operator: jaq reads the rest of the chain inside the call that read it.
    fn operator(&mut self) {
        if let Some((_, operators)) = self.open.last_mut() {
            *operators += 1;
            self.count(1);
        }
    }

    /// A new branch of an `if`: the chain of the condition or of the last branch is read.
    fn branch(&mut self) {
        if let Some((Part::If, operators)) = self.open.last_mut() {
            self.level -= *operators;
            *operators = 0;
        }
    }

    fn word(&mut self, word: Word) {
        match word {
            Word::Operator => self.operator(),
            Word::If => self.open(Part::If),
            Word::Branch => self.branch(),
            Word::End => self.close(Part::If),
            Word::Def => self.open(Part::Def),
        }
    }
}

/// The depth of `program`: how many calls jaq makes to read one part of it inside another.
///
/// A bracket opens a level. So does each operator of a chain such as `a | b | c`. This is because
/// jaq reads the rest of the chain inside the call that read the operator. The words `and`, `or`,
/// `try`, `catch`, `reduce`, `foreach`, `as`, and `label` are operators too. An `if` and a `def`
/// open a level like a bracket, and `end` and `;` close it.
///
/// The count is not exact. An operator in a name counts like code, and so does a field with the
/// name of a keyword. A count that is too high only makes the check refuse a program early. A
/// comment counts for nothing, as in jaq: a closer in a comment must not close a level of the code.
pub(super) fn depth(program: &str) -> usize {
    let mut levels = Levels::new();
    let mut chars = program.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if levels.in_text() {
            match c {
                '"' => levels.close(Part::Text),
                '\\' if chars.next().is_some_and(|(_, next)| next == '(') => {
                    levels.open(Part::Bracket);
                }
                _ => {}
            }
            continue;
        }
        match c {
            '"' => levels.open(Part::Text),
            '(' | '[' | '{' => levels.open(Part::Bracket),
            ')' | ']' | '}' => levels.close(Part::Bracket),
            ';' => levels.close(Part::Def),
            '#' => skip_comment(&mut chars),
            '|' | '+' | '-' | '*' | '/' | '%' | ',' | '=' | '<' | '>' => levels.operator(),
            // a name starts with a letter or `_`, so the `and` in `1and` is a keyword
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut end = at + 1;
                while let Some((i, _)) =
                    chars.next_if(|(_, n)| n.is_ascii_alphanumeric() || *n == '_')
                {
                    end = i + 1;
                }
                if let Some(word) = keyword(program, at, end) {
                    levels.word(word);
                }
            }
            _ => {}
        }
    }
    levels.deepest
}

/// The word at `program[start..end]`, when it is a keyword that counts.
///
/// A word that lowers the count must be a keyword for sure. `.end`, `$end`, and `{end: 1}` are a
/// field, a variable, and a key, and they close nothing. A word that raises the count is a keyword
/// even when it is a field. So the count is never too low.
fn keyword(program: &str, start: usize, end: usize) -> Option<Word> {
    let text = &program[start..end];
    let (_, word) = WORDS.iter().find(|(name, _)| *name == text)?;
    let lowers = matches!(word, Word::Branch | Word::End);
    let name =
        program[..start].ends_with(['.', '$']) || program[end..].trim_start().starts_with(':');
    if lowers && name { None } else { Some(*word) }
}

/// Read a comment: the `#` is read. jaq ends a comment at the end of the first line that does not end
/// with an odd number of backslashes. It does not count a `\r` before the `\n`.
fn skip_comment(chars: &mut impl Iterator<Item = (usize, char)>) {
    // the backslashes that end the text so far, and those before a `\r` that ends it
    let (mut backslashes, mut before_cr, mut cr) = (0, 0, false);
    for (_, c) in chars {
        match c {
            '\n' => {
                if (if cr { before_cr } else { backslashes }) % 2 == 0 {
                    return;
                }
                (backslashes, cr) = (0, false);
            }
            '\r' => (before_cr, backslashes, cr) = (backslashes, 0, true),
            '\\' => (backslashes, cr) = (backslashes + 1, false),
            _ => (backslashes, cr) = (0, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jq::Jq;

    /// A program of `levels` levels in the shape `shape`: brackets, a chain of pipes or sums, a
    /// chain of the keywords that jaq reads with a call of their own, or brackets with their
    /// closers in a comment.
    fn nested(shape: &str, levels: usize) -> String {
        match shape {
            "parens" => format!("{}1{}", "(".repeat(levels), ")".repeat(levels)),
            "arrays" => format!("{}1{}", "[".repeat(levels), "]".repeat(levels)),
            "objects" => format!("{}1{}", "{a:".repeat(levels), "}".repeat(levels)),
            "pipes" => format!("{}.", ".|".repeat(levels)),
            "sums" => format!("{}1", "1+".repeat(levels)),
            "strings" => format!("{}1{}", "\"\\(".repeat(levels), ")\"".repeat(levels)),
            "try" => format!("{}[]", "try ".repeat(levels)),
            "and" => format!("{}true", "true and ".repeat(levels)),
            "or" => format!("{}true", "true or ".repeat(levels)),
            "if" => format!(
                "{}1{}",
                "if . then ".repeat(levels),
                " else 2 end".repeat(levels)
            ),
            "defs" => format!("{}.{}", "def f: ".repeat(levels), "; f".repeat(levels)),
            "labels" => format!("{}.", "label $l | ".repeat(levels)),
            "comments" => format!("{}1{}", "(# )\n".repeat(levels), ")".repeat(levels)),
            "continued comments" => {
                format!("{}1{}", "(# \\\n)\n".repeat(levels), ")".repeat(levels))
            }
            _ => unreachable!("{shape}"),
        }
    }

    const SHAPES: [&str; 14] = [
        "parens",
        "arrays",
        "objects",
        "pipes",
        "sums",
        "strings",
        "try",
        "and",
        "or",
        "if",
        "defs",
        "labels",
        "comments",
        "continued comments",
    ];

    #[test]
    fn a_program_that_nests_deeper_than_the_limit_is_refused_and_never_overflows_the_stack() {
        let jq = Jq::new();
        for shape in SHAPES {
            // far past the depth that overflows the stack of a test thread
            let program = nested(shape, 5000);
            let err = jq.check("deep", &program, false).unwrap_err();
            assert!(
                err.to_string().contains("nests more than"),
                "{shape}: {err}"
            );
            assert!(
                jq.check("deep", &nested(shape, 10), false).is_ok(),
                "{shape}"
            );
        }
    }

    #[test]
    fn the_depth_of_a_program_counts_brackets_and_chains_and_not_the_text_of_a_string() {
        assert_eq!(depth("."), 0);
        assert_eq!(depth(".a.b.c"), 0, "a path is not a chain of operators");
        assert_eq!(depth("(.a)"), 1);
        assert_eq!(depth(".a | .b | .c"), 2);
        assert_eq!(depth("[.[] | select(.x == 1)]"), 5);
        assert_eq!(depth(r#""((((((((((""#), 0, "brackets in a string are text");
        assert_eq!(
            depth(r#""a\"((((""#),
            0,
            "an escaped quote does not end the string"
        );
        assert_eq!(
            depth(r#""\(.a + 1)""#),
            2,
            "the expression in a string is code"
        );
        assert_eq!(depth(")))"), 0, "a bracket that closes nothing is no level");
        assert_eq!(depth(&nested("parens", 10)), 10);
    }

    #[test]
    fn the_depth_of_a_program_counts_each_keyword_that_jaq_reads_with_a_call_of_its_own() {
        assert_eq!(depth("try try try ."), 3);
        assert_eq!(depth("true and true or true and true"), 3);
        assert_eq!(depth("try error catch ."), 2);
        assert_eq!(depth("label $out | 1"), 2);
        assert_eq!(depth(". as $x | $x"), 2);
        assert_eq!(depth("reduce .[] as $x (0; . + $x)"), 4);
        assert_eq!(depth("foreach .[] as $x (0; . + $x)"), 4);
        assert_eq!(
            depth("1and 1or 1"),
            2,
            "a keyword after a number is a keyword"
        );
        assert_eq!(depth("(true and true)"), 2);
        assert_eq!(depth("(true and true) and (true and true)"), 3);
        assert_eq!(
            depth(r#""try and if \(1 and 1) def""#),
            2,
            "a keyword in a string is text, and one in its expression is code"
        );
        assert_eq!(
            depth("android orange iffy tryit define _or or_"),
            0,
            "a name that holds a keyword is no keyword"
        );
    }

    #[test]
    fn an_if_and_a_def_are_a_level_that_end_and_a_semicolon_close() {
        assert_eq!(depth("if . then 1 else 2 end"), 1);
        assert_eq!(
            depth("if if if . then 1 else 2 end then 1 else 2 end then 1 else 2 end"),
            3
        );
        assert_eq!(
            depth("if . then 1 elif . then 2 elif . then 3 else 4 end"),
            1,
            "the branches of an if are not nested in each other"
        );
        assert_eq!(
            depth("if . and . then 1 else 2 end | if . or . then 3 else 4 end"),
            3,
            "an if after an end is not inside it"
        );
        assert_eq!(depth("def f: 1; def g: 2; def h: 3; ."), 1);
        assert_eq!(depth("def f: def g: def h: 1; h; g; f"), 3);
        assert_eq!(
            depth("def f(a; b): a | b | b | b; ."),
            4,
            "the ; of the arguments does not close the def"
        );
        assert_eq!(
            depth("end end ; ; else then"),
            0,
            "a closer that closes nothing is no level"
        );
        assert_eq!(
            depth(".spec.pool.end | if .a then .end else .else end"),
            2,
            "a field is no keyword that closes a level"
        );
        assert_eq!(
            depth("$end | {end: 1, then: 2} | if . then 1 else 2 end"),
            3,
            "a variable and a key are no keyword that closes a level"
        );
    }

    #[test]
    fn a_comment_counts_for_nothing_and_ends_as_in_jaq() {
        assert_eq!(depth("1 # ((((\n+ 2"), 1);
        assert_eq!(depth("(# )\n1"), 1, "a closer in a comment closes nothing");
        assert_eq!(
            depth("1 # ((((\\\n(((("),
            0,
            "a backslash continues the comment"
        );
        assert_eq!(depth("1 # ((((\\\\\n(1)"), 1, "two backslashes do not");
        assert_eq!(
            depth("1 # ((((\\\r\n(((("),
            0,
            "a backslash before a \\r\\n continues it"
        );
        assert_eq!(
            depth("1 # ((((\\\r\r\n(1)"),
            1,
            "only one \\r is out of the count"
        );
        assert_eq!(depth("1 # (((("), 0, "a comment can end with the program");
        assert_eq!(depth(r##""# (((("##), 0, "a # in a string is text");
    }

    #[test]
    fn the_programs_of_the_catalog_are_far_from_the_limit_of_depth() {
        let catalog = crate::catalog::builtin().unwrap();
        let deepest = catalog
            .rules()
            .flat_map(|r| r.check.iter().map(|c| c.expr()).chain(r.when.as_deref()))
            .map(depth)
            .max()
            .unwrap();
        assert!(
            deepest * 2 < MAX_DEPTH,
            "the deepest program has depth {deepest}"
        );
    }
}
