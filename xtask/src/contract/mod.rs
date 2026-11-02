//! The contract run: it asks a real tenant the requests of the facts of Palette, and compares each
//! answer with the stand-in.
//!
//! The list of the facts states what strata depends on. The stand-in imitates it, and the book
//! tells it. This module finds out whether Palette still agrees with the stand-in.
//!
//! For each fact of the kind `request` that a read-only request can show, the run finds an object in
//! the tenant and sends the request. It compares the answer with the documents that the stand-in
//! serves for the fact. It compares the status and the type of each field that strata reads. It
//! checks that the answer holds each name that strata needs from a set that Palette can extend. It
//! also lists each value of a closed set that neither the fact nor the stand-in names: strata does
//! not know such a value. It compares shapes, and never the values of two tenants. A fact can name
//! the error codes that mean the tenant has no such object. The run skips the fact for an HTTP 404
//! with one of them.
//!
//! The result is the document `strata-contract/1`, with one entry for each fact. The run changes
//! nothing in the tenant: it sends GET requests only. The [`cloud`] module adds the entry of each
//! fact of the kind `cloud`, a read-only call of the tool of a cloud. [The list of the
//! facts](../../../docs/src/development/facts.md#the-weekly-contract-run) tells how the run works.

use std::collections::BTreeSet;

use serde_json::Value;
use strata::api::{Method, PaletteApi};
use strata_preflight::world::unreadable;

use crate::facts::{Fact, Kind};

mod cloud;
mod command;
mod document;
mod releases;
mod shape;
mod stand_in;
mod subject;
#[cfg(test)]
mod tests;

pub use command::{command, harvest_command};
use document::{Contract, Difference, DifferenceKind, Entry, Outcome, SCHEMA, Summary, Unknown};
use shape::{JsonType, Path};
pub use stand_in::StandIn;
use subject::{Probe, Subject};

/// Ask the tenant that `client` talks to the request of each fact in `facts` of the kind
/// `request`, and compare the answers with `stand_in`. `tenant` talks to the same tenant and names
/// no project: the run sends it each fact of the scope [`crate::facts::Scope::Tenant`]. `target` is
/// the address of the tenant and `checked_at` the time of the run, for the document.
pub fn run<A: PaletteApi>(
    facts: &[Fact],
    stand_in: &StandIn,
    client: &A,
    tenant: &A,
    target: &str,
    checked_at: &str,
) -> Contract {
    let probe = Probe::new(client, tenant);
    let entries: Vec<Entry> = facts
        .iter()
        .filter(|fact| fact.kind == Kind::Request)
        .map(|fact| check(&probe, facts, stand_in, fact))
        .collect();
    Contract {
        schema: SCHEMA,
        checked_at: checked_at.to_string(),
        target: target.to_string(),
        summary: Summary::of(&entries),
        facts: entries,
        releases: Vec::new(),
    }
}

/// The entry of one fact.
fn check<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    stand_in: &StandIn,
    fact: &Fact,
) -> Entry {
    let request = fact.request.clone().unwrap_or_default();
    let entry = |outcome, reason: Option<String>, differences| Entry {
        id: fact.id.clone(),
        kind: Kind::Request,
        request: request.clone(),
        outcome,
        reason,
        differences,
        unknown_values: Vec::new(),
        live: fact.live.clone(),
        pages: fact.pages.clone(),
        rules: Vec::new(),
    };
    let skipped = |reason: &str| entry(Outcome::Skipped, Some(reason.to_string()), Vec::new());
    let Some((method, template)) = fact.parts() else {
        return skipped("the fact states no request");
    };
    if method != Method::Get {
        return skipped("the request sends a body: its live checks send it");
    }
    if !fact.reads.reads_a_document() {
        return skipped(
            "the fact reads no field, no value, and no name: it reads the code of a refusal",
        );
    }
    let documents = stand_in.documents(&fact.stand_in);
    if documents.is_empty() {
        return skipped("the stand-in serves no document for it");
    }
    let path = match subject::find(probe, facts, template) {
        Subject::Path(path) => path,
        Subject::Missing(why) => return skipped(&why),
        Subject::Failed(failure) => {
            return entry(Outcome::Error, Some(failure.message), Vec::new());
        }
    };
    match probe.get(fact.scope, &path).as_ref() {
        Ok(found) => {
            let differences = compare(fact, &documents, found);
            let outcome = if differences.is_empty() {
                Outcome::Same
            } else {
                Outcome::Differs
            };
            Entry {
                unknown_values: unknown_values(fact, &documents, found),
                ..entry(outcome, None, differences)
            }
        }
        Err(failure) => {
            if let Some(code) = failure.absent(&fact.absent_codes) {
                return skipped(&format!(
                    "the tenant has no such object: Palette answers HTTP 404 with the code `{code}`"
                ));
            }
            match failure.status.filter(|status| unreadable(*status)) {
                Some(status) => entry(
                    Outcome::Differs,
                    None,
                    vec![Difference {
                        kind: DifferenceKind::Status,
                        path: format!("GET {template}"),
                        expected: vec!["200".to_string()],
                        found: vec![status.to_string()],
                    }],
                ),
                None => entry(Outcome::Error, Some(failure.message.clone()), Vec::new()),
            }
        }
    }
}

