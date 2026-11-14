//! The tests of `version`: the order of Semantic Versioning, the forms of a text that does not
//! parse, the pre-releases, and the pick of `latest`.

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

/// The test of a chain: each version is below the next one.
fn ascending(chain: &[&str]) {
    for pair in chain.windows(2) {
        gt(pair[1], pair[0]);
    }
}

/// Issue #714. Palette reads no version in `1.2.develop`. #616 put it above each version, and
/// the first answer of brett below each version. The changed answer follows the crate
/// `versions`: it is a pre-release of `1.2.0`.
#[test]
fn one_or_two_numbers_and_a_tail_of_text_are_a_pre_release_below_each_pre_release_of_them() {
    ascending(&["1.1.10", "1.2.develop", "1.2.0-0", "1.2.0-rc.1", "1.2.0"]);
    ascending(&["0.9.0", "1.develop", "1.0.0-alpha", "1.0.0"]);
    // the numbers decide first, and a missing number is `0`
    ascending(&["1.1.99.develop", "1.2.develop", "1.10.develop", "2.develop"]);
    // fewer numbers are lower, and then the tail decides as the crate orders it
    ascending(&["1.develop", "1.0.develop", "1.0.0.develop"]);
    ascending(&["1.2.beta", "1.2.develop", "1.2.x", "1.2.0.develop"]);
    ascending(&["1.2.develop", "1.2.develop.2", "1.2.0.1"]);
    // text right after the last number is below a tail after a dot, as the crate orders it
    ascending(&["1rc1", "1.develop", "1.0rc1", "1.0.develop"]);
    ascending(&[
        "1.1.10",
        "1.2beta1",
        "1.2rc1",
        "1.2rc2",
        "1.2rc10",
        "1.2.beta",
        "1.2.develop",
    ]);
    ascending(&["1.2rc1", "1.2rc1-1", "1.2.rc1"]);
    eq("1.2rc1+5", "1.2rc1");
    // a part of digits and other text is a text, so `1.2a1b` holds one number, and `1a1b` none
    ascending(&["0.9.0", "1.2a1b", "1.develop", "1.0.0"]);
    ascending(&["1a1b", "develop", "0.0.1"]);
    // the rules of Palette stay: a missing number is `0`, and a `v` in front does not count
    gt("1.2", "1.2.develop");
    gt("1", "1.develop");
    eq("v1.2.develop", "1.2.develop");
}

