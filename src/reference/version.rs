//! The order of Palette version strings: Semantic Versioning 2.0.0 where a version parses, the
//! numbers of a text where it holds them, and the natural order of any other text, below each
//! version.

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
/// Palette reads a string that does not parse as the first three numbers that it holds, at any
/// place. It ignores the rest of the text, and it reads no version in a string with no three
/// numbers. Strata reads the same numbers, and the numbers decide first. For the rest, strata
/// follows the order of the crate `versions` 8.0.1. One `v` in front does not count, as in Palette.
///
/// For the numbers `1.2.0`, the forms go from the lowest to the highest:
///
/// 1. Text before the three numbers: `chart-1.2.0`.
/// 2. One or two numbers and a tail of text: `1.2rc1` or `1.2.develop`. A missing number is `0`.
/// 3. A tail of text right after the three numbers: `1.2.0.develop`.
/// 4. A pre-release: `1.2.0-rc.1`.
/// 5. The version: `1.2.0`.
/// 6. A fourth number: `1.2.0.1`. Text right after it puts the string below it: `1.2.0.1rc1`. A
///    tail after `-` or after a dot puts the string above it: `1.2.0.1-rc1`.
///
/// A part between dots is a number when it holds only digits, as the crate reads it. In the last
/// part, the digits in front of letters are a number too, as in `2rc1` or `2rc`. So `1.2rc1` holds
/// two numbers, and `1.2a1b` holds one number and the tail `2a1b`.
///
/// The natural order of the text before the numbers orders two strings of the first form. When
/// that text is the same, the rest decides: `chart-1.2.0.develop` is below `chart-1.2.0-rc.1`. Of
/// two strings of the second form, the string with fewer numbers is lower. Then text right after
/// the last number is lower than a tail after a dot: `1rc1` is below `1.develop`.
///
/// Each other tail orders part by part, at its dots, as the crate orders it. A part of digits is
/// above a text, and two parts of digits compare as numbers. Digits with letters right after them,
/// as in `4rc1`, are below the digits alone. Two texts compare as bytes. Then a pre-release after
/// `-` is above no pre-release. Build data after `+` does not count.
///
/// The crate gives no total order to some forms: `1.2.3rc1`, `1.0.0-a_b`, `1.2rc1.x`, and `1.2rc`
/// beside `1.2rc1`. Strata keeps the rules above for them.
///
/// A string with no number at its start and no three numbers, such as `develop` or `1a1b`, holds
/// no version. Strata puts it below each version. A number that does not fit in a `u64` holds no
/// version, as Palette refuses it. The empty string is the lowest.
///
/// Two strings with no version keep the natural order. Runs of digits compare as numbers, and a
/// zero in front does not count. Runs of other characters compare as text. When one string is a
/// prefix of the other, the longer one is higher. This orders `LTS__24.x` above `LTS__22.04`. The
/// order of all strings is total.
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

/// True when `v` is a pre-release, such as `1.0.1-rc.1`, `1.2.develop`, or `chart-1.2.3`.
///
/// Five forms are a pre-release:
///
/// - A version that parses, with a pre-release after `-`: `1.0.1-rc.1` or `1.0.1-g0123456789ab`.
/// - One or two numbers and a tail of text: `1.2.develop`, `1.develop`, or `1.2rc1`.
/// - A tail of text right after the three numbers: `1.1.8.develop` or `1.2.3rc1`.
/// - Text before the three numbers: `chart-1.2.3`. One `v` in front is no such text.
/// - Text after a fourth number: `1.2.3.4-rc1`, `1.2.3.4rc1`, or `1.1.8.1.develop`.
///
/// Strata puts the first four forms below the version of their numbers, and the last form above
/// its three numbers: see [`compare`]. A tail of numbers, as in `10.4.0.1` and `v10.4.0.1`, is no
/// pre-release. Build data, as in `1.0.1+2`, is no pre-release. A string with no version, such as
/// `develop`, is no pre-release.
pub fn is_pre_release(v: &str) -> bool {
    Version::read(v).is_some_and(|s| s.is_pre_release())
}

/// The item of `items` whose version is `wanted`, or the item with the highest version by
/// [`compare`] when `wanted` is [`LATEST`].
///
/// `version_of` gives the version of an item. For [`LATEST`], a pre-release by
/// [`is_pre_release`] is the highest only when no item has a version that is not a pre-release.
/// So `1.10.0` is the latest of `1.2.develop`, `1.10.0`, and `1.9.0`. A string with no version is
/// the highest only when no item holds a version. None when no item has the version, and when
/// `items` is empty.
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