/// Each difference between the answer `found` of the tenant and the documents `expected` of the
/// stand-in, for the fields and the names that `fact` reads.
fn compare(fact: &Fact, expected: &[&Value], found: &Value) -> Vec<Difference> {
    let mut differences = Vec::new();
    for field in &fact.reads.fields {
        let path = Path::parse(field);
        let wanted: BTreeSet<JsonType> = expected.iter().flat_map(|d| path.types(d)).collect();
        differences.extend(type_difference(field, &wanted, &path.types(found)));
    }
    differences.extend(missing_names(fact, found));
    differences
}

/// The difference of each set of names of `fact` for which the answer `found` of the tenant lacks
/// a name that strata needs.
///
/// Palette can add names to such a set, and strata ignores them. So an extra name is no
/// difference. An answer with no value at the field has nothing to compare, as a list with no
/// entry has no type to compare.
fn missing_names(fact: &Fact, found: &Value) -> Vec<Difference> {
    let mut differences = Vec::new();
    for (field, names) in &fact.reads.names {
        let present = Path::parse(field).texts(found);
        let lacking: Vec<String> = names
            .iter()
            .filter(|name| !present.contains(*name))
            .cloned()
            .collect();
        if present.is_empty() || lacking.is_empty() {
            continue;
        }
        differences.push(Difference {
            kind: DifferenceKind::Name,
            path: field.clone(),
            expected: lacking,
            found: present.into_iter().collect(),
        });
    }
    differences
}

/// Each closed set of `fact` for which the answer `found` of the tenant holds a value that neither
/// the fact nor the documents `expected` of the stand-in name.
///
/// The fact names the values that strata tells apart, and the stand-in can hold more. A value that
/// neither names is a value that Palette added: strata does not know it. The answer is not a
/// difference of the fact, so a new value alone leaves the outcome `same`.
fn unknown_values(fact: &Fact, expected: &[&Value], found: &Value) -> Vec<Unknown> {
    let mut unknown = Vec::new();
    for (field, names) in &fact.reads.values {
        let path = Path::parse(field);
        let mut known: BTreeSet<String> = names.iter().cloned().collect();
        known.extend(expected.iter().flat_map(|d| path.texts(d)));
        let values: Vec<String> = path.texts(found).difference(&known).cloned().collect();
        if !values.is_empty() {
            unknown.push(Unknown {
                set: field.clone(),
                known: known.into_iter().collect(),
                values,
            });
        }
    }
    unknown
}

/// The difference at one field between the types that the stand-in has (`wanted`) and the types
/// that the answer has (`got`), when there is one.
///
/// A field that the stand-in sometimes lacks can be missing from the answer. A field that the
/// stand-in always has is not missing from every place of the answer. A field that neither side
/// reaches has no difference.
fn type_difference(
    field: &str,
    wanted: &BTreeSet<JsonType>,
    got: &BTreeSet<JsonType>,
) -> Option<Difference> {
    if wanted.is_empty() || got.is_empty() {
        return None;
    }
    let present = |types: &BTreeSet<JsonType>| -> Vec<String> {
        types
            .iter()
            .filter(|t| **t != JsonType::Absent)
            .map(|t| t.name())
            .collect()
    };
    let new_type = got
        .iter()
        .any(|t| *t != JsonType::Absent && !wanted.contains(t));
    let gone = got.iter().all(|t| *t == JsonType::Absent) && !wanted.contains(&JsonType::Absent);
    let kind = match (new_type, gone) {
        (true, _) => DifferenceKind::Type,
        (false, true) => DifferenceKind::Missing,
        (false, false) => return None,
    };
    Some(Difference {
        kind,
        path: field.to_string(),
        expected: present(wanted),
        found: if gone {
            vec![JsonType::Absent.name()]
        } else {
            present(got)
        },
    })
}
