//! The facts of the clouds: the read-only calls of the tool of a cloud that the cloud-tier rules of
//! strata-preflight depend on, and the tables of cloud facts that the rules hold as text.
//!
//! A rule of the cloud tier runs `aws`, `gcloud`, or `az` and reads the answer. The list names each
//! such call as a fact of the kind `cloud`. The contract run sends the call in the lab account and
//! compares the shape of the answer with the answers that the fixtures of the rules hold. A table
//! of cloud facts in a rule, such as the zone ids that EKS refuses, is a fact too: the answer of
//! its call must list each value. A cloud fact needs no live script.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde_json::Value;
use strata::api::Method;
use strata::closed_set::ClosedSet;
use strata::model::cloud::Cloud;
use strata_preflight::fixture;
use strata_preflight::rule::{Catalog, Check, ExecCheck, Rule, Tier};

use super::model::{Fact, Kind};
use super::sources::Sources;
use crate::files::files_under;

#[cfg(test)]
mod tests;

/// The engines that the lab has an account of. The lab has no account of Azure, MAAS, or vSphere,
/// so a call of those clouds has no contract check.
const LAB: [Cloud; 4] = [Cloud::Aws, Cloud::Eks, Cloud::Gcp, Cloud::Gke];

/// The engine of a rule that checks the lab itself, such as the tools on its PATH. The rule runs a
/// command of this machine, and no cloud answers it.
const ENGINE_LAB: &str = "lab";

/// The directories of the code of strata, from the root of the repository, that hold no table of
/// cloud facts.
const CODE: &[&str] = &["src", "preflight/src", "adapter/src"];

/// The line that starts an item for the unit tests. A module of tests after it is test data.
const TESTS_START: &str = "#[cfg(test)]";

/// The last word of the operation of a call starts with one of these words. A call that starts
/// with another word could change the account, so the list refuses it.
const READ_ONLY: &[&str] = &["list", "describe", "get", "simulate", "show"];

/// The kinds of text that name a cloud fact, each as its name and a pattern. A rule or a file of
/// code that holds such a text holds a table of cloud facts.
const SHAPES: &[(&str, &str)] = &[
    (
        "a zone id",
        r"\b[a-z]{2,4}[0-9]-(?:[a-z]+[0-9]-)?az[0-9]+\b",
    ),
    (
        "an AWS region",
        r"\b(?:us|eu|ap|ca|sa|me|af|il|mx|cn)-(?:gov-)?(?:east|west|north|south|central|northeast|northwest|southeast|southwest)-[0-9]\b",
    ),
    (
        "a GCP region",
        r"\b(?:us|europe|asia|australia|me|africa|northamerica|southamerica)-(?:east|west|north|south|central|northeast|southeast|southwest)[0-9]+\b",
    ),
    (
        "an instance type",
        r"\b[a-z]{1,4}[0-9][a-z0-9]*\.(?:[0-9]*xlarge|large|medium|small|micro|nano|metal(?:-[0-9a-z]+)?)\b",
    ),
    (
        "a machine type",
        r"\b[a-z][0-9][a-z]?-(?:standard|highmem|highcpu|micro|small|medium|custom|ultramem|megamem|hypermem|highgpu|megagpu|ultragpu|metal)(?:-[0-9]+)?\b",
    ),
    (
        "an Azure size",
        r"\b(?:Standard|Basic)_[A-Za-z][A-Za-z0-9_]*\b",
    ),
    ("an image id", r"\bami-[0-9a-f]{8,17}\b"),
    (
        "an AMI type",
        r"\b(?:AL2023|AL2|BOTTLEROCKET|WINDOWS_CORE|WINDOWS_FULL)_[A-Za-z0-9_]+\b",
    ),
    ("a quota code", r"\bL-[0-9A-F]{8}\b"),
];

/// The compiled patterns of [`SHAPES`], with their names.
static PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    SHAPES
        .iter()
        .map(|(name, pattern)| (*name, Regex::new(pattern).expect("a pattern")))
        .collect()
});

