//! The order of Palette version strings: Semantic Versioning 2.0.0 where a version parses, the
//! first three numbers where a text holds them, and the natural order of any other text.

use std::cmp::Ordering;
use std::ops::Range;

use crate::reference::LATEST;

/// Compare two version strings as Palette does.
///
/// Palette orders by semantic version. This function does the same when both strings parse as
/// Palette parses a version. Such a version has an optional `v`, one to three numbers, an optional
/// pre-release after `-`, and optional build data after `+`. A pre-release is below its version:
/// `1.0.1-rc.1` is below `1.0.1`. Build data has no precedence: `1.0.1+2` equals `1.0.1`. A
/// missing number is `0`.
///
/// Palette reads a string that does not parse as the first three numbers that it holds. So
/// `1.1.8.develop` reads as `1.1.8`, and `10.4.0.1` reads as `10.4.0`. Palette gives no order to
/// two strings that read as one version, and strata adds it. A tail of numbers, such as the `.1`
/// of `10.4.0.1`, puts the string above the version. Any other text, such as the `.develop` of
/// `1.1.8.develop`, puts it below the version, and makes it a pre-release: see
/// [`is_pre_release`]. Two strings with one kind of tail keep the natural order.
///
/// A string with no three numbers keeps the natural order. Runs of digits compare as numbers, and
/// a zero in front does not count. Runs of other characters compare as text. When one string is a
/// prefix of the other, the longer one is higher. This orders `LTS__24.x` above `LTS__22.04`. The
/// order of all strings is total. The empty string is the lowest. Then come the strings that parse
/// or hold three numbers, and then each other string.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (Version::read(a), Version::read(b)) {
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
    Version::parse(v).is_some()
}

/// True when `text` holds three numbers with dots between them, `\d+\.\d+\.\d+`, at any place.
/// Palette reads a version that does not parse as these numbers: see [`compare`].
pub(crate) fn has_three_numbers(text: &str) -> bool {
    three_numbers(text).is_some()
}

/// The numbers of `v` as it writes them: one to three. None when `v` does not parse.
///
/// `4.10.3-rc.1` gives `[4, 10, 3]`, and `4.10` gives `[4, 10]`. A range of releases reads the
/// numbers that a bound names, and omits the others.
pub fn numbers(v: &str) -> Option<Vec<u64>> {
    Version::parse(v).map(|s| s.numbers[..s.count].to_vec())
}

/// The release line of `v`: its first `count` numbers, joined by dots. A number that `v` omits is
/// `0`. None when `v` does not parse.
///
/// `4.10.3-rc.1` with a `count` of 2 gives `4.10`, and `4` gives `4.0`. Two versions are one
/// release when their lines are equal.
pub fn line(v: &str, count: usize) -> Option<String> {
    let semver = Version::parse(v)?;
    let numbers: Vec<String> = (0..count)
        .map(|i| semver.numbers.get(i).copied().unwrap_or(0).to_string())
        .collect();
    Some(numbers.join("."))
}

/// True when `v` is a pre-release: `1.0.1-rc.1`, `1.0.1-g0123456789ab`, or `1.1.8.develop`.
///
/// A version that parses is a pre-release when it has one. A string with a tail of text after
/// its three numbers is a pre-release too, because strata puts it below the version of its
/// numbers: see [`compare`]. A tail of numbers, as in `10.4.0.1`, is no pre-release. A string
/// with no three numbers is no pre-release.
pub fn is_pre_release(v: &str) -> bool {
    Version::read(v).is_some_and(|s| !s.pre.is_empty() || s.tail == Tail::Text)
}

