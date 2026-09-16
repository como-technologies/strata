//! Natural ordering of Palette version strings.

use std::cmp::Ordering;

/// Compare two version strings naturally: runs of digits compare as numbers (leading zeros do not
/// count), runs of anything else compare as text, and when one string is a prefix of the other
/// the longer one is higher. This orders `1.30.6` above `1.29.10`, `22.04` above `20.04`, and
/// `LTS__24.x` above `LTS__22.04`. It is not semver: a pre-release suffix sorts above its base.
pub fn compare(a: &str, b: &str) -> Ordering {
    let (ra, rb) = (runs(a), runs(b));
    for (x, y) in ra.iter().zip(rb.iter()) {
        let ord = if is_digits(x) && is_digits(y) {
            numeric(x, y)
        } else {
            x.cmp(y)
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ra.len().cmp(&rb.len())
}

/// Split into alternating runs of digits and non-digits.
fn runs(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut digits = None;
    for (i, c) in s.char_indices() {
        let d = c.is_ascii_digit();
        if digits.is_some_and(|prev| prev != d) {
            out.push(&s[start..i]);
            start = i;
        }
        digits = Some(d);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Compare two digit runs of any length by value.
fn numeric(x: &str, y: &str) -> Ordering {
    let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
    x.len().cmp(&y.len()).then_with(|| x.cmp(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn gt(a: &str, b: &str) {
        assert_eq!(compare(a, b), Ordering::Greater, "{a} > {b}");
        assert_eq!(compare(b, a), Ordering::Less, "{b} < {a}");
    }

    #[test]
    fn numeric_runs_compare_as_numbers() {
        gt("22.04", "20.04");
        gt("1.30.6", "1.29.10");
        gt("1.10.0", "1.9.0");
        gt("10", "9");
    }

    #[test]
    fn palette_tag_forms_order_by_their_numbers() {
        gt("LTS__24.x", "LTS__22.04");
        assert_eq!(compare("LTS__20.04", "LTS__20.4"), Ordering::Equal);
        gt("1.29.4-rc2", "1.29.4-rc1");
    }

    #[test]
    fn a_longer_version_is_higher_when_the_prefix_is_equal() {
        gt("1.2.1", "1.2");
        assert_eq!(compare("1.2.0", "1.2.0"), Ordering::Equal);
        assert_eq!(compare("", ""), Ordering::Equal);
        gt("1", "");
    }

    #[test]
    fn text_runs_compare_as_text() {
        gt("b", "a");
        gt("1.2b", "1.2a");
    }
}
