//! The order of Palette version strings: Semantic Versioning 2.0.0 where a version parses, the
//! first three numbers where a text holds them, and the natural order of any other text, below
//! each version.

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
/// [`is_pre_release`]. Two strings with one kind of tail keep the natural order, and a `v` in
/// front does not count: `v1.1.8.1` equals `1.1.8.1`.
///
/// A string with no three numbers, such as `1.2.develop`, holds no version for Palette. Strata
/// puts it below each version, so `1.10.0` is above `1.2.develop`. The empty string is the lowest.
///
/// Two strings with no three numbers keep the natural order. Runs of digits compare as numbers,
/// and a zero in front does not count. Runs of other characters compare as text. When one string
/// is a prefix of the other, the longer one is higher. This orders `LTS__24.x` above `LTS__22.04`.
/// The order of all strings is total.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (Version::read(a), Version::read(b)) {
        (Some(x), Some(y)) => x.precedence(&y),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
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

/// True when `v` is a pre-release, such as `1.0.1-rc.1`, `1.1.8.develop`, or `chart-1.2.3`.
///
/// Three forms are a pre-release:
///
/// - A version that parses, with a pre-release after `-`: `1.0.1-rc.1` or `1.0.1-g0123456789ab`.
/// - A string with a tail of text after its three numbers: `1.1.8.develop` or `1.2.3.4-rc1`.
/// - A string with text before its three numbers: `chart-1.2.3`. One `v` in front is no such text.
///
/// Strata puts a string of the last two forms below the version of its numbers: see [`compare`].
/// A tail of numbers, as in `10.4.0.1` and `v10.4.0.1`, is no pre-release. Build data, as in
/// `1.0.1+2`, is no pre-release. A string with no three numbers, such as `1.2.develop`, is no
/// pre-release: it holds no version.
pub fn is_pre_release(v: &str) -> bool {
    Version::read(v).is_some_and(|s| s.is_pre_release())
}

/// The item of `items` whose version is `wanted`, or the item with the highest version by
/// [`compare`] when `wanted` is [`LATEST`].
///
/// `version_of` gives the version of an item. For [`LATEST`], a pre-release by
/// [`is_pre_release`] is the highest only when no item has a version that is not a pre-release. A
/// string with no three numbers holds no version, so it is the highest only when no item holds a
/// version. None when no item has the version, and when `items` is empty.
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
    let any_final = items.iter().any(|i| is_final(version_of(i)));
    items
        .into_iter()
        .filter(|i| !any_final || is_final(version_of(i)))
        .max_by(|a, b| compare(version_of(a), version_of(b)))
}

