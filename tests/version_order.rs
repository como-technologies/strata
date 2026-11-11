//! The order of versions of strata, against the order of the crate `versions` 8.0.1 (#714).
//!
//! brett chose the order of the crate where Palette reads no version, and the rules of Palette
//! where Palette reads one. Each row states the order of strata and who decides it. A row of the
//! crate fails when the crate or strata gives another order. A row of Palette fails when the crate
//! agrees with it, because then the row is no exception. So a change of `version.rs` cannot drift
//! from the crate without a failed test. The crate is a dev-dependency only.

use std::cmp::Ordering::{self, Equal, Greater, Less};

use strata::reference::LATEST;
use strata::reference::version::{compare, is_pre_release, pick};
use versions::Versioning;

/// A rule of Palette that strata keeps where the crate gives another order.
#[derive(Clone, Copy, Debug)]
enum PaletteRule {
    /// One `v` in front of the numbers does not count: `v1.2.3` equals `1.2.3`.
    VInFront,
    /// A missing number is `0`: `1.2` equals `1.2.0`.
    MissingNumber,
    /// Palette finds the three numbers at any place, so `chart-1.2.3` reads as `1.2.3`. The rest
    /// orders two strings with one text before the numbers. The crate reads `1.2.3` there as a
    /// pre-release of the text `chart`.
    TextBefore,
}

/// Who decides the order of a row.
#[derive(Clone, Copy, Debug)]
enum Source {
    /// The crate: it gives the order of the row.
    Crate,
    /// A rule of Palette: the crate gives another order.
    Palette(PaletteRule),
    /// The crate reads no version in one of the two strings. Strata puts a string with no
    /// version below each version, and orders two of them by the natural order.
    NoRead,
}

use PaletteRule::{MissingNumber, TextBefore, VInFront};
use Source::{Crate, NoRead, Palette};

/// The rows of the probe of the changed answer, in the comment on #714, in its order.
const PROBE: &[(&str, Ordering, &str, Source)] = &[
    ("1.2.develop", Less, "1.10.0", Crate),
    ("1.2.develop", Less, "1.9.0", Crate),
    ("1.2.develop", Less, "1.2.0", Crate),
    ("1.1.8.develop", Less, "1.1.8", Crate),
    ("1.1.8.develop", Less, "1.1.10", Crate),
    ("1.1.8.develop", Less, "1.1.8-rc.1", Crate),
    ("10.4.0.1", Greater, "10.4.0", Crate),
    ("10.4.0.1", Less, "10.4.1", Crate),
    ("1.2.3.4", Less, "1.2.3.10", Crate),
    ("1.0.0-rc10", Less, "1.0.0-rc9", Crate),
    ("1.0.0-rc.10", Greater, "1.0.0-rc.9", Crate),
    ("1.0.0-beta.11", Greater, "1.0.0-beta.2", Crate),
    ("1.0.0-alpha", Less, "1.0.0-alpha.1", Crate),
    ("1.0.0-alpha.beta", Less, "1.0.0-beta", Crate),
    ("4.10.3-rc.1", Less, "4.10.3", Crate),
    ("1.0.1-g0123456789ab", Less, "1.0.1", Crate),
    ("1.0.0+2", Equal, "1.0.0", Crate),
    ("v1.2.3", Equal, "1.2.3", Palette(VInFront)),
    ("v1.1.8.1", Equal, "1.1.8.1", Palette(VInFront)),
    ("1.2", Equal, "1.2.0", Palette(MissingNumber)),
    ("1.2", Less, "1.2.1", Crate),
    ("1.10", Greater, "1.9", Crate),
    ("chart-1.2.3", Less, "1.2.3", Crate),
    ("LTS__24.x", Greater, "LTS__22.04", NoRead),
    ("", Less, "1.0.0", NoRead),
    ("1.1.8.develop", Less, "1.1.8-alpha", Crate),
    ("1.2.develop", Less, "1.2.0-rc.1", Crate),
    ("1.2.develop", Greater, "1.1.10", Crate),
    ("1.2.develop", Less, "1.2.0.develop", Crate),
    ("1.develop", Less, "1.0.0", Crate),
    ("1.develop", Greater, "0.9.0", Crate),
    ("1.2.x", Less, "1.2.0", Crate),
    ("develop", Less, "1.0.0", Crate),
    ("develop", Less, "0.0.1", Crate),
];

