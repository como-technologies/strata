//! The texts that a message builds from a list of names.

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

#[cfg(test)]
mod tests {
    use super::*;

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
