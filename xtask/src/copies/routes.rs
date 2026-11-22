//! The rule of the routes: a route of an added line that another line of the same tree holds.

use std::collections::{BTreeMap, BTreeSet};

use super::hits::{Added, Reported, touches};
use super::source::{Source, Tree};
use super::{Hit, Lang, Place, shown};

/// The start of a route of the API of Palette and of the observer.
const ROUTE_START: &str = "/v1/";

/// The methods that can come before a route, as in `GET /v1/...`.
const METHODS: [&str; 5] = ["GET ", "POST ", "PUT ", "PATCH ", "DELETE "];

/// The marks of Go formats that a route can hold in place of a segment.
const FORMATS: [&str; 4] = ["%s", "%d", "%v", "%q"];

/// The route that a literal holds, with `{}` in place of each segment that a format fills: the
/// route of `GET /v1/packs/{uid}` is `/v1/packs/{}`.
pub fn route(value: &str) -> Option<String> {
    let path = METHODS
        .iter()
        .find_map(|method| value.strip_prefix(method))
        .unwrap_or(value);
    if !path.starts_with(ROUTE_START) {
        return None;
    }
    let mut out = String::new();
    let mut open = false;
    for c in path.chars() {
        match c {
            '{' => open = true,
            '}' if open => {
                open = false;
                out.push_str("{}");
            }
            _ if !open => out.push(c),
            _ => {}
        }
    }
    Some(
        FORMATS
            .iter()
            .fold(out, |route, format| route.replace(format, "{}")),
    )
}

/// The tree of a route and the route: its language, the root of its tree, and its text.
type RouteKey<'a> = (Lang, &'a str, String);

/// A line that holds a route: its path, its line, and whether it is a route string.
type RoutePlace<'a> = (&'a str, usize, bool);

/// True when the literal at `line` of `file` is a route string of its tree. A route string is in
/// the code, or in the test code that the tests of other files share.
fn route_string(file: &Source, line: usize) -> bool {
    file.shared || !file.tests.holds(line)
}

/// The routes of the added lines that another line of the same tree holds. A route of a test is
/// a copy of a route string of its tree. A route string is a copy of a route in any line. A const
/// that holds a route is out: the rule of the literals reads it, with its reach.
pub fn routes(tree: &Tree, added: &Added, reported: &Reported) -> Vec<Hit> {
    let declared: BTreeSet<(&str, usize)> = tree
        .holders
        .iter()
        .map(|h| (h.path.as_str(), h.line))
        .collect();
    let mut lines: BTreeMap<RouteKey, Vec<RoutePlace>> = BTreeMap::new();
    for file in &tree.files {
        for literal in file.literals.iter().filter(|l| l.class.is_finding()) {
            if declared.contains(&(file.path.as_str(), literal.line)) {
                continue;
            }
            if let Some(route) = route(&literal.value) {
                let place = (
                    file.path.as_str(),
                    literal.line,
                    route_string(file, literal.line),
                );
                lines
                    .entry((file.lang, &file.root, route))
                    .or_default()
                    .push(place);
            }
        }
    }
    let mut out = Vec::new();
    for file in &tree.files {
        let added = added(&file.path);
        for literal in &file.literals {
            let fresh = touches(added, literal.line, literal.last)
                && literal.class.is_finding()
                && !declared.contains(&(file.path.as_str(), literal.line))
                && !reported.contains(&(file.path.clone(), literal.line));
            let Some(route) = route(&literal.value).filter(|_| fresh) else {
                continue;
            };
            let own = route_string(file, literal.line);
            let Some(places) = lines.get(&(file.lang, file.root.as_str(), route)) else {
                continue;
            };
            let others: Vec<String> = places
                .iter()
                .filter(|(path, line, string)| {
                    (*path, *line) != (file.path.as_str(), literal.line) && (*string || own)
                })
                .map(|(path, line, _)| format!("{path}:{line}"))
                .collect();
            if !others.is_empty() {
                out.push(Hit {
                    path: file.path.clone(),
                    line: literal.line,
                    place: Place::Literal(literal.class.clone()),
                    what: shown(&literal.value),
                    copies: format!("the route is also at {}", others.join(", ")),
                });
            }
        }
    }
    out
}
