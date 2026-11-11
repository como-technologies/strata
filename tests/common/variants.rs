//! The list of one value of each variant of an enum, for a test that walks the variants.
//!
//! The tests of `error/` in strata include this file with `#[path]`. So do the tests of the edits
//! of the stand-in in `xtask`, and the test of `Check` in `strata-preflight`. So the lists keep
//! one rule.

/// Make the `Vec` of the values of the list, in order, and tie the list to each variant of their
/// enum.
///
/// Write each value with the path of its variant, as `Edit::Reclaim("Retain")` or `Error::NoUid`.
/// A `match` names the variant of each value, and has no `_` arm. So the build fails on a new
/// variant until the list holds a value of it. A variant that the list holds two times gives an
/// unreachable arm, and `just lint` fails.
macro_rules! one_of_each {
    ($($set:ident::$variant:ident $(($($tuple:tt)*))? $({$($fields:tt)*})?),+ $(,)?) => {{
        let list = vec![$($set::$variant $(($($tuple)*))? $({$($fields)*})?),+];
        for value in &list {
            match value {
                $($set::$variant { .. } => {})+
            }
        }
        list
    }};
}
pub(crate) use one_of_each;