/// Issue #714. Strata put a string with no number at its start below each version. The
/// changed answer keeps it, as the crate `versions` does.
#[test]
fn a_string_with_no_number_at_its_start_is_below_each_version_and_keeps_the_natural_order() {
    ascending(&["", "LTS__22.04", "LTS__24.x", "beta", "develop", "0.0.1"]);
    gt("1.develop", "develop");
    gt("0.0.1-rc.1", "develop");
    gt("chart-0.0.1", "develop");
    // a number that does not fit is no version, as Palette refuses it
    gt("2.0.0", "99999999999999999999999.0.0");
    gt("22.04", "LTS__24.x");
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

/// Issue #616 put `1.1.8.develop` below `1.1.8`, and above `1.1.8-rc.1`. The changed answer of
/// #714 puts it below each pre-release of `1.1.8`, as the crate `versions` does.
#[test]
fn a_tail_of_text_after_three_numbers_is_below_each_pre_release_of_them() {
    ascending(&[
        "1.1.7",
        "1.1.8.develop",
        "1.1.8-0",
        "1.1.8-alpha",
        "1.1.8-rc.1",
        "1.1.8",
    ]);
    gt("1.1.10", "1.1.8.develop");
    gt("1.2.0.develop", "1.1.10");
    // two tails of text order part by part, and a part of digits is above a text
    ascending(&[
        "1.1.8.beta",
        "1.1.8.develop",
        "1.1.8.develop.beta",
        "1.1.8.develop.2",
        "1.1.8.develop.10",
    ]);
    eq("1.1.8.develop", "1.1.8.develop");
    // a tail drops its build data, and its pre-release starts at the first `-`
    eq("1.1.8.develop+5", "1.1.8.develop");
    ascending(&["1.1.8.develop", "1.1.8.develop-rc1", "1.1.8.develop.1"]);
    // a part of digits and other text is a text
    ascending(&["1.2.2", "1.2.3.4a1b", "1.2.3.develop", "1.2.3-0"]);
    // text right after the numbers is a tail of text too. The crate gives these no total order.
    ascending(&["1.2.2", "1.2.3rc1", "1.2.3-0", "1.2.3"]);
    ascending(&["0.9.9", "1.0.0-a_b", "1.0.0-0", "1.0.0"]);
    // Issue #714, call 6: text right after the third number is below a tail after a dot, as the
    // crate orders it. Strata compared the two tails as bytes.
    ascending(&[
        "1.2.2",
        "1.2.3a1b",
        "1.2.3rc1b",
        "1.2.3x1y",
        "1.2.3.4a1b",
        "1.2.3.develop",
        "1.2.3.rc1b",
        "1.2.3.x",
        "1.2.3-0",
    ]);
    ascending(&["1.2.3rc1b", "1.2.3rc1b-1", "1.2.3.rc1b"]);
    eq("1.2.3rc1b+5", "1.2.3rc1b");
    eq("v1.2.3rc1b", "1.2.3rc1b");
    // the crate reads `3rc1` as a number with text, above the text `3x1y` and below `1.2.3.develop`
    ascending(&[
        "1.2.3x1y",
        "1.2.3rc1",
        "1.2.3rc9",
        "1.2.3rc10",
        "1.2.3.develop",
    ]);
    // Issue #714, call 9: the crate reads `1.2.0rc1b` as `1.2` and the tail `0rc1b`. Strata put
    // it above `1.2.develop`.
    ascending(&[
        "1.1.10",
        "1.2rc1",
        "1.2.0a1b",
        "1.2.0rc1b",
        "1.2.0x1y",
        "1.2.develop",
        "1.2.rc1b",
        "1.2.x",
        "1.2.0.develop",
        "1.2.0-0",
    ]);
    ascending(&["1.0rc1", "1.0.0rc1b", "1.0.develop", "1.0.0.develop"]);
}

/// Text before the three numbers, as in `chart-1.2.3`: Palette finds the numbers at any place,
/// so strata orders the string by them. It is below each other form with these numbers. Issue
/// #714: the natural order of the text before the numbers decides, and then the rest. Strata
/// ordered the whole text by the natural order.
#[test]
fn text_before_three_numbers_reads_as_the_numbers_as_in_palette() {
    ascending(&[
        "chart-1.2.3.develop",
        "chart-1.2.3-alpha",
        "chart-1.2.3-rc.1",
        "chart-1.2.3",
        "chart-1.2.3.1",
    ]);
    eq("chart-1.2.3+5", "chart-1.2.3");
    ascending(&["app-1.2.3.1", "chart-1.2.3.develop"]);
    ascending(&[
        "1.2.2",
        "app-1.2.3",
        "chart-1.2.3",
        "1.2.3.develop",
        "1.2.3-alpha",
        "1.2.3",
    ]);
    gt("1.2.develop", "chart-1.2.0");
    gt("chart-1.2.4", "chart-1.2.3");
    gt("x2.0.0", "chart-1.9.9");
}

/// Issue #616 put `10.4.0.1` above `10.4.0`. Issue #714: a tail after a fourth number orders
/// as the crate `versions` orders it, also a pre-release, which is above its four numbers.
#[test]
fn a_fourth_number_is_above_three_and_a_tail_after_it_orders_as_the_crate_orders_it() {
    ascending(&[
        "10.4.0-rc.1",
        "10.4.0",
        "10.4.0.1",
        "10.4.0.2",
        "10.4.1-rc.1",
        "10.5.0",
    ]);
    gt("v10.4.0.1", "10.4.0");
    ascending(&["1.2.3.4", "1.2.3.5", "1.2.3.10", "1.2.4"]);
    ascending(&[
        "1.2.3",
        "1.2.3.4",
        "1.2.3.4-rc1",
        "1.2.3.4-rc2",
        "1.2.3.4.develop",
        "1.2.3.4.1",
    ]);
    ascending(&[
        "10.4.0.1",
        "10.4.0.1-rc.1",
        "10.4.0.1.develop",
        "10.4.0.1.1",
    ]);
    gt("1.1.8.1.develop", "1.1.8.develop");
    // text right after a fourth number in the last part puts the string below the fourth number
    ascending(&[
        "1.2.3",
        "1.2.3.3",
        "1.2.3.4beta1",
        "1.2.3.4rc",
        "1.2.3.4rc1",
        "1.2.3.4rc2",
        "1.2.3.4rc10",
        "1.2.3.4",
        "1.2.3.4-rc1",
        "1.2.3.4.develop",
    ]);
    ascending(&["10.4.0", "10.4.0.0", "10.4.0.1rc1", "10.4.0.1"]);
    ascending(&["1.2.3.4", "1.2.3.4.5rc1", "1.2.3.4.5"]);
    // build data has no precedence
    eq("10.4.0.1+5", "10.4.0.1");
    // a fourth number is above a tail of text on the same three numbers
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
        "v1.2.develop",
        "1.develop",
        "1.0.develop",
        "1.2",
        "1.2.0-rc.1",
        "1.2.0.develop",
        "chart-1.2.0",
        "1.2.3rc1",
        "1.2.3rc1b",
        "1.2.3rc1b-1",
        "1.2.3.rc1b",
        "1.2.3x1y",
        "1.2.3.4-rc1",
        "1.2.3.4.develop",
        "1.2.3.4.1",
        "1.2.3.4rc",
        "1.2.3.4rc1",
        "1.2.3.4rc1.x",
        "10.4.0.1rc1",
        "1.2rc",
        "1.2rc1",
        "1.2rc1.x",
        "1rc1",
        "1.2a1b",
        "1a1b",
        "1.2.develop-rc1",
        "1.2.develop+5",
        "chart-1.2.3-alpha",
        "chart-1.2.3.develop",
        "app-1.2.3",
        "develop",
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
fn a_move_is_more_than_one_minor_version_above_as_palette_reads_the_two_versions() {
    for (from, to, above) in [
        ("1.34.9", "1.36.2", true),
        ("1.31.14", "1.37.0", true),
        ("v1.34.9", "1.36.0-rc.1", true),
        ("1.36.2", "2.0.0", true),
        // Palette reads the first three numbers of a text that does not parse
        ("chart-1.34.9", "1.36.2.1", true),
        ("1.34", "1.36", true),
        ("1.34.9", "1.35.6", false),
        ("1.34.9", "1.34.10", false),
        ("1.36.2", "1.34.9", false),
        ("2.0.0", "1.36.2", false),
        // Palette reads no version in these, so it makes no check
        ("1.34.develop", "1.36.2", false),
        ("1.34.9", "LTS__1.x", false),
        ("", "1.36.2", false),
    ] {
        assert_eq!(minor_above(from, to, 1), above, "{from} -> {to}");
    }
    assert!(!minor_above("1.34.9", "1.36.2", 2));
    assert!(minor_above("1.34.9", "1.35.0", 0));
}

#[test]
fn the_line_above_a_version_takes_its_major_number_and_a_higher_minor_number() {
    assert_eq!(minor_line_above("1.34.9", 1).as_deref(), Some("1.35"));
    assert_eq!(minor_line_above("v1.31", 1).as_deref(), Some("1.32"));
    assert_eq!(minor_line_above("chart-1.31.2", 2).as_deref(), Some("1.33"));
    assert_eq!(minor_line_above("1.34.develop", 1), None);
    assert_eq!(minor_line_above("LTS__24.x", 1), None);
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
    // a string with no number at its start holds no version, so it is no pre-release
    assert!(!is_pre_release("LTS__24.x"));
    assert!(!is_pre_release("develop"));
    assert!(!is_pre_release(""));
    // one or two numbers and a tail of text are a pre-release (#714)
    assert!(is_pre_release("1.2.develop"));
    assert!(is_pre_release("1.develop"));
    assert!(is_pre_release("v1.2.x"));
    assert!(is_pre_release("1.2rc1"));
    assert!(is_pre_release("1rc1"));
    assert!(is_pre_release("1.2a1b"));
    // a part of digits and other text is no number, so `1a1b` holds no version
    assert!(!is_pre_release("1a1b"));
    // a tail of text after the three numbers is a pre-release, and a tail of numbers is not
    assert!(is_pre_release("1.1.8.develop"));
    assert!(is_pre_release("v1.1.8.develop"));
    assert!(is_pre_release("1.2.3rc1"));
    assert!(!is_pre_release("10.4.0.1"));
    assert!(!is_pre_release("v10.4.0.1"));
    assert!(!is_pre_release("10.4.0.1.2+5"));
    // text after a fourth number is a pre-release, also above its four numbers
    assert!(is_pre_release("1.2.3.4-rc1"));
    assert!(is_pre_release("1.1.8.1.develop"));
    assert!(is_pre_release("10.4.0.1rc1"));
    // text before the three numbers is a pre-release, also with a tail of numbers
    assert!(is_pre_release("chart-1.2.3"));
    assert!(is_pre_release("chart-1.2.3.4"));
    assert!(is_pre_release("chart-1.2.3+5"));
    assert!(is_pre_release("vv1.2.3"));
}

#[test]
fn two_versions_keep_their_numbers_when_a_missing_number_is_zero_and_only_the_pre_release_or_the_build_data_differs()
 {
    for (a, b) in [
        ("1.66.0-g0123456789ab", "1.66.0"),
        ("1.66.0-rc.1", "1.66.0-rc.2"),
        ("1.2.0-mr12.345", "1.2.0"),
        ("1.2.0+build.5", "1.2.0"),
        ("v1.2.0", "1.2.0"),
        ("1.2", "1.2.0"),
        ("1.2-rc.1", "1.2.0"),
        ("1", "1.0.0+build.5"),
    ] {
        assert!(keeps_numbers(a, b), "{a} and {b}");
    }
    for (a, b) in [
        ("20.04", "24.04"),
        ("1.2.0-build5", "2026.11.0"),
        ("1.2", "1.2.1"),
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

/// Issue #714: `@latest` took `1.2.develop` over `1.10.0`. It is a pre-release of `1.2.0` now.
#[test]
fn latest_skips_a_string_with_no_three_numbers_while_a_version_exists() {
    let short = ["1.2.develop", "1.10.0", "1.9.0"];
    assert_eq!(pick(short, |v| *v, LATEST), Some("1.10.0"));
    let barney = ["1.1.8", "1.1.8.develop", "1.1.10", "1.2.develop"];
    assert_eq!(pick(barney, |v| *v, LATEST), Some("1.1.10"));
    // with no version that is no pre-release, the highest pre-release is the answer
    assert_eq!(
        pick(["1.2.develop", "1.0.0-rc.1"], |v| *v, LATEST),
        Some("1.2.develop")
    );
    assert_eq!(
        pick(["1.2.develop", "1.1.8.develop"], |v| *v, LATEST),
        Some("1.2.develop")
    );
    assert_eq!(
        pick(["1.2.develop", "1.10.develop"], |v| *v, LATEST),
        Some("1.10.develop")
    );
    // a string with no version is the answer only when no item holds a version
    assert_eq!(
        pick(["develop", "1.2.develop"], |v| *v, LATEST),
        Some("1.2.develop")
    );
    assert_eq!(pick(["develop", "beta"], |v| *v, LATEST), Some("develop"));
    assert_eq!(pick(short, |v| *v, "1.2.develop"), Some("1.2.develop"));
}

/// Issue #714. The crate `versions` puts `1.2.3.4-rc1` above `1.2.3.4`, and strata follows it.
/// It is still a pre-release, so `@latest` skips it.
#[test]
fn latest_skips_a_pre_release_after_a_fourth_number_also_above_its_four_numbers() {
    let four = ["1.2.3.4", "1.2.3.4-rc1"];
    assert_eq!(compare(four[1], four[0]), Ordering::Greater);
    assert_eq!(pick(four, |v| *v, LATEST), Some("1.2.3.4"));
    assert_eq!(
        pick(["1.2.3", "1.1.8.1.develop"], |v| *v, LATEST),
        Some("1.2.3")
    );
    // text right after a fourth number is a pre-release too, above its three numbers
    let rc = ["10.4.0", "10.4.0.1rc1"];
    assert_eq!(compare(rc[1], rc[0]), Ordering::Greater);
    assert_eq!(pick(rc, |v| *v, LATEST), Some("10.4.0"));
    assert_eq!(pick(["1.1.10", "1.2rc1"], |v| *v, LATEST), Some("1.1.10"));
}

#[test]
fn text_runs_compare_as_text() {
    gt("b", "a");
    gt("1.2b", "1.2a");
}

/// Issue #625. `@latest` names the highest pre-release above the version that it takes, so the
/// skip is no longer silent. A pre-release below that version, such as `2.2.8-hotfix` below
/// `2.2.8`, gives none.
#[test]
fn skipped_above_gives_the_highest_pre_release_above_the_pick_of_latest() {
    let cases: [(&[&str], Option<&str>); 8] = [
        (&["1.0.0", "1.0.1-mr2.770685"], Some("1.0.1-mr2.770685")),
        (
            &["1.0.0", "1.0.1-rc.1", "1.0.2-rc.1", "0.9.0-rc.1"],
            Some("1.0.2-rc.1"),
        ),
        (&["2.2.7", "2.2.8-hotfix"], Some("2.2.8-hotfix")),
        (&["2.2.8", "2.2.8-hotfix"], None),
        (
            &["1.1.8", "1.1.8.develop", "1.1.10", "1.2.develop"],
            Some("1.2.develop"),
        ),
        (&["1.2.3.4", "1.2.3.4-rc1"], Some("1.2.3.4-rc1")),
        // a pick of a pre-release skips none, and a text with no version is below each version
        (&["1.0.2-rc.2", "1.0.2-rc.1"], None),
        (&["develop", "1.0.0"], None),
    ];
    for (versions, skipped) in cases {
        let picked = pick(versions.iter().copied(), |v| *v, LATEST).unwrap();
        assert_eq!(
            skipped_above(versions.iter().copied(), picked),
            skipped,
            "{versions:?}"
        );
    }
}
