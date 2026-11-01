//! The order of Palette version strings: Semantic Versioning 2.0.0 where a version parses, and the
//! natural order of the text where it does not.

use std::cmp::Ordering;

use crate::reference::LATEST;

/// Compare two version strings as Palette does.
///
/// Palette orders by semantic version. This function does the same when both strings parse as
/// Palette parses a version. Such a version has an optional `v`, one to three numbers, an optional
/// pre-release after `-`, and optional build data after `+`. A pre-release is below its version:
/// `1.0.1-rc.1` is below `1.0.1`. Build data has no precedence: `1.0.1+2` equals `1.0.1`. A
/// missing number is `0`.
///
/// A string that does not parse keeps the natural order. Runs of digits compare as numbers, and a
/// zero in front does not count. Runs of other characters compare as text. When one string is a
/// prefix of the other, the longer one is higher. This orders `LTS__24.x` above `LTS__22.04`. The
/// order of all strings is total: the empty string is the lowest, then each string that parses,
/// then each string that does not.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (Semver::parse(a), Semver::parse(b)) {
        (Some(x), Some(y)) => x.precedence(&y),
        (Some(_), None) => {
            if b.is_empty() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (None, Some(_)) => {
            if a.is_empty() {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (None, None) => natural(a, b),
    }
}

/// True when `v` parses as Palette parses a version: see [`compare`].
pub(crate) fn parses(v: &str) -> bool {
    Semver::parse(v).is_some()
}

/// The numbers of `v` as it writes them: one to three. None when `v` does not parse.
///
/// `4.10.3-rc.1` gives `[4, 10, 3]`, and `4.10` gives `[4, 10]`. A range of releases reads the
/// numbers that a bound names, and omits the others.
pub fn numbers(v: &str) -> Option<Vec<u64>> {
    Semver::parse(v).map(|s| s.numbers[..s.count].to_vec())
}

/// True when `v` parses and has a pre-release: `1.0.1-rc.1`, `1.0.1-g0123456789ab`. A string that
/// does not parse is not a pre-release.
pub fn is_pre_release(v: &str) -> bool {
    Semver::parse(v).is_some_and(|s| !s.pre.is_empty())
}

/// The item of `items` whose version is `wanted`, or the item with the highest version by
/// [`compare`] when `wanted` is [`LATEST`].
///
/// `version_of` gives the version of an item. For [`LATEST`], a pre-release is the highest only
/// when no item has another version. None when no item has the version, and when `items` is
/// empty.
pub fn pick<T>(
    items: impl IntoIterator<Item = T>,
    version_of: impl Fn(&T) -> &str,
    wanted: &str,
) -> Option<T> {
    let items = items.into_iter();
    if wanted != LATEST {
        let mut items = items;
        return items.find(|item| version_of(item) == wanted);
    }
    let items: Vec<T> = items.collect();
    let any_final = items.iter().any(|i| !is_pre_release(version_of(i)));
    items
        .into_iter()
        .filter(|i| !any_final || !is_pre_release(version_of(i)))
        .max_by(|a, b| compare(version_of(a), version_of(b)))
}

/// A version that parses as Palette parses a version.
struct Semver<'a> {
    /// The three numbers. A number that the version omits is `0`.
    numbers: [u64; 3],
    /// How many numbers the version writes, from one to three.
    count: usize,
    /// The identifiers of the pre-release. Empty for a version that is not a pre-release.
    pre: Vec<&'a str>,
}

