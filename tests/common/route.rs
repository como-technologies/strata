//! The match of a route of Palette with a pattern, such as `v1/clusterprofiles/{uid}`.
//!
//! xtask and the tests of strata-preflight include this file with `#[path]`. So each package
//! matches a route with a pattern in one way.

/// True when `route` matches `pattern`. The two have the same number of segments, and each segment
/// is equal, or is one group in braces in the pattern, such as `{uid}`. Such a group stands for
/// any one segment.
pub fn route_matches(pattern: &str, route: &str) -> bool {
    let (pattern, route): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), route.split('/').collect());
    pattern.len() == route.len()
        && pattern
            .iter()
            .zip(&route)
            .all(|(p, r)| is_group(p) || p == r)
}

/// True when `segment` of a pattern is one group in braces and nothing else, such as `{uid}`.
fn is_group(segment: &str) -> bool {
    segment
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|inner| !inner.contains(['{', '}']))
}
