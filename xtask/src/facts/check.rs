//! The rules that the list keeps: each part of the list agrees with the book, the stand-in, the
//! live scripts, and the rules of strata-preflight. Each function reads a `Sources` and returns
//! one message for each fault.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;

use super::cloud;
use super::model::{Fact, Kind, Piece, is_placeholder, pieces, unread};
use super::sets;
use super::sources::{API_PAGE, FACT_PAGES, Sources, method_pattern, query_names, shape};
use super::subjects;

/// The headings of `internals/palette/` that state nothing about Palette, each as `page#anchor`:
/// they say how strata or the book works. Every other `##` heading needs a fact.
pub const NOT_FACTS: &[&str] = &[
    "internals/palette/limits.md#what-strata-preflight-does-not-check",
    "internals/palette/limits.md#what-the-catalog-does-not-hold-yet",
    "internals/palette/limits.md#where-these-answers-come-from",
    "internals/palette/validate-routes.md#where-these-answers-come-from",
];

/// Every fault of the list, as messages.
pub fn check(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    bad.extend(ids(&sources.facts));
    bad.extend(fields(&sources.facts));
    bad.extend(placeholders_read(&sources.facts));
    bad.extend(pages_exist(sources));
    bad.extend(code_exists(sources));
    bad.extend(codes_stated(sources));
    bad.extend(requests_listed(sources));
    bad.extend(rules_agree(sources));
    bad.extend(queries_held(sources));
    bad.extend(sections_stated(sources));
    bad.extend(live_checks_exist(sources));
    bad.extend(stand_in_agrees(sources));
    bad.extend(cloud::check(sources));
    bad.extend(sets::check(sources));
    bad.extend(subjects::check(&sources.facts));
    bad
}

/// Each group in braces of a request and of a call is a `{name}` that the run reads. Only a request
/// lists alternatives, as `{aws,gcp,azure}`. Any other group reaches the tenant or the tool as
/// text, because no reader fills it. A route of the stand-in has its own check, `well_formed`.
fn placeholders_read(facts: &[Fact]) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in facts {
        let id = &fact.id;
        let request = fact.request.iter().map(|text| ("request", text));
        let call = fact.call.iter().map(|text| ("call", text));
        for (place, text) in request.chain(call.clone()) {
            for group in unread(text) {
                bad.push(format!(
                    "fact `{id}`: the {place} holds `{group}`, and no reader fills it: a placeholder is `{{name}}`, with letters, digits, and `_`"
                ));
            }
        }
        for (place, text) in call {
            if pieces(text)
                .iter()
                .any(|piece| matches!(piece, Piece::Alternatives(_)))
            {
                bad.push(format!(
                    "fact `{id}`: the {place} lists alternatives, and only a request does"
                ));
            }
        }
    }
    bad
}

/// An id is lower case words with dashes, and no two facts have one id.
fn ids(facts: &[Fact]) -> Vec<String> {
    let word = Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").expect("a pattern");
    let mut seen = BTreeSet::new();
    let mut bad = Vec::new();
    for fact in facts {
        if !word.is_match(&fact.id) {
            bad.push(format!(
                "fact `{}`: the id must be words with dashes",
                fact.id
            ));
        }
        if !seen.insert(&fact.id) {
            bad.push(format!("fact `{}`: another fact has this id", fact.id));
        }
    }
    bad
}

/// The fields that each fact must have, by its kind.
fn fields(facts: &[Fact]) -> Vec<String> {
    let method = Regex::new(&format!(r"^({}) /v1/\S+$", method_pattern())).expect("a pattern");
    let mut bad = Vec::new();
    for fact in facts {
        let id = &fact.id;
        match (&fact.request, fact.kind) {
            (Some(request), Kind::Request) if !method.is_match(request) => bad.push(format!(
                "fact `{id}`: the request `{request}` must be GET or POST and a path under /v1/"
            )),
            (None, Kind::Request) => {
                bad.push(format!("fact `{id}`: a request fact needs a request"))
            }
            (Some(_), Kind::Behavior | Kind::Gap | Kind::Cloud) => {
                bad.push(format!("fact `{id}`: only a request fact has a request"));
            }
            _ => {}
        }
        if !fact.absent_codes.is_empty() && fact.get_path().is_none() {
            bad.push(format!(
                "fact `{id}`: only a fact with a GET request has `absent_codes`"
            ));
        }
        bad.extend(proof(fact));
    }
    bad
}

