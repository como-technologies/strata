//! The cloud side of the contract run: the read-only calls of the tool of a cloud that the
//! cloud-tier rules of strata-preflight depend on.
//!
//! A fact of the kind `cloud` names a call, such as `aws service-quotas get-service-quota`. The run
//! sends it in the lab account and compares the shape of the answer with the answers that the
//! fixtures of the rules hold. The fixtures are the stand-in of a cloud: the tests of the catalog
//! assume them. A fact with a table also checks that the answer lists each value that a rule holds.
//! A table of values that each account sees for itself lacks a value that this account does not
//! have: the account cannot check that value, and it is no difference.
//!
//! The call names the values of the lab as `{name}`. The environment gives them, so no value of the
//! lab is in the repository. The run sends no call that changes anything: each call lists or
//! describes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;
use strata_preflight::probe::{Backend, ExecOutput};

use super::compare;
use super::document::{Difference, DifferenceKind, Entry, Outcome};
use super::shape::Path;
use crate::facts::cloud::{CloudRule, documents};
use crate::facts::{Fact, Kind};

#[cfg(test)]
mod tests;

/// How long the run waits for one call of a cloud.
const TIMEOUT: Duration = Duration::from_secs(120);

/// The prefix of the variable that gives a value of the lab: `STRATA_CONTRACT_REGION` for
/// `{region}`.
const VARIABLE_PREFIX: &str = "STRATA_CONTRACT_";

/// The values of the answer that a difference lists, at most.
const SAMPLE: usize = 10;

/// The characters of an error of the tool that an entry keeps.
const ERROR_TEXT: usize = 200;

/// A value of the lab that has more sources than its own variable.
struct Parameter {
    /// The name in the call: `region` for `{region}`.
    name: &'static str,
    /// The variables that the tool of the cloud reads too, in the order of use.
    from: &'static [&'static str],
    /// The value when nothing gives one. None means that the call is skipped.
    default: Option<&'static str>,
}

/// The values of the lab that the tools of the clouds know by their own variables, or that can be
/// empty.
const PARAMETERS: &[Parameter] = &[
    Parameter {
        name: "region",
        from: &["AWS_REGION", "AWS_DEFAULT_REGION"],
        default: None,
    },
    Parameter {
        name: "project",
        from: &["CLOUDSDK_CORE_PROJECT", "GOOGLE_CLOUD_PROJECT"],
        default: None,
    },
    Parameter {
        name: "gcp_region",
        from: &["CLOUDSDK_COMPUTE_REGION"],
        default: None,
    },
    // the account of the lab names its roles with no prefix, unless the variable says one
    Parameter {
        name: "name_prefix",
        from: &[],
        default: Some(""),
    },
];

/// The values of the lab account, from the environment of the run.
#[derive(Clone, Debug, Default)]
pub struct Lab {
    variables: BTreeMap<String, String>,
}

impl Lab {
    /// The lab of the variables `variables`. A variable with an empty value gives nothing.
    pub fn new(variables: impl IntoIterator<Item = (String, String)>) -> Lab {
        Lab {
            variables: variables
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .collect(),
        }
    }

    /// The lab of the environment of this process. A variable whose name or value is no text is
    /// not one of the lab.
    pub fn from_env() -> Lab {
        Lab::new(std::env::vars_os().filter_map(|(name, value)| {
            Some((name.into_string().ok()?, value.into_string().ok()?))
        }))
    }

    /// The variable that gives the value `name` first.
    pub fn variable(name: &str) -> String {
        format!("{VARIABLE_PREFIX}{}", name.to_uppercase())
    }

    /// The value of the lab named `name`, when one is there.
    fn value(&self, name: &str) -> Option<String> {
        let parameter = PARAMETERS.iter().find(|p| p.name == name);
        let from = parameter.map_or(&[][..], |p| p.from);
        std::iter::once(Lab::variable(name))
            .chain(from.iter().map(|v| (*v).to_string()))
            .find_map(|variable| self.variables.get(&variable).cloned())
            .or_else(|| parameter.and_then(|p| p.default).map(str::to_string))
    }

    /// The words of `call` with each `{name}` replaced by its value. The error is the name of the
    /// first value that the lab does not give.
    fn words(&self, call: &[String]) -> Result<Vec<String>, String> {
        static PLACEHOLDER: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\{([a-z_]+)\}").expect("a pattern"));
        call.iter()
            .map(|word| {
                let mut missing = None;
                let filled = PLACEHOLDER.replace_all(word, |found: &regex::Captures| {
                    let name = &found[1];
                    self.value(name).unwrap_or_else(|| {
                        missing.get_or_insert_with(|| name.to_string());
                        String::new()
                    })
                });
                match missing {
                    Some(name) => Err(name),
                    None => Ok(filled.into_owned()),
                }
            })
            .collect()
    }
}

/// The entry of each fact of the kind `cloud` in `facts`, in the order of the list.
///
/// `rules` are the rules of the cloud tier with the answers of their fixtures. `backend` runs the
/// call, and tells whether the tool is on this machine.
pub fn run(
    facts: &[Fact],
    rules: &BTreeMap<String, CloudRule>,
    backend: &dyn Backend,
    lab: &Lab,
) -> Vec<Entry> {
    facts
        .iter()
        .filter(|fact| fact.kind == Kind::Cloud)
        .map(|fact| check(fact, &documents(fact, rules), backend, lab))
        .collect()
}