/// A version as strata reads it: a version that parses, or the numbers of a text.
struct Version<'a> {
    /// The text before the three numbers, as in `chart-1.2.3`. Empty for no such text, and for one
    /// `v` in front.
    before: &'a str,
    /// The three numbers. A number that the version omits is `0`.
    numbers: [u64; 3],
    /// How many numbers the version writes before its tail, from one to three.
    count: usize,
    /// The form of the text around the numbers.
    form: Form,
    /// True when the tail starts right after the last number, as in `1.2rc1`, and not after a
    /// dot. Only one or two numbers have such a tail.
    attached: bool,
    /// The identifiers of the pre-release after `-`. Empty for a version with no pre-release.
    pre: Vec<&'a str>,
    /// The text that orders two versions of one form. Empty for a version that parses.
    tail: &'a str,
}

/// The form of a version around its three numbers: see [`compare`].
///
/// The derived order is the order of the variants. For one set of three numbers, it goes from the
/// lowest version to the highest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Form {
    /// One or two numbers and a tail of text, as in `1.2.develop` or `1.2rc1`.
    Short,
    /// A tail of text right after the three numbers, as in `1.1.8.develop` or `1.2.3rc1`.
    Text,
    /// A version that parses.
    Parsed,
    /// A fourth number after the three numbers, as in `10.4.0.1` or `10.4.0.1rc1`, with an
    /// optional tail.
    More,
}

impl<'a> Version<'a> {
    /// The version that strata reads in `v`: the version that `v` parses as, the first three
    /// numbers of `v`, or the numbers at its start. None when `v` holds no version.
    fn read(v: &'a str) -> Option<Self> {
        Self::parse(v)
            .or_else(|| Self::three_numbers_of(v))
            .or_else(|| Self::short(v))
    }

    /// The first three numbers of `v` and the form around them, as Palette reads a version that
    /// does not parse. None when `v` has no three numbers, or a number does not fit in a `u64`.
    fn three_numbers_of(v: &'a str) -> Option<Self> {
        let found = three_numbers(v)?;
        // one `v` in front is no text before the numbers
        let before = &v[..found.start];
        if !matches!(before, "" | "v") {
            // Palette ignores the text before the numbers, and strata reads the rest
            return Self::read(&v[found.start..]).map(|rest| Version { before, ..rest });
        }
        let mut numbers = [0; 3];
        for (slot, n) in numbers.iter_mut().zip(v[found.clone()].split('.')) {
            *slot = n.parse().ok()?;
        }
        let version = |form, tail, pre| Version {
            before: "",
            numbers,
            count: 3,
            form,
            attached: false,
            pre,
            tail,
        };
        let (rest, pre) = split_pre(&v[found.end..]);
        let tail = rest.strip_prefix('.');
        // the crate reads a number with text right after it only in the last part
        let fourth = tail.is_some_and(|tail| match tail.split_once('.') {
            Some((first, _)) => is_digits(first),
            None => number_and_text(tail).is_some(),
        });
        let form = if fourth { Form::More } else { Form::Text };
        Some(version(form, tail.unwrap_or(rest), pre))
    }