/// The answer of a command that a fixture of a rule holds.
#[derive(Clone, Debug)]
pub struct Answer {
    /// The command, as one text with a space between the arguments.
    pub command: String,
    /// What the command printed, as JSON.
    pub stdout: Value,
}

/// A rule of the cloud tier, as the checks of the cloud facts read it.
#[derive(Clone, Debug, Default)]
pub struct CloudRule {
    /// True when the rule applies to a cloud that the lab has an account of.
    pub lab: bool,
    /// True when the rule checks the lab itself and not a cloud: its engine is `lab`.
    pub of_the_lab: bool,
    /// The command of its check, one word for each argument; empty when the rule runs none.
    pub argv: Vec<String>,
    /// The second command of its check, one word for each argument; empty when the check has none.
    pub also: Vec<String>,
    /// The answers that the fixtures of its tests hold.
    pub answers: Vec<Answer>,
}

/// What the checks of the cloud facts compare with.
#[derive(Clone, Debug, Default)]
pub struct CloudSources {
    /// The rules of the cloud tier, by id.
    pub rules: BTreeMap<String, CloudRule>,
    /// The text of the programs and the arguments of every rule, by id: where a table of cloud
    /// facts can be.
    pub programs: BTreeMap<String, String>,
    /// The code of strata that is no test, by the path from the root of the repository.
    pub code: BTreeMap<String, String>,
}

/// The words of a command up to its first flag or its first value of the lab: the tool, the
/// service, and the operation. A rule, a fixture, and a fact name one command when these words are
/// the same, whatever their flags are. A fixture can have a value after them, such as the name of a
/// project.
pub fn command_words<'a>(words: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    words
        .into_iter()
        .take_while(|w| !w.starts_with('-') && !w.contains('{'))
        .collect()
}

/// The answers that the fixtures of the rules of `fact` hold for its command, as documents. A
/// fixture answers the command when the words of the fact start its command.
pub fn documents<'a>(fact: &Fact, rules: &'a BTreeMap<String, CloudRule>) -> Vec<&'a Value> {
    let want = command_words(fact.call.iter().map(String::as_str));
    fact.rules
        .iter()
        .filter_map(|id| rules.get(id))
        .flat_map(|rule| &rule.answers)
        .filter(|answer| {
            answer
                .command
                .split_whitespace()
                .collect::<Vec<_>>()
                .starts_with(&want)
        })
        .map(|answer| &answer.stdout)
        .collect()
}

/// The rules of the cloud tier in `catalog`, the text of the programs of every rule, and a message
/// for each fixture that cannot be read.
pub fn read_rules(catalog: &Catalog) -> (CloudSources, Vec<String>) {
    let mut sources = CloudSources::default();
    let mut bad = Vec::new();
    for rule in catalog.rules() {
        sources.programs.insert(rule.id.clone(), programs_of(rule));
        if rule.effective_tier() != Tier::Cloud {
            continue;
        }
        let mut cloud = CloudRule {
            lab: LAB.iter().any(|c| rule.applies_to(c.as_str())),
            of_the_lab: rule.engines.iter().any(|e| e == ENGINE_LAB),
            ..CloudRule::default()
        };
        if let Some(Check::Exec(ExecCheck { argv, also, .. })) = &rule.check {
            cloud.argv.clone_from(argv);
            cloud.also.clone_from(also);
        }
        let fixtures = rule
            .tests
            .iter()
            .flat_map(|t| [&t.fault, &t.clean])
            .flatten();
        for path in fixtures {
            match fixture::load(path) {
                Ok(world) => cloud.answers.extend(answers_of(&world)),
                Err(e) => bad.push(format!("rule `{}`: {e}", rule.id)),
            }
        }
        sources.rules.insert(rule.id.clone(), cloud);
    }
    (sources, bad)
}