/// More forms of each rule of the changed answer, and of each rule of Palette that stays.
const FORMS: &[(&str, Ordering, &str, Source)] = &[
    // one or two numbers and a tail of text: a pre-release of the version of the numbers
    ("1.2.develop", Less, "1.2.0-0", Crate),
    ("1.2.develop", Greater, "1.1.99.develop", Crate),
    ("1.2.develop", Greater, "1.develop", Crate),
    ("1.2.develop", Less, "2.develop", Crate),
    ("1.2.develop", Greater, "1.2.beta", Crate),
    ("1.2.develop", Less, "1.2.x", Crate),
    ("1.2.develop", Less, "1.2.0.1", Crate),
    ("1.2.develop", Greater, "develop", Crate),
    ("1.9.develop", Less, "1.10.develop", Crate),
    ("1.develop", Less, "1.0.develop", Crate),
    ("1.develop", Greater, "0.develop", Crate),
    ("1.develop", Less, "1.0.0-alpha", Crate),
    ("1.2.develop", Less, "1.2", Palette(MissingNumber)),
    ("1.develop", Less, "1", Palette(MissingNumber)),
    ("v1.2.develop", Equal, "1.2.develop", Palette(VInFront)),
    // a tail of text right after three numbers: below each pre-release of them
    ("1.1.8.develop", Less, "1.1.8-0", Crate),
    ("1.1.8.develop", Less, "1.1.8-alpha.1", Crate),
    ("1.1.8.develop", Greater, "1.1.7", Crate),
    ("1.1.8.develop", Less, "1.1.8+5", Crate),
    ("1.1.8.develop", Less, "1.1.8.0", Crate),
    ("1.1.8.develop", Greater, "1.1.8.beta", Crate),
    ("1.1.8.develop", Less, "1.1.8.develop.2", Crate),
    ("1.1.8.develop.2", Less, "1.1.8.develop.10", Crate),
    ("1.1.8.develop.beta", Less, "1.1.8.develop.2", Crate),
    ("1.1.8.develop", Less, "1.2.develop", Crate),
    ("1.2.3.develop", Less, "1.2.3-rc1", Crate),
    ("v1.1.8.develop", Equal, "1.1.8.develop", Palette(VInFront)),
    // no number at the start and no three numbers: below each version
    ("develop", Greater, "beta", Crate),
    ("develop", Greater, "alpha.1", Crate),
    ("develop", Less, "1.develop", Crate),
    ("LTS__24.x", Less, "1.0.0", NoRead),
    ("LTS__24.x", Less, "1.2.develop", NoRead),
    ("", Less, "develop", NoRead),
    // text before the three numbers: Palette reads the numbers
    ("chart-1.2.3", Greater, "1.2.2", Palette(TextBefore)),
    ("chart-1.2.3", Greater, "0.0.1", Palette(TextBefore)),
    ("chart-1.2.3", Greater, "develop", Palette(TextBefore)),
    ("chart-1.2.3", Less, "chart-1.2.4", Crate),
    ("chart-1.2.3", Less, "1.2.3-alpha", Crate),
    ("chart-1.2.3", Less, "1.2.3.develop", Crate),
    ("chart-1.2.0", Less, "1.2.develop", Crate),
    ("app-1.2.3", Less, "chart-1.2.3", Crate),
    // one `v` in front and a missing number, the rules of Palette
    ("v1.2.3", Greater, "1.2.2", Palette(VInFront)),
    ("v1.2.3", Greater, "1.2.3-rc.1", Palette(VInFront)),
    ("v1.2.3", Less, "1.2.4", Crate),
    ("v1.2.3", Less, "v1.2.4", Crate),
    ("v1.2", Equal, "1.2.0", Palette(VInFront)),
    ("1", Equal, "1.0.0", Palette(MissingNumber)),
    ("1", Greater, "0.9.0", Crate),
    ("1", Less, "1.0.1", Crate),
    ("1.2", Greater, "1.1.9", Crate),
    ("1.2", Greater, "1.2.0-rc.1", Palette(MissingNumber)),
    ("1.2-rc.1", Equal, "1.2.0-rc.1", Palette(MissingNumber)),
    ("1.2-rc.1", Less, "1.2.0", Crate),
    // Semantic Versioning 2.0.0 where a version parses
    ("1.0.0-0", Less, "1.0.0-alpha", Crate),
    ("1.0.0-alpha", Greater, "1.0.0-1", Crate),
    ("1.0.0-rev9", Greater, "1.0.0-rev10", Crate),
    ("1.0.1-g0123456789ac", Greater, "1.0.1-g0123456789ab", Crate),
    ("1.0.1+2", Equal, "1.0.1+3", Crate),
    ("1.0.1-rc.1+5", Less, "1.0.1", Crate),
    ("1.29.4-rc2", Greater, "1.29.4-rc1", Crate),
    ("1.21.1-rev1", Less, "1.21.1", Crate),
    ("22.04", Greater, "20.04", Crate),
    ("22.04", Equal, "22.4", Crate),
    ("22.04", Less, "22.04.1", Crate),
    // a tail of numbers is above its version
    ("10.4.0.1", Less, "10.4.0.2", Crate),
    ("10.4.0.1", Less, "10.4.0.1.1", Crate),
    ("10.4.0.1", Greater, "10.4.0-rc.1", Crate),
    ("10.4.0.1", Less, "10.4.1-rc.1", Crate),
    ("10.4.0.1", Greater, "10.4.0.develop", Crate),
    ("1.2.3.4", Greater, "1.2.3.develop", Crate),
    // a tail after a fourth number: the tie that the rules do not decide
    ("1.2.3.4-rc1", Greater, "1.2.3", Crate),
    ("1.2.3.4-rc1", Greater, "1.2.3-rc1", Crate),
    ("1.2.3.4-rc1", Greater, "1.2.3.4", Crate),
    ("1.2.3.4-rc1", Less, "1.2.3.4.develop", Crate),
    ("1.2.3.4-rc1", Less, "1.2.4", Crate),
    ("1.2.3.4.develop", Less, "1.2.3.4.1", Crate),
    ("1.1.8.1.develop", Greater, "1.1.8.1", Crate),
    ("1.1.8.1.develop", Greater, "1.1.8.develop", Crate),
    ("10.4.0.1-rc.1", Greater, "10.4.0.1", Crate),
    ("10.4.0.1-rc.1", Less, "10.4.0.2", Crate),
    // text right after a fourth number: the crate reads the number when the part is the last
    ("10.4.0.1rc1", Greater, "10.4.0", Crate),
    ("1.2.3.4rc1", Greater, "1.2.3-rc.1", Crate),
    ("1.2.3.4rc1", Less, "1.2.3.4", Crate),
    ("10.4.0.1rc1", Greater, "10.4.0.0", Crate),
    ("1.2.3.4rc1", Greater, "1.2.3.3", Crate),
    ("1.2.3.4rc1", Greater, "1.2.3.develop", Crate),
    ("1.2.3.4rc1", Less, "1.2.3.4-rc1", Crate),
    ("1.2.3.4rc1", Less, "1.2.3.4.develop", Crate),
    ("1.2.3.4rc1", Less, "1.2.3.4rc2", Crate),
    ("1.2.3.4rc10", Greater, "1.2.3.4rc9", Crate),
    ("1.2.3.4beta1", Less, "1.2.3.4rc1", Crate),
    ("1.2.3.4rc", Greater, "1.2.3.4beta", Crate),
    ("1.2.3.4rc1-1", Greater, "1.2.3.4rc1", Crate),
    ("1.2.3.4.5rc1", Greater, "1.2.3.4", Crate),
    ("1.2.3.4.5rc1", Less, "1.2.3.4.5", Crate),
    ("1.1.8.develop.4rc1", Greater, "1.1.8.develop.3", Crate),
    ("1.1.8.develop.4rc1", Less, "1.1.8.develop.4", Crate),
    // a part of digits and other text is a text: `1.2.3.4a1b` is a tail of text
    ("1.2.3.4a1b", Less, "1.2.3-rc.1", Crate),
    ("1.2.3.4a1b", Less, "1.2.3.develop", Crate),
    // one or two numbers with text right after the last number: below a tail after a dot
    ("1.2rc1", Less, "1.2.develop", Crate),
    ("1.2rc1", Less, "1.2.rc1", Crate),
    ("1rc1", Less, "1.develop", Crate),
    ("1.2rc1", Greater, "1.1.10", Crate),
    ("1.2rc1", Greater, "1.1.99.develop", Crate),
    ("1.2rc1", Less, "1.2.0-rc.1", Crate),
    ("1.2rc1", Greater, "1.develop", Crate),
    ("1.2rc1", Less, "1.2rc2", Crate),
    ("1.2rc10", Greater, "1.2rc9", Crate),
    ("1.2beta1", Less, "1.2rc1", Crate),
    ("1.2rc1-1", Greater, "1.2rc1", Crate),
    ("1.2rc1+5", Equal, "1.2rc1", Crate),
    ("1rc1", Greater, "0.9.0", Crate),
    ("1rc1", Less, "1.0rc1", Crate),
    ("1.0rc1", Less, "1.0.develop", Crate),
    // `1.2a1b` holds one number and the tail `2a1b`, and `1a1b` holds no number
    ("1.2a1b", Less, "1.0.0", Crate),
    ("1.2a1b", Greater, "0.9.0", Crate),
    ("1.2a1b", Less, "1.develop", Crate),
    ("1a1b", Less, "0.0.1", Crate),
    // a tail drops its build data, and its pre-release starts at the first `-`
    ("1.2.develop+5", Equal, "1.2.develop", Crate),
    ("1.1.8.develop+5", Equal, "1.1.8.develop", Crate),
    ("1.2.develop-rc1", Greater, "1.2.develop", Crate),
    ("1.2.develop-rc1", Less, "1.2.develop.1", Crate),
    ("1.1.8.develop-rc1", Greater, "1.1.8.develop", Crate),
    ("1.1.8.develop-rc1", Less, "1.1.8.develop.1", Crate),
    // one text before the numbers: Palette ignores it, and strata orders the rest
    ("chart-1.2.3.develop", Less, "chart-1.2.3-alpha", Crate),
    ("chart-1.2.3+5", Equal, "chart-1.2.3", Crate),
    ("chart-1.2.3-alpha", Less, "chart-1.2.3-rc.1", Crate),
    (
        "chart-1.2.3",
        Greater,
        "chart-1.2.3-alpha",
        Palette(TextBefore),
    ),
    (
        "chart-1.2.3",
        Greater,
        "chart-1.2.3.develop",
        Palette(TextBefore),
    ),
    (
        "chart-1.2.3.1",
        Greater,
        "chart-1.2.3.develop",
        Palette(TextBefore),
    ),
];