    /// One or two numbers at the start of `v` and the tail after them, as in `1.2.develop`. None
    /// when `v` has no number at its start, or a number does not fit in a `u64`.
    ///
    /// The parts of digits at the start, between dots, are the numbers. The crate `versions` also
    /// reads the number of a last part with text right after it, as in `1.2rc1`.
    fn short(v: &'a str) -> Option<Self> {
        let (mut rest, pre) = split_pre(v.strip_prefix('v').unwrap_or(v));
        let mut numbers = [0; 3];
        let mut count = 0;
        loop {
            let (part, more) = rest.split_once('.').unwrap_or((rest, ""));
            if !is_digits(part) {
                break;
            }
            *numbers.get_mut(count)? = part.parse().ok()?;
            count += 1;
            rest = more;
        }
        let number = number_and_text(rest).filter(|_| !rest.contains('.'));
        if let Some((number, _, _)) = number {
            *numbers.get_mut(count)? = number.parse().ok()?;
            count += 1;
        }
        (count > 0).then_some(Version {
            before: "",
            numbers,
            count,
            form: Form::Short,
            attached: number.is_some(),
            pre,
            tail: rest,
        })
    }

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
        Some(Version {
            before: "",
            numbers,
            count,
            form: Form::Parsed,
            attached: false,
            pre,
            tail: "",
        })
    }

    /// True when the version is a pre-release: see [`is_pre_release`].
    fn is_pre_release(&self) -> bool {
        !self.before.is_empty()
            || match self.form {
                Form::Short | Form::Text => true,
                Form::Parsed => !self.pre.is_empty(),
                Form::More => !self.pre.is_empty() || !self.tail.split('.').all(is_digits),
            }
    }

    /// The precedence of Semantic Versioning 2.0.0, item 11, and then the order that strata adds.
    ///
    /// The numbers decide first. Text before the numbers is lower, and then its natural order
    /// decides. Then the [`Form`] decides. Of two versions that parse, a version is above its
    /// pre-releases. The identifiers of two pre-releases decide, as item 11.4 says.
    ///
    /// The crate `versions` orders two versions of another form. Of two versions with one or two
    /// numbers, fewer numbers are lower, and then a tail right after the last number is lower.
    /// Then the tails decide: see [`tails`]. Then a pre-release is above no pre-release, and the
    /// identifiers decide.
    fn precedence(&self, other: &Self) -> Ordering {
        let tails = || {
            tails(self.tail, other.tail)
                .then_with(|| other.pre.is_empty().cmp(&self.pre.is_empty()))
                .then_with(|| identifiers(&self.pre, &other.pre))
        };
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| self.before.is_empty().cmp(&other.before.is_empty()))
            .then_with(|| natural(self.before, other.before))
            .then_with(|| self.form.cmp(&other.form))
            .then_with(|| match self.form {
                Form::Short => self
                    .count
                    .cmp(&other.count)
                    .then_with(|| other.attached.cmp(&self.attached))
                    .then_with(tails),
                Form::Text | Form::More => tails(),
                Form::Parsed => self
                    .pre
                    .is_empty()
                    .cmp(&other.pre.is_empty())
                    .then_with(|| identifiers(&self.pre, &other.pre)),
            })
    }
}

/// The text of a tail and the identifiers of its pre-release, as the crate `versions` reads them.
/// Build data after `+` does not count, and a pre-release starts at the first `-`.
fn split_pre(v: &str) -> (&str, Vec<&str>) {
    let v = v.split_once('+').map_or(v, |(v, _)| v);
    match v.split_once('-') {
        Some((tail, pre)) => (tail, pre.split('.').collect()),
        None => (v, Vec::new()),
    }
}

/// The order of two tails, part by part at their dots, as the crate `versions` orders them.
///
/// The first pair of parts that differs decides: see [`part`]. When one tail is the start of the
/// other, the longer tail is higher.
fn tails(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.split('.'), b.split('.'));
    loop {
        let ord = match (a.next(), b.next()) {
            (Some(x), Some(y)) => part(x, y),
            (x, y) => return x.is_some().cmp(&y.is_some()),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
}

/// The order of two parts of a tail, as the crate `versions` orders them.
///
/// A number is above a text, two numbers compare as numbers, and two texts compare as bytes. A
/// number with text right after it, as in `4rc1`, is below the number alone, and above each lower
/// number. Two such parts compare by the number, then the text, and then the number after the
/// text. No number after the text is the lowest, so `4rc` is below `4rc0`.
fn part(a: &str, b: &str) -> Ordering {
    match (number_and_text(a), number_and_text(b)) {
        (Some((n, text, after)), Some((m, other_text, other_after))) => numeric(n, m)
            .then_with(|| text.is_empty().cmp(&other_text.is_empty()))
            .then_with(|| text.cmp(other_text))
            .then_with(|| (!after.is_empty()).cmp(&!other_after.is_empty()))
            .then_with(|| numeric(after, other_after)),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => a.cmp(b),
    }
}

/// A part of a tail as the crate `versions` reads the last part of a version: a number, the
/// letters right after it, and a number after the letters. The letters and the second number can
/// be empty. So `4` gives `4`, `4rc` gives `4` and `rc`, and `4rc1` gives `4`, `rc`, and `1`. None
/// for each other part, such as `develop` or `4a1b`.
fn number_and_text(part: &str) -> Option<(&str, &str, &str)> {
    let end = |s: &str, f: fn(&char) -> bool| s.chars().take_while(f).count();
    let (number, rest) = part.split_at(end(part, char::is_ascii_digit));
    let (text, after) = rest.split_at(end(rest, char::is_ascii_alphabetic));
    let read = !number.is_empty() && (after.is_empty() || (!text.is_empty() && is_digits(after)));
    read.then_some((number, text, after))
}

/// The order of two lists of identifiers, such as the identifiers of two pre-releases.
///
/// Compare the identifiers from the left, and the first pair that differs decides. Two numbers
/// compare as numbers, a number is below a text, and two texts compare as bytes. When one list is
/// the start of the other, the longer list is higher.
fn identifiers(a: &[&str], b: &[&str]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
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
    a.len().cmp(&b.len())
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
mod tests;