/// The text of the programs and the arguments of the check of `rule`, and of its `when`.
fn programs_of(rule: &Rule) -> String {
    let mut programs = vec![rule.when.clone().unwrap_or_default()];
    if let Some(check) = &rule.check {
        programs.push(check.expr().to_string());
        match check {
            Check::Exec(exec) => {
                programs.extend(exec.argv.iter().chain(&exec.also).cloned());
                programs.extend(exec.palette.iter().map(|read| read.path.clone()));
            }
            Check::Palette(request) => {
                programs.push(request.path.clone());
                programs.extend(request.body.clone());
                programs.extend(request.each.clone());
            }
            Check::Static { .. } | Check::Watch { .. } => {}
        }
    }
    programs.join("\n")
}

/// The answers under `_exec` in a fixture. A command that failed has no document to compare.
fn answers_of(world: &Value) -> Vec<Answer> {
    let Some(exec) = world.get("_exec").and_then(Value::as_object) else {
        return Vec::new();
    };
    exec.iter()
        .filter(|(_, output)| output.get("exit").and_then(Value::as_i64) == Some(0))
        .filter_map(|(command, output)| {
            Some(Answer {
                command: command.clone(),
                stdout: output.get("stdout").filter(|v| !v.is_string())?.clone(),
            })
        })
        .collect()
}

/// The code of strata under `repo` that is no test: each file of the directories of [`CODE`],
/// without the lines of a comment, up to a module of unit tests. A file named `tests.rs`, and a file
/// under a directory `tests`, is a test. The second value holds a message for each file that cannot
/// be read.
pub fn read_code(repo: &Path) -> (BTreeMap<String, String>, Vec<String>) {
    let mut code = BTreeMap::new();
    let mut bad = Vec::new();
    for directory in CODE {
        for file in files_under(&repo.join(directory), "rs") {
            let Ok(relative) = file.strip_prefix(repo) else {
                continue;
            };
            let relative = relative.to_string_lossy().into_owned();
            if file.file_name().is_some_and(|n| n == "tests.rs") || relative.contains("/tests/") {
                continue;
            }
            match std::fs::read_to_string(&file) {
                Ok(text) => {
                    code.insert(relative, without_tests(&text));
                }
                Err(e) => bad.push(format!("{relative}: {e}")),
            }
        }
    }
    (code, bad)
}