/// The fields that a fact needs to be proved: a live check, or a reason why none proves it. A
/// fact of the kind `cloud` is proved by a call that the contract run sends. It needs a call or a
/// reason, and it names no live check.
fn proof(fact: &Fact) -> Vec<String> {
    let id = &fact.id;
    let cloud = fact.kind == Kind::Cloud;
    let proved = if cloud {
        !fact.call.is_empty()
    } else {
        !fact.live.is_empty()
    };
    // a cloud fact that no call proves can state its reasons in `unchecked` alone
    let reads_needed = !cloud || proved;
    let mut bad = Vec::new();
    for (name, empty) in [
        ("reads", reads_needed && fact.reads.is_empty()),
        ("pages", fact.pages.is_empty()),
        (
            "code or rules",
            fact.code.is_empty() && fact.rules.is_empty(),
        ),
    ] {
        if empty {
            bad.push(format!("fact `{id}`: `{name}` is empty"));
        }
    }
    let means = if cloud {
        "no call proves it"
    } else {
        "no live check proves it"
    };
    let has = if cloud {
        "it has a call"
    } else {
        "it has a live check"
    };
    match (proved, fact.unchecked.as_deref()) {
        (false, None) => bad.push(format!(
            "fact `{id}`: {means}, and `unchecked` gives no reason"
        )),
        (true, Some(_)) => bad.push(format!("fact `{id}`: {has}, so `unchecked` must go")),
        (false, Some(reason)) if reason.trim().is_empty() => {
            bad.push(format!("fact `{id}`: `unchecked` is empty"));
        }
        _ => {}
    }
    if cloud {
        bad.extend(cloud_fields(fact));
    } else if !fact.call.is_empty() || fact.table.is_some() {
        bad.push(format!(
            "fact `{id}`: only a fact of the kind cloud has a call or a table"
        ));
    }
    bad
}

/// The fields that a fact of the kind `cloud` must not have: a live check, and a route of the
/// stand-in. The fixtures of its rules are its stand-in.
fn cloud_fields(fact: &Fact) -> Vec<String> {
    let id = &fact.id;
    let mut bad = Vec::new();
    if !fact.live.is_empty() {
        bad.push(format!(
            "fact `{id}`: the contract run proves a cloud fact, so it names no live check"
        ));
    }
    if !fact.stand_in.is_empty() {
        bad.push(format!(
            "fact `{id}`: the fixtures of its rules are the stand-in of a cloud fact, so it names no route"
        ));
    }
    if fact.rules.is_empty() {
        bad.push(format!(
            "fact `{id}`: a cloud fact names a rule of the cloud tier"
        ));
    }
    bad
}

/// Split `page#anchor` into the path and the anchor.
fn page_and_anchor(page: &str) -> (&str, Option<&str>) {
    match page.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (page, None),
    }
}

/// Each page of a fact is a page of the book, and its anchor is a heading of that page.
fn pages_exist(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        for page in &fact.pages {
            let (path, anchor) = page_and_anchor(page);
            match (sources.anchors.get(path), anchor) {
                (None, _) => bad.push(format!(
                    "fact `{}`: the page `{path}` is not a page of the book",
                    fact.id
                )),
                (Some(headings), Some(anchor)) if !headings.contains(anchor) => bad.push(format!(
                    "fact `{}`: the page `{path}` has no heading `#{anchor}`",
                    fact.id
                )),
                _ => {}
            }
        }
    }
    bad
}

/// Each file of strata that a fact names exists.
fn code_exists(sources: &Sources) -> Vec<String> {
    sources
        .facts
        .iter()
        .flat_map(|fact| fact.code.iter().map(move |path| (fact, path)))
        .filter(|(_, path)| !sources.files.contains(*path))
        .map(|(fact, path)| format!("fact `{}`: the file `{path}` does not exist", fact.id))
        .collect()
}

