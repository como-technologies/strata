//! The input type of a profile variable: `text`, `dropdown`, or `multiline`.

use crate::closed_set::impl_texts;

/// The input type of a profile variable, as Palette gives it in `v1VariableInputType`.
///
/// A record keeps the text, so a type that a later Palette adds stays in the record. Palette gives
/// `text` when a variable gives none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputType {
    /// `text`: one line of text.
    Text,
    /// `dropdown`: one of the options of the variable.
    Dropdown,
    /// `multiline`: text of more than one line.
    Multiline,
}

impl_texts!(pub(crate) InputType {
    Text => "text",
    Dropdown => "dropdown",
    Multiline => "multiline",
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_that_is_no_input_type_of_palette_reads_as_none() {
        assert_eq!(InputType::from_text("dropdown"), Some(InputType::Dropdown));
        assert_eq!(InputType::from_text("Dropdown"), None);
        assert_eq!(InputType::from_text(""), None);
    }
}