impl<'a> Semver<'a> {
    /// The version in `v`, or None when `v` does not parse. A number that does not fit in a `u64`
    /// does not parse, as Palette refuses it.
    fn parse(v: &'a str) -> Option<Self> {
        let v = v.strip_prefix('v').unwrap_or(v);
        let (v, build) = v.split_once('+').map_or((v, None), |(a, b)| (a, Some(b)));
        let (core, pre) = v.split_once('-').map_or((v, None), |(a, b)| (a, Some(b)));
        let mut numbers = [0; 3];
        let mut count = 0;
        for n in core.split('.') {
            if !is_digits(n) {
                return None;
            }
            *numbers.get_mut(count)? = n.parse().ok()?;
            count += 1;
        }
        let ids = |s: &'a str| -> Option<Vec<&'a str>> {
            let ids: Vec<&str> = s.split('.').collect();
            let valid = ids.iter().all(|id| {
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
            valid.then_some(ids)
        };
        if let Some(build) = build {
            ids(build)?;
        }
        let pre = match pre {
            Some(pre) => ids(pre)?,
            None => Vec::new(),
        };
        Some(Semver {
            numbers,
            count,
            pre,
        })
    }

    /// The precedence of Semantic Versioning 2.0.0, item 11: the numbers, then the pre-release.
    /// A version is above its pre-releases. Of two pre-releases, the first identifier that
    /// differs decides: a number is below a text, numbers compare as numbers, texts compare as
    /// bytes, and the longer list is higher when the shorter one is its prefix.
    fn precedence(&self, other: &Self) -> Ordering {
        self.numbers.cmp(&other.numbers).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre_cmp(other),
            }
        })
    }

    /// The order of two pre-releases, item 11.4 of Semantic Versioning 2.0.0.
    ///
    /// Compare the identifiers from the left, and the first pair that differs decides. A number is
    /// below a text. Two numbers compare as numbers, and two texts as bytes. When one list is the
    /// start of the other, the longer list is higher.
    fn pre_cmp(&self, other: &Self) -> Ordering {
        for (x, y) in self.pre.iter().zip(&other.pre) {
            let ord = match (is_digits(x), is_digits(y)) {
                (true, true) => numeric(x, y),
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (false, false) => x.cmp(y),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        self.pre.len().cmp(&other.pre.len())
    }
}

/// The natural order of two strings: see [`compare`].
///
/// The steps:
///
/// 1. Split each string into runs of digits and runs of other characters.
/// 2. Compare the runs from the left. Two digit runs compare as numbers. Any other pair compares
///    as text.
/// 3. The first pair of runs that differs decides.
/// 4. When all pairs are equal, the string with more runs is higher.
fn natural(a: &str, b: &str) -> Ordering {
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

    fn eq(a: &str, b: &str) {
        assert_eq!(compare(a, b), Ordering::Equal, "{a} = {b}");
        assert_eq!(compare(b, a), Ordering::Equal, "{b} = {a}");
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
        eq("LTS__20.04", "LTS__20.4");
        gt("1.29.4-rc2", "1.29.4-rc1");
    }

    #[test]
    fn a_pre_release_is_below_its_version() {
        gt("1.0.1", "1.0.1-g0123456789ab");
        gt("1.0.1", "1.0.1-mr12.345");
        gt("1.0.1", "1.0.1-rc.1");
        gt("1.21.1", "1.21.1-rev1");
        gt("1.0.2-rc.1", "1.0.1");
    }

    #[test]
    fn pre_releases_order_by_their_identifiers_as_semver_does() {
        // the example of Semantic Versioning 2.0.0, item 11
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for pair in chain.windows(2) {
            gt(pair[1], pair[0]);
        }
        // a number is below a text, and two texts compare as bytes
        gt("1.0.0-alpha", "1.0.0-1");
        gt("1.0.0-rev9", "1.0.0-rev10");
        gt("1.0.1-g0123456789ac", "1.0.1-g0123456789ab");
    }

    #[test]
    fn build_data_has_no_precedence() {
        eq("1.0.1+2", "1.0.1");
        eq("1.0.1+2", "1.0.1+3");
        gt("1.0.1", "1.0.1-rc.1+5");
    }

    #[test]
    fn the_prefix_and_the_missing_numbers_do_not_count() {
        eq("v1.2.3", "1.2.3");
        eq("1.2", "1.2.0");
        eq("1", "1.0.0");
        gt("1.2.1", "1.2");
        eq("22.04", "22.4");
    }

    #[test]
    fn a_string_that_does_not_parse_keeps_the_natural_order() {
        for text in [
            "LTS__24.x",
            "1.2.3.4",
            "1.0.0-",
            "1.0.0-a_b",
            "1..2",
            "99999999999999999999999.0.0",
        ] {
            assert!(!parses(text), "{text} does not parse");
        }
        gt("1.2.3.5", "1.2.3.4");
        gt("1.2.3.10", "1.2.3.9");
    }

    #[test]
    fn the_order_of_all_strings_is_total() {
        let versions = [
            "",
            "1",
            "1.0.0-a_b",
            "1.0.0-rc.1",
            "1.0.0",
            "1.2.3.4",
            "LTS__22.04",
            "LTS__24.x",
            "b",
        ];
        for a in versions {
            for b in versions {
                for c in versions {
                    if compare(a, b).is_le() && compare(b, c).is_le() {
                        assert!(compare(a, c).is_le(), "{a} <= {b} <= {c}");
                    }
                }
            }
        }
        gt("1", "");
        gt("LTS__24.x", "22.04");
        gt("b", "a");
    }

    #[test]
    fn a_longer_version_is_higher_when_the_prefix_is_equal() {
        gt("1.2.1", "1.2");
        eq("1.2.0", "1.2.0");
        eq("", "");
        gt("1", "");
    }

    #[test]
    fn numbers_gives_the_numbers_that_the_version_writes() {
        assert_eq!(numbers("4.10.3-rc.1"), Some(vec![4, 10, 3]));
        assert_eq!(numbers("v4.10"), Some(vec![4, 10]));
        assert_eq!(numbers("22"), Some(vec![22]));
        assert_eq!(numbers("LTS__24.x"), None);
        assert_eq!(numbers("1.2.3.4"), None);
    }

    #[test]
    fn only_a_version_with_a_pre_release_is_one() {
        assert!(is_pre_release("1.0.1-g0123456789ab"));
        assert!(is_pre_release("v1.0.1-rc.1+5"));
        assert!(!is_pre_release("1.0.1"));
        assert!(!is_pre_release("1.0.1+2"));
        assert!(!is_pre_release("LTS__24.x"));
        assert!(!is_pre_release("1.2.3.4-rc1"));
    }

    #[test]
    fn pick_gives_the_wanted_version_or_the_highest_for_latest() {
        let versions = ["1.9.0", "1.10.0", "1.2.0"];
        assert_eq!(pick(versions, |v| *v, "1.9.0"), Some("1.9.0"));
        assert_eq!(pick(versions, |v| *v, LATEST), Some("1.10.0"));
        assert_eq!(pick(versions, |v| *v, "2.0.0"), None);
        assert_eq!(pick(Vec::<&str>::new(), |v| *v, LATEST), None);
    }

    #[test]
    fn latest_takes_the_highest_version_that_is_not_a_pre_release() {
        let versions = ["1.0.1", "1.0.1-g0123456789ab", "1.0.2-rc.1", "1.0.0"];
        assert_eq!(pick(versions, |v| *v, LATEST), Some("1.0.1"));
        // a pre-release is the answer only when the name has no other version
        let only = ["1.0.2-rc.2", "1.0.2-rc.1"];
        assert_eq!(pick(only, |v| *v, LATEST), Some("1.0.2-rc.2"));
        // an exact version still names a pre-release
        assert_eq!(pick(versions, |v| *v, "1.0.2-rc.1"), Some("1.0.2-rc.1"));
    }

    #[test]
    fn text_runs_compare_as_text() {
        gt("b", "a");
        gt("1.2b", "1.2a");
    }
}
