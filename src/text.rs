//! The texts that a message builds:
//!
//! - A phrase from a list of names, and the name of an object.
//! - A long text cut to a length.
//! - A mark for an empty text, and a text in a cell of a markdown table.
//!
//! `strata-preflight` calls the public ones, so both tools write each of these in one way.

/// The name of an object, or its uid when Palette gives no name.
pub fn name_or_uid<'a>(name: &'a str, uid: &'a str) -> &'a str {
    if name.is_empty() { uid } else { name }
}

/// `one` when `n` is 1, else `many`: the noun that agrees with a count, such as `cluster` or
/// `clusters`.
pub(crate) fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// `items` as one phrase: `a`, `a and b`, or `a, b, and c`. Empty text for no item.
///
/// Each message of strata that names a list of things builds it here, so each one puts the comma
/// before `and` in the same way.
pub(crate) fn and_list<S: AsRef<str>>(items: &[S]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_string(),
        [a, b] => format!("{} and {}", a.as_ref(), b.as_ref()),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{}, and {}", rest.join(", "), last.as_ref())
        }
    }
}

/// `s`, cut to at most `n` characters: its first `n - 1` characters and `…` when it is longer.
///
/// Each message and each column of a report that shows the start of a long text cuts it here.
/// So each one marks the cut in the same way. An `n` of zero gives empty text.
pub fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let Some(kept) = n.checked_sub(1) else {
        return String::new();
    };
    let mut part: String = s.chars().take(kept).collect();
    part.push('…');
    part
}

/// `s`, or `-` when `s` is empty.
///
/// Each report that shows a field with no value shows this one mark.
pub fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// `s` as the text of one cell of a markdown table.
///
/// A `|` ends a cell, and a line break ends the row. So the text escapes each `|`, and writes a
/// line break as a space.
pub fn markdown_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noun_agrees_with_its_count_and_only_one_takes_the_singular() {
        assert_eq!(plural(0, "cluster", "clusters"), "clusters");
        assert_eq!(plural(1, "cluster", "clusters"), "cluster");
        assert_eq!(plural(2, "cluster", "clusters"), "clusters");
    }

    #[test]
    fn an_object_shows_its_name_and_its_uid_only_when_it_has_no_name() {
        assert_eq!(name_or_uid("lab", "u1"), "lab");
        assert_eq!(name_or_uid("", "u1"), "u1");
        assert_eq!(name_or_uid("", ""), "");
    }

    #[test]
    fn a_cut_text_has_at_most_n_characters_and_ends_with_a_mark() {
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("abcd", 3), "ab…");
        assert_eq!(cut("abcd", 3).chars().count(), 3);
        // a character of two bytes is one character, and no cut stops in it
        assert_eq!(cut("ééééé", 4), "ééé…");
        assert_eq!(cut("abcd", 1), "…");
        assert_eq!(cut("abcd", 0), "");
        assert_eq!(cut("", 0), "");
    }

    #[test]
    fn an_empty_text_shows_a_dash_and_another_text_shows_as_it_is() {
        assert_eq!(dash(""), "-");
        assert_eq!(dash("lab"), "lab");
    }

    #[test]
    fn a_cell_of_a_markdown_table_holds_no_bar_and_no_line_break() {
        assert_eq!(markdown_cell("a|b"), "a\\|b");
        assert_eq!(markdown_cell("first\nsecond"), "first second");
        assert_eq!(markdown_cell("plain"), "plain");
    }

    #[test]
    fn a_list_of_names_has_a_comma_before_and_only_from_three_names() {
        assert_eq!(and_list::<&str>(&[]), "");
        assert_eq!(and_list(&["os"]), "os");
        assert_eq!(and_list(&["os", "k8s"]), "os and k8s");
        assert_eq!(and_list(&["os", "k8s", "cni"]), "os, k8s, and cni");
        assert_eq!(
            and_list(&[
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]),
            "a, b, c, and d"
        );
    }
}