/// The lines of `text` that are no comment and that come before the module of its unit tests. An
/// item with `#[cfg(test)]` that is no module, such as an `impl`, does not end the code.
fn without_tests(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let end = lines
        .windows(2)
        .position(|pair| pair[0].trim() == TESTS_START && pair[1].trim_start().starts_with("mod "))
        .unwrap_or(lines.len());
    lines[..end]
        .iter()
        .filter(|line| !line.trim_start().starts_with("//"))
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every fault of the facts of the kind `cloud`, as messages.
pub fn check(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    bad.extend(rules_named(sources));
    bad.extend(calls_agree(sources));
    bad.extend(second_commands_named(sources));
    bad.extend(calls_read_only(sources));
    bad.extend(tables_proven(sources));
    bad.extend(routes_beside(sources));
    bad.extend(literals_covered(sources));
    bad
}

/// The facts of the kind `cloud`.
fn clouds(sources: &Sources) -> impl Iterator<Item = &Fact> {
    sources.facts.iter().filter(|f| f.kind == Kind::Cloud)
}

/// Each rule of the cloud tier is the rule of a fact of the kind `cloud`, and each rule of a fact
/// with no table is a rule of the cloud tier. A table can name a rule of any tier.
///
/// A fact with no call and no table cannot name a rule that runs a command on a cloud of the lab:
/// the contract run sends nothing for that rule. The rule of a row, with no command, the rule of
/// a cloud that the lab lacks, and the rule that checks the lab itself are not such rules.
fn rules_named(sources: &Sources) -> Vec<String> {
    let named: BTreeSet<&String> = clouds(sources).flat_map(|f| &f.rules).collect();
    let mut bad: Vec<String> = sources
        .cloud
        .rules
        .keys()
        .filter(|id| !named.contains(id))
        .map(|id| {
            format!("the rule `{id}` is of the cloud tier, and no fact of the kind cloud names it")
        })
        .collect();
    for fact in clouds(sources).filter(|f| f.table.is_none()) {
        for id in &fact.rules {
            let Some(rule) = sources.cloud.rules.get(id) else {
                if sources.rule_ids.contains(id) {
                    bad.push(format!(
                        "fact `{}`: the rule `{id}` is not of the cloud tier",
                        fact.id
                    ));
                }
                continue;
            };
            if fact.call.is_empty() && !rule.argv.is_empty() && rule.lab && !rule.of_the_lab {
                bad.push(format!(
                    "fact `{}`: the rule `{id}` runs `{}` on a cloud of the lab, and the fact has no call",
                    fact.id,
                    command_words(rule.argv.iter().map(String::as_str)).join(" ")
                ));
            }
        }
    }
    bad
}

/// The call of a fact that is no table is a command of each of its rules, and a fixture of those
/// rules answers it. No two facts send one call.
fn calls_agree(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    let mut seen: BTreeMap<&[String], &str> = BTreeMap::new();
    for fact in clouds(sources).filter(|f| !f.call.is_empty() && f.table.is_none()) {
        let words = command_words(fact.call.iter().map(String::as_str));
        if let Some(other) = seen.insert(&fact.call, &fact.id) {
            bad.push(format!(
                "the facts {other} and {} send one call, `{}`: keep one",
                fact.id,
                fact.call.join(" ")
            ));
        }
        for id in &fact.rules {
            let Some(rule) = sources.cloud.rules.get(id) else {
                continue;
            };
            if !runs(rule, &words) {
                bad.push(format!(
                    "fact `{}`: no command of the rule `{id}` is `{}`",
                    fact.id,
                    words.join(" ")
                ));
            }
        }
        if documents(fact, &sources.cloud.rules).is_empty() {
            bad.push(format!(
                "fact `{}`: no fixture of its rules answers `{}`",
                fact.id,
                words.join(" ")
            ));
        }
    }
    bad
}

/// True when a command of `rule` starts with `words`: its command, or its second command.
fn runs(rule: &CloudRule, words: &[&str]) -> bool {
    [&rule.argv, &rule.also]
        .into_iter()
        .any(|command| command_words(command.iter().map(String::as_str)) == words)
}

/// The second command of a rule that runs on a cloud of the lab is the call of a fact that names
/// the rule. The contract run sends the calls of the list, so a command with no fact has no check.
fn second_commands_named(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    let rules = sources
        .cloud
        .rules
        .iter()
        .filter(|(_, rule)| rule.lab && !rule.of_the_lab && !rule.also.is_empty());
    for (id, rule) in rules {
        let words = command_words(rule.also.iter().map(String::as_str));
        let proved = clouds(sources)
            .filter(|fact| fact.table.is_none() && fact.rules.contains(id))
            .any(|fact| command_words(fact.call.iter().map(String::as_str)) == words);
        if !proved {
            bad.push(format!(
                "the rule `{id}` runs `{}` as its second command on a cloud of the lab, and no fact of the kind cloud has that call",
                words.join(" ")
            ));
        }
    }
    bad
}

/// The operation of each call lists, describes, gets, shows, or simulates. The contract run sends
/// the call each week in the lab account, so a call that changes the account is a fault.
fn calls_read_only(sources: &Sources) -> Vec<String> {
    clouds(sources)
        .filter(|fact| !fact.call.is_empty())
        .filter(|fact| {
            let words = command_words(fact.call.iter().map(String::as_str));
            !words.last().is_some_and(|operation| {
                words.len() > 1 && READ_ONLY.iter().any(|verb| operation.starts_with(verb))
            })
        })
        .map(|fact| {
            format!(
                "fact `{}`: the call `{}` must list, describe, get, show, or simulate",
                fact.id,
                fact.call.join(" ")
            )
        })
        .collect()
}

/// A table has a call that lists its values, unless each rule of the fact is for a cloud that the
/// lab has no account of. Each value of the table is in a program of its rules, unless it is a
/// number: a limit is a number that a program holds in its own way.
fn tables_proven(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in clouds(sources) {
        let Some(table) = &fact.table else {
            continue;
        };
        let id = &fact.id;
        if NaiveDate::parse_from_str(&table.as_of, "%Y-%m-%d").is_err() {
            bad.push(format!("fact `{id}`: `as_of` must be a date, YYYY-MM-DD"));
        }
        if table.holds.is_empty() {
            bad.push(format!("fact `{id}`: the table holds no value"));
        }
        if fact.reads.fields.len() != 1 {
            bad.push(format!(
                "fact `{id}`: a table reads one field, the field that lists its values"
            ));
        }
        let in_lab = fact
            .rules
            .iter()
            .any(|r| sources.cloud.rules.get(r).is_some_and(|rule| rule.lab));
        if fact.call.is_empty() && in_lab {
            bad.push(format!(
                "fact `{id}`: the table has no call, and a rule of the fact is for a cloud of the lab"
            ));
        }
        for value in table.holds.iter().filter(|v| v.parse::<f64>().is_err()) {
            let held = fact.rules.iter().any(|r| {
                sources
                    .cloud
                    .programs
                    .get(r)
                    .is_some_and(|program| program.contains(value.as_str()))
            });
            if !held {
                bad.push(format!(
                    "fact `{id}`: the table holds `{value}`, and no rule of the fact does"
                ));
            }
        }
    }
    bad
}

/// The route of Palette beside a call starts with a method, and names the fields that it lacks.
/// Each one is a field that the call reads: a route that lacks no field answers the whole
/// call, and the rule needs no cloud. A table is no call, so it has no route.
fn routes_beside(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in clouds(sources) {
        let Some(palette) = &fact.palette else {
            continue;
        };
        let id = &fact.id;
        let method = palette.route.split_once(' ').map(|(method, _)| method);
        if !method.is_some_and(|m| Method::parse(m).is_some()) {
            bad.push(format!(
                "fact `{id}`: the route `{}` must start with a method",
                palette.route
            ));
        }
        if palette.lacks.is_empty() {
            bad.push(format!(
                "fact `{id}`: the route names no field that it lacks, so it answers the whole call"
            ));
        }
        for field in palette
            .lacks
            .iter()
            .filter(|f| !fact.reads.fields.contains(f))
        {
            bad.push(format!(
                "fact `{id}`: the route lacks `{field}`, and the fact does not read it"
            ));
        }
        if fact.table.is_some() {
            bad.push(format!("fact `{id}`: a table has no route of Palette"));
        }
    }
    bad
}

/// The texts in `text` that name a cloud fact, each with its kind, once.
fn literals(text: &str) -> BTreeSet<(&'static str, String)> {
    PATTERNS
        .iter()
        .flat_map(|(name, pattern)| {
            pattern
                .find_iter(text)
                .map(move |found| (*name, found.as_str().to_string()))
        })
        .collect()
}

/// No rule and no file of the code holds a text that names a cloud fact, unless a fact of the list
/// proves it. A fact proves a text of a rule when it names the rule, and its table or its call
/// holds the text. The code of strata holds none.
fn literals_covered(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for (id, program) in &sources.cloud.programs {
        let proofs: Vec<&Fact> = clouds(sources).filter(|f| f.rules.contains(id)).collect();
        for (kind, literal) in literals(program) {
            let proven = proofs.iter().any(|f| proves(f, &literal));
            if !proven {
                bad.push(format!(
                    "the rule `{id}` holds {kind}, `{literal}`, and no fact proves it with a call: read it from the cloud, or add a fact with a table"
                ));
            }
        }
    }
    for (path, text) in &sources.cloud.code {
        for (kind, literal) in literals(text) {
            bad.push(format!(
                "the file `{path}` holds {kind}, `{literal}`: read it from the cloud or from a route of Palette"
            ));
        }
    }
    bad
}

/// True when the table or the call of `fact` holds `literal`.
fn proves(fact: &Fact, literal: &str) -> bool {
    let in_table = fact
        .table
        .as_ref()
        .is_some_and(|t| t.holds.iter().any(|v| v == literal));
    in_table || fact.call.iter().any(|word| word.contains(literal))
}