/// Each error code that a fact reads, or takes for an absent object, is on one of its pages.
fn codes_stated(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        for code in fact.reads.codes.iter().chain(&fact.absent_codes) {
            let stated = fact.pages.iter().any(|page| {
                sources
                    .texts
                    .get(page_and_anchor(page).0)
                    .is_some_and(|text| text.contains(code.as_str()))
            });
            if !stated {
                bad.push(format!(
                    "fact `{}`: no page of the fact has the code `{code}`",
                    fact.id
                ));
            }
        }
    }
    bad
}

/// The facts of the kind `request`, by the shape of the request.
fn by_request(facts: &[Fact]) -> BTreeMap<String, Vec<&Fact>> {
    let mut map: BTreeMap<String, Vec<&Fact>> = BTreeMap::new();
    for fact in facts {
        if let Some(request) = &fact.request {
            map.entry(shape(request)).or_default().push(fact);
        }
    }
    map
}

/// Each request of `reference/palette-api.md` and each request of a rule of strata-preflight is
/// the request of a fact. No request is the request of two facts.
fn requests_listed(sources: &Sources) -> Vec<String> {
    let known = by_request(&sources.facts);
    let mut bad = Vec::new();
    for (request, facts) in &known {
        if facts.len() > 1 {
            let ids: Vec<&str> = facts.iter().map(|f| f.id.as_str()).collect();
            bad.push(format!(
                "the facts {} have one request, `{request}`: keep one",
                ids.join(", ")
            ));
        }
    }
    for request in &sources.requests {
        if !known.contains_key(request) {
            bad.push(format!(
                "{API_PAGE} lists `{request}`, and no fact has this request"
            ));
        }
    }
    for (request, rules) in &sources.rule_requests {
        if !known.contains_key(request) {
            let rules: Vec<&str> = rules.iter().map(String::as_str).collect();
            bad.push(format!(
                "the rules {} send `{request}`, and no fact has this request",
                rules.join(", ")
            ));
        }
    }
    bad
}

/// The rules of a fact exist, and they are the rules that send its request.
fn rules_agree(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        for rule in &fact.rules {
            if !sources.rule_ids.contains(rule) {
                bad.push(format!(
                    "fact `{}`: the rule `{rule}` does not exist",
                    fact.id
                ));
            }
        }
        let sent: BTreeSet<&String> = fact
            .request
            .as_ref()
            .and_then(|request| sources.rule_requests.get(&shape(request)))
            .into_iter()
            .flatten()
            .collect();
        let named: BTreeSet<&String> = fact.rules.iter().collect();
        if fact.request.is_some() && sent != named {
            bad.push(format!(
                "fact `{}`: `rules` is {:?}, and the rules that send the request are {:?}",
                fact.id, named, sent
            ));
        }
    }
    bad
}

/// The request of a fact holds each query that its rules send in their paths. A rule sends the
/// query that its route needs, so a request that lacks one is not the request that Palette gets.
fn queries_held(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        let Some(request) = &fact.request else {
            continue;
        };
        let held = query_names(request);
        for rule in &fact.rules {
            let lacking: Vec<&str> = sources
                .rule_queries
                .get(rule)
                .into_iter()
                .flatten()
                .filter(|name| !held.contains(*name))
                .map(String::as_str)
                .collect();
            if !lacking.is_empty() {
                bad.push(format!(
                    "fact `{}`: the rule `{rule}` sends the query {} in its path, and the request `{request}` lacks it",
                    fact.id,
                    lacking.join(", ")
                ));
            }
        }
    }
    bad
}

/// Each `##` section of the pages in `internals/palette/` is the page of a fact, except the
/// sections of [`NOT_FACTS`].
fn sections_stated(sources: &Sources) -> Vec<String> {
    let named: BTreeSet<&str> = sources
        .facts
        .iter()
        .flat_map(|f| f.pages.iter().map(String::as_str))
        .collect();
    sources
        .sections
        .iter()
        .filter(|section| {
            !named.contains(section.as_str()) && !NOT_FACTS.contains(&section.as_str())
        })
        .map(|section| format!("{section} is a section of {FACT_PAGES}/, and no fact names it"))
        .collect()
}

