//! The checks of the loader: the rules of the format that the parser cannot express, and the
//! compile of each program of a rule.

use std::collections::BTreeSet;
use std::path::Path;

use strata::api::Method;
use strata::closed_set::ClosedSet;

use super::{
    Check, Level, PaletteCheck, Reproduce, Rule, Section, engine_list, is_engine, parse_range,
};
use crate::error::{Error, Result};
use crate::jq::{ITEM, Jq};

/// The rules of the format that the parser cannot express.
pub(super) fn check_section(section: &Section, path: &Path) -> Result<()> {
    let fail = |message: String| Error::Catalog {
        path: path.to_path_buf(),
        message,
    };
    if !is_kebab(&section.section) {
        return Err(fail(format!(
            "section '{}' is not kebab-case",
            section.section
        )));
    }
    let mut ids = BTreeSet::new();
    for rule in &section.rules {
        if !is_kebab(&rule.id) {
            return Err(fail(format!("rule id '{}' is not kebab-case", rule.id)));
        }
        if !ids.insert(rule.id.as_str()) {
            return Err(fail(format!("rule {} is in the file two times", rule.id)));
        }
        check_rule(rule).map_err(|message| fail(format!("rule {}: {message}", rule.id)))?;
    }
    Ok(())
}

/// The rules of the format for one rule, in the order of the fields. `Err` is the reason.
fn check_rule(rule: &Rule) -> std::result::Result<(), String> {
    check_words(rule)?;
    if let Some(check) = &rule.check {
        check_tier_and_programs(rule, check)?;
    }
    check_level(rule)?;
    check_conditions(rule)?;
    check_tests(rule)
}

/// A rule has a title, a `what`, and engines that exist.
fn check_words(rule: &Rule) -> std::result::Result<(), String> {
    if rule.title.trim().is_empty() {
        return Err("no title".into());
    }
    if rule.what.trim().is_empty() {
        return Err("no `what`".into());
    }
    if rule.engines.is_empty() {
        return Err("no engines; name one, or `all`".into());
    }
    match rule.engines.iter().find(|e| !is_engine(e)) {
        Some(e) => Err(format!("engine '{e}' is not one of {}", engine_list())),
        None => Ok(()),
    }
}

/// The check has the tier of its rule, a program, and the fields that its kind needs.
fn check_tier_and_programs(rule: &Rule, check: &Check) -> std::result::Result<(), String> {
    if check.tier() != rule.tier {
        return Err(format!(
            "the check is of kind {}, which is tier {}, and the rule says tier {}",
            check.kind().as_str(),
            check.tier().as_str(),
            rule.tier.as_str()
        ));
    }
    if check.expr().trim().is_empty() {
        return Err("the check has no expr".into());
    }
    if let Check::Exec { argv, .. } = check
        && argv.is_empty()
    {
        return Err("an exec check needs an argv".into());
    }
    if let Check::Palette(PaletteCheck { each, .. }) = check
        && each
            .as_deref()
            .is_some_and(|program| program.trim().is_empty())
    {
        return Err("`each` is empty; drop it, or give a program".into());
    }
    Ok(())
}

/// A preflight check has a check, and the runs of its promotion.
fn check_level(rule: &Rule) -> std::result::Result<(), String> {
    if rule.level != Level::Preflight {
        return Ok(());
    }
    if rule.check.is_none() {
        return Err("a preflight check needs a check".into());
    }
    match &rule.promotion {
        Some(p) if !p.runs.is_empty() => Ok(()),
        _ => Err("a preflight check names the runs of its promotion".into()),
    }
}

/// The fields that say when a rule applies, `when`, `releases`, and `reproduce`, are well formed.
fn check_conditions(rule: &Rule) -> std::result::Result<(), String> {
    if rule.when.as_deref().is_some_and(|w| w.trim().is_empty()) {
        return Err("`when` is empty; drop it, or give a program".into());
    }
    if let Some(range) = &rule.releases
        && parse_range(range).is_none()
    {
        return Err(format!(
            "releases '{range}' is not a range such as '>=4.10' or '>=4.9 <4.11'"
        ));
    }
    if let Reproduce::ByEngine(m) = &rule.reproduce
        && let Some(e) = m.keys().find(|e| !is_engine(e))
    {
        return Err(format!(
            "reproduce names the engine '{e}', which is not one of {}",
            engine_list()
        ));
    }
    Ok(())
}

/// Each test names a fixture, and a rule with a test has a check.
fn check_tests(rule: &Rule) -> std::result::Result<(), String> {
    for t in &rule.tests {
        if t.fault.is_none() && t.clean.is_none() {
            return Err("a test names a fault fixture, a clean fixture, or both".into());
        }
        if rule.check.is_none() {
            return Err("a test needs a check".into());
        }
    }
    Ok(())
}

/// Make sure that every program of every rule of `section` can run: see [`check_programs`].
pub(super) fn check_section_programs(section: &Section, path: &Path) -> Result<()> {
    let jq = Jq::new();
    for rule in &section.rules {
        check_programs(rule, &jq).map_err(|message| Error::Catalog {
            path: path.to_path_buf(),
            message: format!("rule {}: {message}", rule.id),
        })?;
    }
    Ok(())
}

/// Make sure that every program of `rule` compiles, with the variables that it has when it runs.
///
/// A program of a check with `each` reads `$item` in its `path` and in its `body`, and in no other
/// place. `each`, `expr`, `when`, an `argv`, and a `reproduce` line run with no item. So a rule
/// that reads `$item` there cannot run, and the loader refuses it with the place and the reason.
/// A POST with `each` and no `body` is refused too: it sends the entity one time for each
/// item.
fn check_programs(rule: &Rule, jq: &Jq) -> std::result::Result<(), String> {
    let id = rule.id.as_str();
    let describe = |place: &str, e: Error| {
        let reason = match e {
            Error::Jq { message, .. } => message,
            other => other.to_string(),
        };
        format!("{place}: {reason}")
    };
    let program = |place: &str, text: &str, item: bool| {
        jq.check(id, text, item).map_err(|e| describe(place, e))
    };
    let template = |place: &str, text: &str, item: bool| {
        jq.check_template(id, text, item)
            .map_err(|e| describe(place, e))
    };
    if let Some(when) = &rule.when {
        program("`when`", when, false)?;
    }
    match &rule.check {
        Some(Check::Static { expr } | Check::Watch { expr }) => {
            program("`expr`", expr, false)?;
        }
        Some(Check::Palette(PaletteCheck {
            method,
            path,
            body,
            each,
            expr,
        })) => {
            let item = each.is_some();
            if let Some(each) = each {
                program("`each`", each, false)?;
            }
            template("`path`", path, item)?;
            if let Some(body) = body {
                program("`body`", body, item)?;
            }
            if *method == Method::Post && item && body.is_none() {
                return Err(format!(
                    "`body`: a POST with `each` needs a `body` that reads {ITEM}; with none, it sends the entity one time for each item"
                ));
            }
            program("`expr`", expr, false)?;
        }
        Some(Check::Exec { argv, expr, .. }) => {
            for arg in argv {
                template("`argv`", arg, false)?;
            }
            program("`expr`", expr, false)?;
        }
        None => {}
    }
    for line in rule.reproduce.lines() {
        template("`reproduce`", line, false)?;
    }
    Ok(())
}

fn is_kebab(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}
