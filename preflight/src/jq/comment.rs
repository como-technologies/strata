//! The comments of a jq program.

/// A part of a program that [`without_comments`] has open.
#[derive(Clone, Copy)]
enum Part {
    /// A string.
    Text,
    /// The code in a `\( )` of a string, with the depth of its own brackets.
    Code(usize),
}

/// `program` with each comment replaced by a space, so that the program fits on one line.
///
/// A `#` in a string starts no comment. A string can hold code in `\( )`, and that code can hold
/// strings and comments, so the open parts make a stack.
pub(crate) fn without_comments(program: &str) -> String {
    let mut out = String::with_capacity(program.len());
    let mut open: Vec<Part> = Vec::new();
    let mut chars = program.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        match (open.last_mut(), c) {
            (Some(Part::Text), '"') => {
                open.pop();
            }
            (Some(Part::Text), '\\') => {
                out.push(c);
                match chars.next() {
                    Some((_, '(')) => {
                        out.push('(');
                        open.push(Part::Code(0));
                    }
                    Some((_, next)) => out.push(next),
                    None => {}
                }
                continue;
            }
            (Some(Part::Text), _) => {}
            (_, '"') => open.push(Part::Text),
            (Some(Part::Code(depth)), '(') => *depth += 1,
            (Some(Part::Code(0)), ')') => {
                open.pop();
            }
            (Some(Part::Code(depth)), ')') => *depth -= 1,
            (_, '#') => {
                skip_comment(&mut chars);
                out.push(' ');
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

/// Read a comment: the `#` is read. jaq ends a comment at the end of the first line that does not end
/// with an odd number of backslashes. It does not count a `\r` before the `\n`.
pub(super) fn skip_comment(chars: &mut impl Iterator<Item = (usize, char)>) {
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

    #[test]
    fn a_comment_goes_to_the_end_of_its_line_and_the_next_line_stays() {
        assert_eq!(without_comments("1 # a note\n+ 2"), "1  + 2");
        assert_eq!(without_comments("1 # a note"), "1  ");
        assert_eq!(without_comments("1 + 2"), "1 + 2");
    }

    #[test]
    fn a_hash_in_a_string_starts_no_comment() {
        let program = r#""a # b" + "\" # c" + "x""#;
        assert_eq!(without_comments(program), program);
    }

    #[test]
    fn code_in_a_string_can_hold_a_string_and_a_comment() {
        let program = "\"a \\(1 + (2) # note\n + \"#\") # b\" # end\n| .";
        assert_eq!(
            without_comments(program),
            "\"a \\(1 + (2)   + \"#\") # b\"  | ."
        );
    }

    #[test]
    fn a_comment_that_ends_in_a_backslash_goes_on_in_the_next_line() {
        assert_eq!(without_comments("1 # a \\\n b\n+ 2"), "1  + 2");
    }
}