/// True when `a` and `b` both parse and write the same numbers: see [`numbers`].
///
/// Only the pre-release or the build data can differ. So `1.66.0-g0123456789ab` and `1.66.0` keep
/// their numbers, and so do `1.2.0+build.5` and `1.2.0`. `20.04` and `24.04` do not, and neither do
/// `1.2.0-build5` and `2026.11.0`. A string that does not parse keeps no numbers.
pub fn keeps_numbers(a: &str, b: &str) -> bool {
    match (numbers(a), numbers(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// True when `version` is the final version of the pre-release `pre`, in Semantic Versioning.
///
/// Both strings parse, `pre` has a pre-release and `version` has none, and the two have the same
/// numbers. A number that a version omits is `0`, and build data does not count. So `1.2.0` is the
/// final version of `1.2.0-rc.1` and of `v1.2-g0123456789ab+5`. A string that does not parse,
/// such as `1.1.8.develop`, is the pre-release of no version.
pub fn is_final_of(version: &str, pre: &str) -> bool {
    match (Version::parse(version), Version::parse(pre)) {
        (Some(v), Some(p)) => v.pre.is_empty() && !p.pre.is_empty() && v.numbers == p.numbers,
        _ => false,
    }
}

/// The item of `items` whose version is `wanted`, or the item with the highest version by
/// [`compare`] when `wanted` is [`LATEST`].
///
/// `version_of` gives the version of an item. For [`LATEST`], a pre-release by
/// [`is_pre_release`] is the highest only when no item has a version that is not a pre-release.
/// None when no item has the version, and when `items` is empty.
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

/// A version as Palette reads it: a version that parses, or the first three numbers of a text.
struct Version<'a> {
    /// The three numbers. A number that the version omits is `0`.
    numbers: [u64; 3],
    /// How many numbers the version writes, from one to three.
    count: usize,
    /// The identifiers of the pre-release. Empty for a version that is not a pre-release.
    pre: Vec<&'a str>,
    /// What the text holds beside its three numbers. [`Tail::None`] for a version that parses.
    tail: Tail,
    /// The whole text of the version.
    text: &'a str,
}

/// What a text that does not parse holds beside its three numbers. Palette reads only the
/// numbers, so strata orders two texts with one set of numbers by the tail.
///
/// The derived order is the order of the variants, from the lowest version to the highest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tail {
    /// Any other text, such as `.develop` in `1.1.8.develop`. It is below the version.
    Text,
    /// No tail: the version parses.
    None,
    /// Only dots and numbers after the three numbers, such as `.1` in `10.4.0.1`, with an
    /// optional `v` in front. It is above the version.
    Numbers,
}

impl<'a> Version<'a> {
    /// The version that Palette reads in `v`: the version that `v` parses as, or else the first
    /// three numbers of `v`. None when `v` has no three numbers.
    fn read(v: &'a str) -> Option<Self> {
        Self::parse(v).or_else(|| Self::three_numbers_of(v))
    }

    /// The first three numbers of `v` and the tail around them, as Palette reads a version that
    /// does not parse. None when `v` has no three numbers, or a number does not fit in a `u64`.
    fn three_numbers_of(v: &'a str) -> Option<Self> {
        let found = three_numbers(v)?;
        let mut numbers = [0; 3];
        for (slot, n) in numbers.iter_mut().zip(v[found.clone()].split('.')) {
            *slot = n.parse().ok()?;
        }
        let head = &v[..found.start];
        let numbers_only = v[found.end..]
            .strip_prefix('.')
            .is_some_and(|rest| rest.split('.').all(is_digits));
        let tail = if (head.is_empty() || head == "v") && numbers_only {
            Tail::Numbers
        } else {
            Tail::Text
        };
        Some(Version {
            numbers,
            count: 3,
            pre: Vec::new(),
            tail,
            text: v,
        })
    }

    /// The version in `v`, or None when `v` does not parse. A number that does not fit in a `u64`
    /// does not parse, as Palette refuses it.
    fn parse(v: &'a str) -> Option<Self> {
        let text = v;
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
        Some(Version {
            numbers,
            count,
            pre,
            tail: Tail::None,
            text,
        })
    }

    /// The precedence of Semantic Versioning 2.0.0, item 11, and then the order that strata adds.
    ///
    /// The numbers decide first, and then the pre-release. A version is above its pre-releases.
    /// Of two pre-releases, the first identifier that differs decides. A number is below a text,
    /// numbers compare as numbers, and texts compare as bytes. The longer list is higher when the
    /// shorter one is its prefix.
    ///
    /// Palette sees no difference after these two steps, and strata adds two more. The kind of
    /// [`Tail`] decides, and then the natural order of the text. Two versions that parse have no
    /// tail, so they are equal.
    fn precedence(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre_cmp(other),
            })
            .then_with(|| self.tail.cmp(&other.tail))
            .then_with(|| match self.tail {
                Tail::None => Ordering::Equal,
                Tail::Text | Tail::Numbers => natural(self.text, other.text),
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

/// The place of the first three numbers in `text` with dots between them, `\d+\.\d+\.\d+`, as
/// Palette finds them. None when `text` has no such numbers.
///
/// Try each start from the left. At a start, read a run of digits, a dot, a run of digits, a dot,
/// and a run of digits. The first start that gives all five decides. The last run takes each digit
/// that follows.
fn three_numbers(text: &str) -> Option<Range<usize>> {
    let b = text.as_bytes();
    let digits_from = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    (0..b.len()).find_map(|start| {
        let mut i = start;
        for part in 0..3 {
            let end = digits_from(i);
            if end == i {
                return None;
            }
            i = end;
            if part < 2 {
                if b.get(i) != Some(&b'.') {
                    return None;
                }
                i += 1;
            }
        }
        Some(start..i)
    })
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
    fn a_string_with_no_three_numbers_keeps_the_natural_order() {
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
        gt("LTS__24.x", "LTS__22.04");
        gt("1.2.x", "1.2.3");
        // a number that does not fit is no version, as Palette refuses it
        gt("99999999999999999999999.0.0", "2.0.0");
    }

    /// Issue #616. Palette reads `1.1.8.develop` as `1.1.8`, and strata puts it below `1.1.8`.
    #[test]
    fn a_tail_of_text_reads_as_its_three_numbers_and_is_below_them() {
        gt("1.1.8", "1.1.8.develop");
        gt("1.1.10", "1.1.8.develop");
        gt("1.1.8.develop", "1.1.7");
        // the numbers decide before the tail, as in Palette
        gt("1.2.0.develop", "1.1.10");
        // Palette reads no pre-release in such a text, so it is above a pre-release that parses
        gt("1.1.8.develop", "1.1.8-rc.1");
        // Palette finds the numbers at any place
        gt("1.2.3", "chart-1.2.3");
        gt("1.0.0", "1.0.0-a_b");
        gt("2.0.0", "1.2.3.4-rc1");
        // two tails of text keep the natural order
        gt("1.1.8.develop", "1.1.8.beta");
        eq("1.1.8.develop", "1.1.8.develop");
    }

    /// Issue #616. Palette reads `10.4.0.1` as `10.4.0`, and strata puts it above `10.4.0`.
    #[test]
    fn a_tail_of_numbers_reads_as_its_three_numbers_and_is_above_them() {
        gt("10.4.0.1", "10.4.0");
        gt("10.5.0", "10.4.0.1");
        gt("10.4.1-rc.1", "10.4.0.1");
        gt("v10.4.0.1", "10.4.0");
        // two tails of numbers keep the natural order, which compares the numbers
        gt("1.2.3.5", "1.2.3.4");
        gt("1.2.3.10", "1.2.3.9");
        gt("1.2.3.4.1", "1.2.3.4");
        // a tail of numbers is above a tail of text on the same three numbers
        gt("1.2.3.4", "1.2.3.develop");
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
            "1.1.8.develop",
            "1.1.8",
            "v1.1.8",
            "1.1.8.1",
            "chart-1.1.8",
            "1.1.10",
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
    fn line_gives_the_first_numbers_and_zero_for_a_number_that_the_version_omits() {
        assert_eq!(line("4.10.3-rc.1", 2).as_deref(), Some("4.10"));
        assert_eq!(line("v4.10.3", 3).as_deref(), Some("4.10.3"));
        assert_eq!(line("4", 2).as_deref(), Some("4.0"));
        assert_eq!(line("4.10", 1).as_deref(), Some("4"));
        assert_eq!(line("LTS__24.x", 2), None);
        assert_eq!(line("1.2.3.4", 2), None);
    }

    #[test]
    fn two_versions_with_one_line_compare_equal() {
        let (a, b) = (line("4", 2).unwrap(), line("4.0.7", 2).unwrap());
        assert_eq!(compare(&a, &b), Ordering::Equal);
    }

    #[test]
    fn only_a_version_with_a_pre_release_is_one() {
        assert!(is_pre_release("1.0.1-g0123456789ab"));
        assert!(is_pre_release("v1.0.1-rc.1+5"));
        assert!(!is_pre_release("1.0.1"));
        assert!(!is_pre_release("1.0.1+2"));
        assert!(!is_pre_release("LTS__24.x"));
        // a tail of text after the three numbers is a pre-release, and a tail of numbers is not
        assert!(is_pre_release("1.1.8.develop"));
        assert!(is_pre_release("1.2.3.4-rc1"));
        assert!(is_pre_release("chart-1.2.3"));
        assert!(!is_pre_release("10.4.0.1"));
        assert!(!is_pre_release("v10.4.0.1"));
    }

    #[test]
    fn two_versions_keep_their_numbers_when_only_the_pre_release_or_the_build_data_differs() {
        for (a, b) in [
            ("1.66.0-g0123456789ab", "1.66.0"),
            ("1.66.0-rc.1", "1.66.0-rc.2"),
            ("1.2.0-mr12.345", "1.2.0"),
            ("1.2.0+build.5", "1.2.0"),
            ("v1.2.0", "1.2.0"),
        ] {
            assert!(keeps_numbers(a, b), "{a} and {b}");
        }
        for (a, b) in [
            ("20.04", "24.04"),
            ("1.2.0-build5", "2026.11.0"),
            ("1.2", "1.2.0"),
            ("1.1.8.develop", "1.1.8"),
            ("LTS__24.x", "LTS__24.x"),
            ("", ""),
        ] {
            assert!(!keeps_numbers(a, b), "{a} and {b}");
        }
    }

    #[test]
    fn a_version_is_the_final_of_a_pre_release_with_its_numbers_in_semantic_versioning() {
        for (version, pre) in [
            ("1.2.0", "1.2.0-rc.1"),
            ("1.2.0", "1.2.0-g0123456789ab"),
            ("1.2.0", "1.2.0-mr12.345"),
            ("1.2.0+build.6", "v1.2-rc.1+build.5"),
        ] {
            assert!(is_final_of(version, pre), "{version} of {pre}");
        }
        for (version, pre) in [
            ("1.2.0-rc.2", "1.2.0-rc.1"),
            ("1.2.1", "1.2.0-rc.1"),
            ("2026.11.0", "1.2.0-build5"),
            ("1.2.0+build.6", "1.2.0+build.5"),
            ("1.2.0", "1.2.0"),
            ("1.1.8", "1.1.8.develop"),
            ("1.2.0-rc.1", "1.2.0"),
            ("", "1.2.0-rc.1"),
        ] {
            assert!(!is_final_of(version, pre), "{version} of {pre}");
        }
    }

    #[test]
    fn has_three_numbers_finds_them_at_any_place() {
        for text in [
            "1.2.3",
            "chart-1.2.3",
            "1.1.8.develop",
            "x91.2.3y",
            "1.2.3.4",
        ] {
            assert!(has_three_numbers(text), "{text}");
        }
        for text in ["", "1.2", "1.2.x", "LTS__24.x", "1..2.3", "a.b.c"] {
            assert!(!has_three_numbers(text), "{text}");
        }
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

    /// Issue #616: `@latest` took `1.1.8.develop` over `1.1.10`, and `10.4.0.1` over `10.5.0`.
    #[test]
    fn latest_skips_a_tail_of_text_and_orders_a_tail_of_numbers_by_its_three_numbers() {
        let barney = ["1.1.8", "1.1.8.develop", "1.1.10"];
        assert_eq!(pick(barney, |v| *v, LATEST), Some("1.1.10"));
        assert_eq!(pick(["10.4.0.1", "10.5.0"], |v| *v, LATEST), Some("10.5.0"));
        assert_eq!(
            pick(["10.4.0", "10.4.0.1"], |v| *v, LATEST),
            Some("10.4.0.1")
        );
        // a tail of text is a pre-release, so `@latest` skips it also when its numbers are higher
        assert_eq!(
            pick(["1.2.0.develop", "1.1.10"], |v| *v, LATEST),
            Some("1.1.10")
        );
        assert_eq!(
            pick(["1.1.8.develop"], |v| *v, LATEST),
            Some("1.1.8.develop")
        );
        assert_eq!(pick(barney, |v| *v, "1.1.8.develop"), Some("1.1.8.develop"));
    }

    #[test]
    fn text_runs_compare_as_text() {
        gt("b", "a");
        gt("1.2b", "1.2a");
    }
}