/// True when `v` holds a version that is not a pre-release. [`LATEST`] takes such a version first.
fn is_final(v: &str) -> bool {
    Version::read(v).is_some_and(|s| !s.is_pre_release())
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
    /// The text of the version with no `v` in front. It breaks a tie of two texts with one tail.
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
        // one `v` in front counts neither for the tail nor for the tie break
        let plain = matches!(&v[..found.start], "" | "v");
        let numbers_only = v[found.end..]
            .strip_prefix('.')
            .is_some_and(|rest| rest.split('.').all(is_digits));
        let tail = if plain && numbers_only {
            Tail::Numbers
        } else {
            Tail::Text
        };
        Some(Version {
            numbers,
            count: 3,
            pre: Vec::new(),
            tail,
            text: if plain { &v[found.start..] } else { v },
        })
    }

    /// The version in `v`, or None when `v` does not parse. A number that does not fit in a `u64`
    /// does not parse, as Palette refuses it.
    fn parse(v: &'a str) -> Option<Self> {
        let v = v.strip_prefix('v').unwrap_or(v);
        let text = v;
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

    /// True when the version is a pre-release: see [`is_pre_release`].
    fn is_pre_release(&self) -> bool {
        !self.pre.is_empty() || self.tail == Tail::Text
    }

    /// The precedence of Semantic Versioning 2.0.0, item 11, and then the order that strata adds.
    ///
    /// The numbers decide first, and then the pre-release. A version is above its pre-releases.
    /// Of two pre-releases, the first identifier that differs decides. A number is below a text,
    /// numbers compare as numbers, and texts compare as bytes. The longer list is higher when the
    /// shorter one is its prefix.
    ///
    /// Palette sees no difference after these two steps, and strata adds two more. The kind of
    /// [`Tail`] decides, and then the natural order of the text with no `v` in front. Two versions
    /// that parse have no tail, so they are equal.
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
    fn a_string_that_does_not_parse_is_no_semantic_version() {
        for text in [
            "LTS__24.x",
            "1.2.3.4",
            "1.0.0-",
            "1.0.0-a_b",
            "1..2",
            "1.2.develop",
            "99999999999999999999999.0.0",
        ] {
            assert!(!parses(text), "{text} does not parse");
        }
    }

    /// Issue #714. Palette reads no version in `1.2.develop`, and strata put it above each version.
    #[test]
    fn a_string_with_no_three_numbers_is_below_each_version_and_keeps_the_natural_order() {
        gt("1.10.0", "1.2.develop");
        gt("1.9.0", "1.2.develop");
        gt("0.0.1", "1.2.develop");
        gt("22.04", "LTS__24.x");
        gt("1.2.3", "1.2.x");
        // a pre-release and a tail of text hold a version, so they are above it too
        gt("1.0.0-rc.1", "1.2.develop");
        gt("1.1.8.develop", "1.2.develop");
        gt("chart-1.0.0", "1.2.develop");
        // a number that does not fit is no version, as Palette refuses it
        gt("2.0.0", "99999999999999999999999.0.0");
        // two such strings keep the natural order, and the empty string is the lowest
        gt("LTS__24.x", "LTS__22.04");
        gt("1.10.develop", "1.2.develop");
        gt("1.2.develop", "");
    }

    /// Issue #714. Palette reads `v1.1.8.1`, `1.1.8.1`, `v1.1.8`, and `1.1.8` as `1.1.8`. Strata
    /// put `v1.1.8.1` above `1.1.8.1`, as the tie break read the `v`.
    #[test]
    fn a_v_in_front_gives_the_same_rank_with_three_numbers_and_with_four() {
        eq("v1.1.8", "1.1.8");
        eq("v1.1.8.1", "1.1.8.1");
        eq("v1.1.8.develop", "1.1.8.develop");
        gt("1.1.8.2", "v1.1.8.1");
        gt("v1.1.8.1", "v1.1.8");
        // only a `v` right in front of the numbers does not count
        gt("vchart-1.1.8", "chart-1.1.8");
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
            "v1.1.8.1",
            "v1.1.8.develop",
            "chart-1.1.8",
            "1.1.10",
            "1.2.develop",
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
        gt("22.04", "LTS__24.x");
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
        // a string with no three numbers holds no version, so it is no pre-release
        assert!(!is_pre_release("LTS__24.x"));
        assert!(!is_pre_release("1.2.develop"));
        // a tail of text after the three numbers is a pre-release, and a tail of numbers is not
        assert!(is_pre_release("1.1.8.develop"));
        assert!(is_pre_release("v1.1.8.develop"));
        assert!(is_pre_release("1.2.3.4-rc1"));
        assert!(!is_pre_release("10.4.0.1"));
        assert!(!is_pre_release("v10.4.0.1"));
        // text before the three numbers is a pre-release, also with a tail of numbers
        assert!(is_pre_release("chart-1.2.3"));
        assert!(is_pre_release("chart-1.2.3.4"));
        assert!(is_pre_release("vv1.2.3"));
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

    /// Issue #714: `@latest` took `1.2.develop` over `1.10.0`.
    #[test]
    fn latest_skips_a_string_with_no_three_numbers_while_a_version_exists() {
        let short = ["1.2.develop", "1.10.0", "1.9.0"];
        assert_eq!(pick(short, |v| *v, LATEST), Some("1.10.0"));
        // a pre-release holds a version, so `@latest` takes it first
        assert_eq!(
            pick(["1.2.develop", "1.0.0-rc.1"], |v| *v, LATEST),
            Some("1.0.0-rc.1")
        );
        assert_eq!(
            pick(["1.2.develop", "1.1.8.develop"], |v| *v, LATEST),
            Some("1.1.8.develop")
        );
        // with no version, the natural order decides
        assert_eq!(
            pick(["1.2.develop", "1.10.develop"], |v| *v, LATEST),
            Some("1.10.develop")
        );
        assert_eq!(pick(short, |v| *v, "1.2.develop"), Some("1.2.develop"));
    }

    #[test]
    fn text_runs_compare_as_text() {
        gt("b", "a");
        gt("1.2b", "1.2a");
    }
}
