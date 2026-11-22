//! The form of a uid of Palette.

/// The fewest characters of a uid. A uid of Palette has 24 hexadecimal digits, and a UUID has more.
const UID_LENGTH: usize = 24;

/// True when `text` is as long as a uid and holds only hexadecimal digits and hyphens. A word
/// holds other letters, and a placeholder such as `{uid}` holds braces.
pub(crate) fn looks_like_uid(text: &str) -> bool {
    text.len() >= UID_LENGTH && text.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uid_of_palette_and_a_uuid_have_the_form_of_a_uid_and_a_name_and_a_short_text_have_not() {
        assert!(looks_like_uid("64f20000000000000000a0d1"));
        assert!(looks_like_uid("0f8c1c3e-5b1a-4c2d-9e7f-1a2b3c4d5e6f"));
        assert!(!looks_like_uid("production"));
        assert!(!looks_like_uid("64f20000000000000000a0d"));
        assert!(!looks_like_uid("{uid}"));
    }
}