/// The `@latest` rows of the probe, and the version that each gives. `@latest` skips a
/// pre-release while a version that is no pre-release exists. The crate has no such rule.
const LATEST_ROWS: &[(&[&str], &str)] = &[
    (&["1.2.develop", "1.10.0", "1.9.0"], "1.10.0"),
    (
        &["1.1.8", "1.1.8.develop", "1.1.10", "1.2.develop"],
        "1.1.10",
    ),
    (&["1.0.0-rc9", "1.0.0-rc10"], "1.0.0-rc9"),
    (&["10.4.0", "10.4.0.1", "10.4.1-rc.1"], "10.4.0.1"),
    (&["1.2.3.4", "1.2.3.4-rc1"], "1.2.3.4"),
    (&["10.4.0", "10.4.0.1rc1"], "10.4.0"),
    (&["1.1.10", "1.2rc1"], "1.1.10"),
];

/// The order of the crate, or None when the crate reads no version in `a` or `b`.
fn crate_order(a: &str, b: &str) -> Option<Ordering> {
    Some(Versioning::new(a)?.cmp(&Versioning::new(b)?))
}

/// The faults of one row: strata gives another order, or the source of the row does not hold.
fn row_faults(row: (&str, Ordering, &str, Source)) -> Vec<String> {
    let (a, want, b, source) = row;
    let mut faults = Vec::new();
    for (x, y, want) in [(a, b, want), (b, a, want.reverse())] {
        let strata = compare(x, y);
        if strata != want {
            faults.push(format!(
                "strata: {x:?} is {strata:?} to {y:?}, the row says {want:?}"
            ));
        }
        let ours = crate_order(x, y);
        let (holds, says) = match source {
            Crate => (ours == Some(want), "the crate gives this order".to_string()),
            Palette(rule) => (
                ours.is_some_and(|o| o != want),
                format!("the crate gives another order, and {rule:?} of Palette decides"),
            ),
            NoRead => (ours.is_none(), "the crate reads no version".to_string()),
        };
        if !holds {
            faults.push(format!(
                "the crate: {x:?} is {ours:?} to {y:?}, but the row says {want:?}: {says}"
            ));
        }
    }
    faults
}

