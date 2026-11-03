//! The targets of a ref and of the version order.

use std::cmp::Ordering;

use strata::reference::{Ref, version};

use crate::answer::ref_fault;
use crate::support::{parts, utf8};

/// Parse `text` as a ref that names no file. Return true when the ref is not a plain uid, so the
/// parser found a shape in the text. The text of the ref must parse as the same ref: see
/// [`ref_fault`].
fn ref_of(text: &str) -> bool {
    let r = ref_fault(text).unwrap_or_else(|fault| panic!("{fault}"));
    !matches!(r, Ref::Uid(_))
}

/// Parse the text of `bytes` as a ref. Return true when the text is UTF-8 and has a shape.
pub(crate) fn parse(bytes: &[u8]) -> bool {
    utf8(bytes).is_some_and(ref_of)
}

#[test]
fn the_text_of_a_ref_parses_as_a_ref_that_prints_the_same_text() {
    bolero::check!().for_each(|bytes| {
        parse(bytes);
    });
}

/// Make sure that [`version::compare`] orders `a`, `b`, and `c` as a total preorder. Each version
/// equals itself. The order of two versions reverses with them. The order is transitive. Two
/// spellings of one version, such as `1.0.1` and `1.0.1+2`, compare equal.
pub(crate) fn order_laws(a: &str, b: &str, c: &str) {
    let cmp = version::compare;
    for x in [a, b, c] {
        assert_eq!(cmp(x, x), Ordering::Equal, "{x:?} equals itself");
    }
    for (x, y) in [(a, b), (b, c), (a, c)] {
        assert_eq!(
            cmp(x, y),
            cmp(y, x).reverse(),
            "{x:?} and {y:?} are not in opposite orders"
        );
    }
    for (x, y, z) in [
        (a, b, c),
        (a, c, b),
        (b, a, c),
        (b, c, a),
        (c, a, b),
        (c, b, a),
    ] {
        if cmp(x, y) != Ordering::Greater && cmp(y, z) != Ordering::Greater {
            assert_ne!(
                cmp(x, z),
                Ordering::Greater,
                "{x:?} <= {y:?} <= {z:?}, but {x:?} > {z:?}"
            );
        }
    }
}

/// Order the three versions that an input holds, split at NUL bytes. Return true when the input
/// is UTF-8.
pub(crate) fn order(bytes: &[u8]) -> bool {
    let Some(text) = utf8(bytes) else {
        return false;
    };
    let mut versions = parts(text.as_bytes()).map(|p| utf8(p).unwrap_or_default());
    let (a, b, c) = (
        versions.next().unwrap_or_default(),
        versions.next().unwrap_or_default(),
        versions.next().unwrap_or_default(),
    );
    order_laws(a, b, c);
    true
}

#[test]
fn the_version_order_is_a_total_preorder() {
    bolero::check!().for_each(|bytes| {
        order(bytes);
    });
}