/// The entry of one fact of the kind `cloud`.
fn check(fact: &Fact, expected: &[&Value], backend: &dyn Backend, lab: &Lab) -> Entry {
    let entry = |outcome, reason: Option<String>, differences| Entry {
        id: fact.id.clone(),
        kind: Kind::Cloud,
        request: fact.call.join(" "),
        outcome,
        reason,
        differences,
        unknown_values: Vec::new(),
        live: Vec::new(),
        pages: fact.pages.clone(),
        rules: fact.rules.clone(),
    };
    let skipped = |reason: String| entry(Outcome::Skipped, Some(reason), Vec::new());
    let failed = |reason: String| entry(Outcome::Error, Some(reason), Vec::new());
    let Some(tool) = fact.call.first() else {
        return skipped(fact.unchecked.clone().unwrap_or_default());
    };
    if !backend.has_tool(tool) {
        return skipped(format!("{tool} is not on the PATH of the run"));
    }
    if fact.table.is_none() && expected.is_empty() {
        return skipped("no fixture of its rules answers the call".to_string());
    }
    if fact.table.is_some() && fact.reads.fields.len() != 1 {
        return failed("a table reads one field, and the fact reads another number".to_string());
    }
    let words = match lab.words(&fact.call) {
        Ok(words) => words,
        Err(name) => {
            return skipped(format!(
                "the lab gives no value for {{{name}}}: set {}",
                Lab::variable(&name)
            ));
        }
    };
    match answer(backend, &words) {
        Ok(found) => {
            let (outcome, reason, differences) = judged(fact, expected, &found);
            entry(outcome, reason, differences)
        }
        Err(reason) => failed(reason),
    }
}

/// The outcome, the reason, and the differences of a call that answered with `found`.
///
/// A table of values that each account sees for itself, such as the zone ids, is no difference
/// when the answer lacks a value: the account cannot check that value. When the account lists
/// none of the values, it cannot check the fact, and the outcome is `skipped`.
fn judged(
    fact: &Fact,
    expected: &[&Value],
    found: &Value,
) -> (Outcome, Option<String>, Vec<Difference>) {
    let differences = differences(fact, expected, found);
    if !differences.is_empty() {
        return (Outcome::Differs, None, differences);
    }
    let Some(table) = fact.table.as_ref().filter(|table| table.per_account) else {
        return (Outcome::Same, None, differences);
    };
    let field = fact.reads.fields.first().map_or("", String::as_str);
    let (_, lacking) = listing(field, &table.holds, found);
    let names = lacking.join(", ");
    if lacking.is_empty() {
        (Outcome::Same, None, differences)
    } else if lacking.len() == table.holds.len() {
        let reason = format!(
            "the account lists none of the values of the table ({names}). Each account sees its own values, so it cannot check them"
        );
        (Outcome::Skipped, Some(reason), differences)
    } else {
        let reason = format!(
            "the account cannot check {names}. Each account sees its own values, and it lists the others"
        );
        (Outcome::Same, Some(reason), differences)
    }
}

/// What the call `words` printed, as JSON. The error is why the run could not get it.
fn answer(backend: &dyn Backend, words: &[String]) -> Result<Value, String> {
    let output: ExecOutput = backend.exec(words, TIMEOUT).map_err(|e| e.to_string())?;
    let tool = words.first().map_or("", String::as_str);
    if output.exit != 0 {
        let text: String = output.stderr.chars().take(ERROR_TEXT).collect();
        return Err(format!("{tool} exited with {}: {text}", output.exit));
    }
    match output.stdout {
        Value::String(_) | Value::Null => Err(format!("{tool} did not print JSON")),
        found => Ok(found),
    }
}

/// Each difference between the answer `found` and the fixtures `expected`, or the table. A table
/// of an account has none: see [`judged`].
fn differences(fact: &Fact, expected: &[&Value], found: &Value) -> Vec<Difference> {
    match &fact.table {
        Some(table) if table.per_account => Vec::new(),
        Some(table) => table_differences(fact, &table.holds, found),
        None => compare(fact, expected, found),
    }
}

/// The values that the answer `found` lists at `field`, and the values of `holds` that it does not
/// list. A value is listed when it is equal to a listed text or number.
fn listing(field: &str, holds: &[String], found: &Value) -> (BTreeSet<String>, Vec<String>) {
    let listed = Path::parse(field).scalars(found);
    let lacking = holds
        .iter()
        .filter(|value| !listed.contains(value.as_str()))
        .cloned()
        .collect();
    (listed, lacking)
}

/// The difference when the answer does not list each value of the table. The field of the fact is
/// the field that lists the values.
fn table_differences(fact: &Fact, holds: &[String], found: &Value) -> Vec<Difference> {
    let Some(field) = fact.reads.fields.first() else {
        return Vec::new();
    };
    let (listed, missing) = listing(field, holds, found);
    if missing.is_empty() {
        return Vec::new();
    }
    let mut sample: Vec<String> = listed.iter().take(SAMPLE).cloned().collect();
    if listed.len() > SAMPLE {
        sample.push(format!("and {} more", listed.len() - SAMPLE));
    }
    vec![Difference {
        kind: DifferenceKind::Table,
        path: field.clone(),
        expected: missing,
        found: sample,
    }]
}
