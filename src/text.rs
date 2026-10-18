//! The texts that a message builds: a phrase from a list of names, the name of an object, and a
//! long text cut to a length.

/// The name of an object, or its uid when Palette gives no name.
pub(crate) fn name_or_uid<'a>(name: &'a str, uid: &'a str) -> &'a str {
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

#[cfg(test)]
mod tests {
    use super::*;

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