/// True when `a` and `b` name one check. A script builds a name from a variable (`fault-$id`),
/// and the book writes it with a placeholder (`fault-<id>`). So one name can be a pattern for
/// the other.
fn same_check(a: &str, b: &str) -> bool {
    a == b || template_matches(a, b) || template_matches(b, a)
}

/// True when `template` has a variable, such as `fault-$id` or `fault-<id>`, and `name` is the
/// template with a text for each variable.
pub fn template_matches(template: &str, name: &str) -> bool {
    static VARIABLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\$\{?\w+\}?|<[^<>]*>").expect("a pattern"));
    let mut pattern = String::from("^");
    let mut last = 0;
    for found in VARIABLE.find_iter(template) {
        pattern.push_str(&regex::escape(&template[last..found.start()]));
        pattern.push_str(".+");
        last = found.end();
    }
    if last == 0 {
        return false;
    }
    pattern.push_str(&regex::escape(&template[last..]));
    pattern.push('$');
    Regex::new(&pattern).is_ok_and(|re| re.is_match(name))
}

/// A live check of a fact is a check that its script calls, and the book has its row. A fact with
/// no live check names no check.
fn live_checks_exist(sources: &Sources) -> Vec<String> {
    sources
        .facts
        .iter()
        .flat_map(|fact| fact.live.iter().map(move |live| (fact, live)))
        .filter_map(|(fact, live)| {
            let fault = live_fault(sources, live)?;
            Some(format!("fact `{}`: {fault}", fact.id))
        })
        .collect()
}

/// The fault of one live check, written `script.sh:check`, or `None` when the script calls it and
/// the book has its row.
fn live_fault(sources: &Sources, live: &str) -> Option<String> {
    let Some((script, check)) = live.split_once(':') else {
        return Some(format!("`{live}` must be `script.sh:check`"));
    };
    let Some(called) = sources.checks.get(script) else {
        return Some(format!("examples/live/ has no script `{script}`"));
    };
    if !called.iter().any(|call| same_check(call, check)) {
        return Some(format!("the script `{script}` calls no check `{check}`"));
    }
    let has_row = sources
        .rows
        .get(script)
        .is_some_and(|rows| rows.iter().any(|row| same_check(row, check)));
    (!has_row).then(|| format!("the book has no row for the check `{check}` of `{script}`"))
}

/// A route matches a pattern when the two have the same number of segments, and each segment is
/// equal or is `{name}` in the pattern.
pub fn route_matches(pattern: &str, route: &str) -> bool {
    let (pattern, route): (Vec<&str>, Vec<&str>) =
        (pattern.split('/').collect(), route.split('/').collect());
    pattern.len() == route.len()
        && pattern
            .iter()
            .zip(&route)
            .all(|(p, r)| is_placeholder(p) || p == r)
}

/// A pattern of the stand-in is a path under `v1/`, with a whole segment as `{name}`.
fn well_formed(pattern: &str) -> bool {
    pattern.starts_with("v1/")
        && pattern.split('/').all(|segment| {
            is_placeholder(segment) || (!segment.is_empty() && !segment.contains(['{', '}']))
        })
}

/// Each route that the stand-in serves matches the `stand_in` of a fact, and each `stand_in`
/// matches a route that the stand-in serves. The second rule needs a run of the book.
fn stand_in_agrees(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        for pattern in &fact.stand_in {
            if !well_formed(pattern) {
                bad.push(format!(
                    "fact `{}`: the stand-in route `{pattern}` must be a path under v1/, with `{{name}}` for a whole segment",
                    fact.id
                ));
            }
        }
    }
    let Some(served) = &sources.served else {
        return bad;
    };
    let patterns: Vec<(&Fact, &String)> = sources
        .facts
        .iter()
        .flat_map(|fact| fact.stand_in.iter().map(move |pattern| (fact, pattern)))
        .collect();
    for route in served {
        if !patterns
            .iter()
            .any(|(_, pattern)| route_matches(pattern, route))
        {
            bad.push(format!(
                "the stand-in serves `{route}`, and no fact names this route"
            ));
        }
    }
    for (fact, pattern) in patterns {
        if !served.iter().any(|route| route_matches(pattern, route)) {
            bad.push(format!(
                "fact `{}`: the stand-in serves no route like `{pattern}`",
                fact.id
            ));
        }
    }
    bad
}
