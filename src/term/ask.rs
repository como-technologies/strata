//! A question to the person at the terminal, with an answer of yes or no.

use std::io::{BufRead, Write};

/// Write `question` to `out`, read one line from `input`, and say whether the answer is yes.
///
/// `y` and `yes` in any case are yes. Any other text is no. An empty line, the end of the input,
/// and a stream that strata cannot read or write are also no: the default answer is no.
pub fn yes_or_no(question: &str, input: &mut impl BufRead, out: &mut impl Write) -> bool {
    // a question that cannot be shown or an answer that cannot be read is a no: the caller
    // never acts on a yes that the person did not give
    if write!(out, "{question}")
        .and_then(|()| out.flush())
        .is_err()
    {
        return false;
    }
    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(typed: &str) -> (bool, String) {
        let mut out = Vec::new();
        let yes = yes_or_no("Send? [y/N] ", &mut typed.as_bytes(), &mut out);
        (yes, String::from_utf8(out).unwrap())
    }

    #[test]
    fn the_question_is_written_without_a_new_line_and_y_or_yes_in_any_case_is_yes() {
        for typed in ["y\n", "Y\n", "yes\n", "YES\n", " y \n", "y"] {
            assert_eq!(
                answer(typed),
                (true, "Send? [y/N] ".to_string()),
                "{typed:?}"
            );
        }
    }

    #[test]
    fn any_other_answer_the_empty_line_and_the_end_of_the_input_are_no() {
        for typed in ["n\n", "N\n", "no\n", "\n", "", "yep\n", "1\n", "ye\n"] {
            assert!(!answer(typed).0, "{typed:?}");
        }
    }

    #[test]
    fn a_stream_that_cannot_be_read_or_written_is_no() {
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("closed"))
            }
        }
        impl std::io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("closed"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut input = std::io::BufReader::new(Broken);
        assert!(!yes_or_no("?", &mut input, &mut Vec::new()));
        assert!(!yes_or_no("?", &mut "y\n".as_bytes(), &mut Broken));
    }
}