#[test]
fn the_order_of_strata_is_the_order_of_the_crate_versions_except_where_palette_reads_a_version() {
    let faults: Vec<String> = PROBE
        .iter()
        .chain(FORMS)
        .flat_map(|&row| row_faults(row))
        .collect();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn latest_gives_the_highest_version_of_the_crate_and_skips_only_a_pre_release() {
    for &(versions, want) in LATEST_ROWS {
        assert_eq!(
            pick(versions.iter().copied(), |v| v, LATEST),
            Some(want),
            "{versions:?}"
        );
        let highest = versions
            .iter()
            .copied()
            .max_by(|a, b| {
                Versioning::new(a)
                    .unwrap()
                    .cmp(&Versioning::new(b).unwrap())
            })
            .unwrap();
        let ours = versions.iter().copied().max_by(|a, b| compare(a, b));
        assert_eq!(ours, Some(highest), "{versions:?}");
        // where `@latest` gives another version than the crate, the highest is a pre-release
        if want != highest {
            assert!(is_pre_release(highest), "{versions:?}: {highest}");
            assert!(!is_pre_release(want), "{versions:?}: {want}");
        }
    }
}

/// The crate gives no total order for `1.2.3rc1` and `1.0.0-a_b`, so no order can follow it there.
/// Strata keeps the rule of a tail of text: it is below each pre-release of its three numbers.
#[test]
fn the_crate_has_a_cycle_for_text_right_after_three_numbers_so_strata_keeps_its_rule() {
    // `1.2.3rc1` equals `1.2.3` and `1.2.3-rc.1` for the crate, and `1.2.3-rc.1` is below `1.2.3`
    assert_eq!(crate_order("1.2.3rc1", "1.2.3"), Some(Equal));
    assert_eq!(crate_order("1.2.3rc1", "1.2.3-rc.1"), Some(Equal));
    assert_eq!(crate_order("1.2.3-rc.1", "1.2.3"), Some(Less));
    // `1.0.0-a_b` is above `1.0.0` and below `1.0.0-alpha` for the crate, but `1.0.0-alpha` is
    // below `1.0.0`
    assert_eq!(crate_order("1.0.0-a_b", "1.0.0"), Some(Greater));
    assert_eq!(crate_order("1.0.0-a_b", "1.0.0-alpha"), Some(Less));
    assert_eq!(crate_order("1.0.0-alpha", "1.0.0"), Some(Less));
    for (low, high) in [
        ("1.2.2", "1.2.3rc1"),
        ("1.2.3rc1", "1.2.3-0"),
        ("0.9.9", "1.0.0-a_b"),
        ("1.0.0-a_b", "1.0.0-0"),
    ] {
        assert_eq!(compare(low, high), Less, "{low} < {high}");
        assert_eq!(compare(high, low), Greater, "{high} > {low}");
    }
}

/// The crate reads a number with text right after it, as in `4rc1`, only in the last part. It
/// compares `r8` and `r23` as numbers in another part, and as text in the last part. So it gives
/// no total order for such a part when it is not the last. Strata reads the part as a number with
/// text, or as a text, in each place.
#[test]
fn the_crate_has_a_cycle_for_digits_and_text_in_a_part_that_is_not_the_last_so_strata_keeps_one_rule()
 {
    // `1.2.3` < `1.2.3.3` < `1.2.3.4rc1.x` < `1.2.3` for the crate
    assert_eq!(crate_order("1.2.3", "1.2.3.3"), Some(Less));
    assert_eq!(crate_order("1.2.3.3", "1.2.3.4rc1.x"), Some(Less));
    assert_eq!(crate_order("1.2.3.4rc1.x", "1.2.3"), Some(Less));
    // `1.1.11.x` < `1.2rc1.x` < `1.1.10` < `1.1.11.x` for the crate
    assert_eq!(crate_order("1.1.11.x", "1.2rc1.x"), Some(Less));
    assert_eq!(crate_order("1.2rc1.x", "1.1.10"), Some(Less));
    assert_eq!(crate_order("1.1.10", "1.1.11.x"), Some(Less));
    // `1.r8.x` < `1.r23.x` < `1.r8` < `1.r8.x` for the crate
    assert_eq!(crate_order("1.r8.x", "1.r23.x"), Some(Less));
    assert_eq!(crate_order("1.r23.x", "1.r8"), Some(Less));
    assert_eq!(crate_order("1.r8", "1.r8.x"), Some(Less));
    for (low, high) in [
        ("1.2.3.develop", "1.2.3.4rc1.x"),
        ("1.2.3.4rc1.x", "1.2.3-0"),
        ("1.develop", "1.2rc1.x"),
        ("1.2rc1.x", "1.0.0-0"),
        ("1.r23.x", "1.r8"),
        ("1.r8", "1.r8.x"),
    ] {
        assert_eq!(compare(low, high), Less, "{low} < {high}");
        assert_eq!(compare(high, low), Greater, "{high} > {low}");
    }
}

/// The crate puts `1.2rc` equal to `1.2rc1` and to `1.2rc2`, but `1.2rc1` below `1.2rc2`. So no
/// order can follow it there. Strata puts letters with no number after them below the same
/// letters with a number after them.
#[test]
fn the_crate_has_no_order_for_letters_with_no_number_after_them_so_strata_puts_them_lower() {
    for (none, one, two) in [
        ("1.2rc", "1.2rc1", "1.2rc2"),
        ("1.2.3.4rc", "1.2.3.4rc1", "1.2.3.4rc2"),
    ] {
        assert_eq!(crate_order(none, one), Some(Equal));
        assert_eq!(crate_order(none, two), Some(Equal));
        assert_eq!(crate_order(one, two), Some(Less));
        for (low, high) in [(none, one), (one, two)] {
            assert_eq!(compare(low, high), Less, "{low} < {high}");
            assert_eq!(compare(high, low), Greater, "{high} > {low}");
        }
    }
    assert_eq!(compare("1.2rc", "1.2rc0"), Less);
}
