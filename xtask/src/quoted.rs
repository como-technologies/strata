//! The read of a text in double quotes with the escapes of C.
//!
//! A text of Go has these escapes, and git writes a file name in quotes with them. The Go lexer of
//! `xtask copies` and the reader of a diff of `git_diff` both read such a text here. So xtask has
//! one table of the escapes.

/// The bytes of `quoted`, a text in double quotes, with its escapes decoded.
///
/// The error names an escape that is not Go. It also names a `quoted` with no quote at its start,
/// or with more text after its end quote.
pub fn unquote(quoted: &str) -> Result<Vec<u8>, String> {
    let Some(rest) = quoted.strip_prefix('"') else {
        return Err(format!("{quoted} does not start with a quote"));
    };
    let chars: Vec<char> = rest.chars().collect();
    let (bytes, length) = read(&chars, 1)?;
    if length < chars.len() {
        return Err(format!("{quoted} has more text after its end quote"));
    }
    Ok(bytes)
}

/// The bytes of the text in `chars`, which start after the open quote, with its escapes decoded.
/// It also gives the count of the chars of the text and of its end quote. `line` is the line of
/// the text, for an error.
///
/// The error names an escape that is not Go, or a text with no end quote on its line.
pub fn read(chars: &[char], line: usize) -> Result<(Vec<u8>, usize), String> {
    let mut bytes = Vec::new();
    let mut at = 0;
    loop {
        match chars.get(at) {
            Some('"') => return Ok((bytes, at + 1)),
            Some('\\') => at += escape(&chars[at..], line, &mut bytes)?,
            Some('\n') | None => return Err(no_end(line)),
            Some(c) => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
                at += 1;
            }
        }
    }
}

/// The error of a text at `line` that does not end.
fn no_end(line: usize) -> String {
    format!("line {line}: a text does not end")
}

/// Decode the escape at the start of `chars` into `bytes`, and give the count of its chars.
fn escape(chars: &[char], line: usize, bytes: &mut Vec<u8>) -> Result<usize, String> {
    let Some(&kind) = chars.get(1) else {
        return Err(no_end(line));
    };
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
        return Ok(2);
    }
    // the digits start after the kind, and the first digit of an octal escape is the kind
    let (start, count, radix) = match kind {
        'x' => (2, 2, 16),
        'u' => (2, 4, 16),
        'U' => (2, 8, 16),
        '0'..='7' => (1, 3, 8),
        _ => return Err(format!("line {line}: the escape \\{kind} is not Go")),
    };
    let digits: String = chars.iter().skip(start).take(count).collect();
    let number = u32::from_str_radix(&digits, radix)
        .map_err(|e| format!("line {line}: the escape {digits}: {e}"))?;
    if matches!(kind, 'u' | 'U') {
        // Go refuses a number that is no char, such as a surrogate, and git writes no such escape
        let c = char::from_u32(number).unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut buffer = [0; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
    } else {
        bytes.push(u8::try_from(number).map_err(|e| format!("line {line}: {e}"))?);
    }
    Ok(start + count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquote_gives_the_bytes_of_one_whole_text_in_quotes() {
        assert_eq!(
            unquote("\"caf\\303\\251 \\\"\\t\\351\"").unwrap(),
            b"caf\xc3\xa9 \"\t\xe9"
        );
        for wrong in ["caf.rs", "\"a\" b", "\"open", "\"\\q\""] {
            assert!(unquote(wrong).is_err(), "{wrong}");
        }
    }

    /// The criterion of #960. The read gives the count of the chars of the text and of its end
    /// quote, so the lexer can read the token after the text.
    #[test]
    fn the_read_of_a_text_gives_its_bytes_and_its_length_with_the_end_quote() {
        let chars: Vec<char> = "a\\x41é\" rest".chars().collect();
        assert_eq!(read(&chars, 3).unwrap(), ("aAé".as_bytes().to_vec(), 7));
        let open: Vec<char> = "open\nnext\"".chars().collect();
        assert_eq!(read(&open, 3).unwrap_err(), "line 3: a text does not end");
        let cut: Vec<char> = "cut\\".chars().collect();
        assert_eq!(read(&cut, 4).unwrap_err(), "line 4: a text does not end");
    }
}
