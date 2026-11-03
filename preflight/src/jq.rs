//! The jq programs of the rules, run by jaq.
//!
//! A program compiles one time and runs on many inputs. A template such as
//! `/v1/spectroclusters/{{ .cloud }}/validate` fills each `{{ jq }}` with the first output of that
//! program, as text.
//!
//! A program can read `$item`, the item of a check with `each`: the caller gives the item with
//! the input. A program that the caller runs with no item does not compile when it reads `$item`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use jaq_core::load::{Arena, File, Loader};
use jaq_core::native::{Fun, bome, run, unary, v};
use jaq_core::{Compiler, Ctx, Error as JaqError, ValR, Vars, data};
use jaq_json::Val;
use jaq_std::ValT;
use serde_json::Value;

use strata::reference::version;
use strata::render::count_of;

use crate::error::{Error, Result};

mod comment;
mod depth;
use depth::{MAX_DEPTH, depth};

pub(crate) use comment::without_comments;

type Filter = jaq_core::Filter<data::JustLut<Val>>;

/// A definition of the standard library of jq.
type Def = jaq_core::load::parse::Def<&'static str>;

/// The variable that holds the item of a check with `each`.
pub(crate) const ITEM: &str = "$item";

/// Compiles and runs jq programs, and keeps each compiled program for the next input.
pub struct Jq {
    /// The definitions of the standard library of jq, read one time: each program starts from a
    /// copy. A load of the catalog compiles every program, so a read of the library for each one
    /// costs most of the time of the load.
    library: Vec<Def>,
    /// Each program, compiled without `$item` (slot 0) and with it as a variable (slot 1). The key
    /// is the program alone, so a lookup borrows the text of the program.
    compiled: RefCell<HashMap<String, [Option<Rc<Filter>>; 2]>>,
}

impl Default for Jq {
    fn default() -> Self {
        Jq {
            library: jaq_core::defs()
                .chain(jaq_std::defs())
                .chain(jaq_json::defs())
                .collect(),
            compiled: RefCell::default(),
        }
    }
}

/// The name of the filter that this crate adds to the standard library of jq.
const SEMVER_NUMBERS: &str = "semver_numbers";

/// The definition of [`SEMVER_NUMBERS`] in jq, for the `jq` tool, which has no such filter. It
/// gives the same answer as [`semver_numbers`], and a test compares the two on a table of texts.
/// The regular expression is the grammar that [`version::numbers`] parses: an optional `v`, one to
/// three numbers, an optional pre-release after `-`, and optional build data after `+`. A number
/// of 20 digits or more is outside the table, because the tool reads a number as a double.
const SEMVER_NUMBERS_DEF: &str = r#"def semver_numbers: ltrimstr("v") | if test("\\A[0-9]+(\\.[0-9]+){0,2}(-[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?(\\+[0-9A-Za-z-]+(\\.[0-9A-Za-z-]+)*)?\\z") then split("+")[0] | split("-")[0] | split(".") | map(tonumber) else null end; "#;

/// The name of the filter that gives a count with its noun: see [`count_of_filter`].
const COUNT_OF: &str = "count_of";

/// The definition of [`COUNT_OF`] in jq, for the `jq` tool, which has no such filter. It gives
/// the same words as [`count_of`], and a test compares the two on a table of counts.
const COUNT_OF_DEF: &str = r#"def count_of($noun): if . == 0 then "no \($noun)s" elif . == 1 then "1 \($noun)" else "\(.) \($noun)s" end; "#;

/// The definitions of the filters that `program` names and the `jq` tool lacks, to put in front
/// of the program. Empty when it names none.
pub(crate) fn tool_definitions(program: &str) -> String {
    [
        (SEMVER_NUMBERS, SEMVER_NUMBERS_DEF),
        (COUNT_OF, COUNT_OF_DEF),
    ]
    .into_iter()
    .filter(|(name, _)| program.contains(name))
    .map(|(_, definition)| definition)
    .collect()
}

/// The filters that this crate adds to the standard library of jq.
///
/// A rule reads a version with `semver_numbers`, so that the one module of `strata` that parses a
/// version parses it. A rule writes a count with `count_of_filter`, so that its words are those
/// of the reports.
fn added_funs() -> impl Iterator<Item = Fun<data::JustLut<Val>>> {
    [
        run((SEMVER_NUMBERS, v(0), |cv| bome(semver_numbers(&cv.1)))),
        run((COUNT_OF, v(1), |cv| unary(cv, count_of_filter))),
    ]
    .into_iter()
}

/// `semver_numbers`: the numbers of the version in the input, as [`version::numbers`] reads them.
///
/// `4.10.3-rc.1` gives `[4, 10, 3]`, and `4.10` gives `[4, 10]`. A text that is no version gives
/// `null`. The input must be a text.
fn semver_numbers(input: &Val) -> ValR<Val> {
    let text = std::str::from_utf8(input.try_as_utf8_bytes()?).ok();
    Ok(text
        .and_then(version::numbers)
        .map_or(Val::Null, |numbers| {
            numbers.into_iter().map(|n| Val::from(n as usize)).collect()
        }))
}

/// `count_of`: the count in the input and its noun, as [`count_of`] writes them.
///
/// `2 | count_of("key pair")` gives `2 key pairs`. `1` gives `1 key pair`, and `0` gives
/// `no key pairs`. The noun is in the singular: the filter adds the `s`. The input must be a
/// number that is not negative, and `noun` must be a text.
fn count_of_filter(count: Val, noun: Val) -> ValR<Val> {
    let Some(n) = count.as_isize().and_then(|n| usize::try_from(n).ok()) else {
        return Err(JaqError::typ(count, "non-negative integer"));
    };
    let noun = std::str::from_utf8(noun.try_as_utf8_bytes()?).map_err(JaqError::str)?;
    Ok(Val::from(count_of(n, noun)))
}

impl Jq {
    /// A new engine with nothing compiled.
    pub fn new() -> Self {
        Self::default()
    }

    fn compile(&self, rule: &str, program: &str, with_item: bool) -> Result<Rc<Filter>> {
        let slot = usize::from(with_item);
        if let Some(f) = self
            .compiled
            .borrow()
            .get(program)
            .and_then(|forms| forms[slot].clone())
        {
            return Ok(f);
        }
        if depth(program) > MAX_DEPTH {
            return Err(Error::Jq {
                rule: rule.to_string(),
                message: format!("the program nests more than {MAX_DEPTH} levels"),
            });
        }
        let funs = jaq_core::funs()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .chain(added_funs());
        let loader = Loader::new(self.library.iter().cloned());
        let arena = Arena::default();
        let modules = loader
            .load(
                &arena,
                File {
                    code: program,
                    path: (),
                },
            )
            .map_err(|errs| Error::Jq {
                rule: rule.to_string(),
                message: format!("the program does not parse: {}", load_errors(&errs)),
            })?;
        let globals: &[&str] = if with_item { &[ITEM] } else { &[] };
        let filter = Compiler::default()
            .with_funs(funs)
            .with_global_vars(globals.iter().copied())
            .compile(modules)
            .map_err(|errs| Error::Jq {
                rule: rule.to_string(),
                message: format!("the program does not compile: {}", compile_errors(&errs)),
            })?;
        let filter = Rc::new(filter);
        self.compiled
            .borrow_mut()
            .entry(program.to_string())
            .or_default()[slot] = Some(filter.clone());
        Ok(filter)
    }

    /// Run `program` on `input` and give every output, in order.
    ///
    /// `item` is the value of `$item`, for a check with `each`. With no item, a program that reads
    /// `$item` does not compile.
    ///
    /// # Errors
    ///
    /// [`Error::Jq`] when the program does not parse, does not compile, or fails on `input`. A
    /// program that reads `$item` does not compile when `item` is `None`.
    pub fn run(
        &self,
        rule: &str,
        program: &str,
        input: &Value,
        item: Option<&Value>,
    ) -> Result<Vec<Value>> {
        let filter = self.compile(rule, program, item.is_some())?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new(item.map(to_val)));
        let input = to_val(input);
        let mut out = Vec::new();
        for output in filter.id.run((ctx, input)) {
            match output.map_err(|e| e.get_err()) {
                Ok(v) => out.push(to_json(&v)),
                Err(Ok(e)) => {
                    return Err(Error::Jq {
                        rule: rule.to_string(),
                        message: format!("the program failed: {e}"),
                    });
                }
                Err(Err(_)) => {
                    return Err(Error::Jq {
                        rule: rule.to_string(),
                        message: "the program ended with a break outside a label".to_string(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Run `program` and give its first output, or `null` when it gives none.
    ///
    /// # Errors
    ///
    /// The errors of [`Jq::run`].
    pub fn first(
        &self,
        rule: &str,
        program: &str,
        input: &Value,
        item: Option<&Value>,
    ) -> Result<Value> {
        Ok(self
            .run(rule, program, input, item)?
            .into_iter()
            .next()
            .unwrap_or(Value::Null))
    }

    /// Make sure that `program` compiles, with `$item` when `with_item` is true and without it
    /// when not. The program does not run.
    ///
    /// A rule file calls this when it loads, with the variables that the program has when it
    /// runs. So a program that cannot run is an error of the file, and not an error of a run.
    ///
    /// # Errors
    ///
    /// [`Error::Jq`] when the program does not parse, or does not compile: a program that reads
    /// `$item` does not compile when `with_item` is false.
    pub fn check(&self, rule: &str, program: &str, with_item: bool) -> Result<()> {
        self.compile(rule, program, with_item).map(drop)
    }

    /// Make sure that each program of `template` compiles, as [`Jq::check`] does.
    ///
    /// # Errors
    ///
    /// [`Error::Jq`] when `template` has a `{{` with no `}}`, and the errors of [`Jq::check`]
    /// for each program of the template.
    pub fn check_template(&self, rule: &str, template: &str, with_item: bool) -> Result<()> {
        for part in parts(rule, template)? {
            if let Part::Program(program) = part {
                self.check(rule, program, with_item)?;
            }
        }
        Ok(())
    }

    /// Fill each `{{ jq }}` of `template` with the first output of that program on `input`, as
    /// text: a string stays as it is, and another value is its JSON. `item` is the value of
    /// `$item` in each program.
    ///
    /// # Errors
    ///
    /// [`Error::Jq`] when `template` has a `{{` with no `}}`, and the errors of [`Jq::run`] for
    /// each program of the template.
    pub fn render(
        &self,
        rule: &str,
        template: &str,
        input: &Value,
        item: Option<&Value>,
    ) -> Result<String> {
        let mut out = String::with_capacity(template.len());
        for part in parts(rule, template)? {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Program(program) => match self.first(rule, program, input, item)? {
                    Value::String(s) => out.push_str(&s),
                    Value::Null => {}
                    other => out.push_str(&other.to_string()),
                },
            }
        }
        Ok(out)
    }
}

/// One part of a template: text that stays as it is, or a program between `{{` and `}}`.
enum Part<'a> {
    Text(&'a str),
    Program(&'a str),
}

/// The parts of `template`, in order.
fn parts<'a>(rule: &str, template: &'a str) -> Result<Vec<Part<'a>>> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push(Part::Text(&rest[..start]));
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::Jq {
                rule: rule.to_string(),
                message: format!("the template has a `{{{{` with no `}}}}`: {template}"),
            });
        };
        out.push(Part::Program(after[..end].trim()));
        rest = &after[end + 2..];
    }
    out.push(Part::Text(rest));
    Ok(out)
}

fn load_errors<P>(errs: &jaq_core::load::Errors<&str, P>) -> String {
    errs.iter()
        .flat_map(|(_, e)| match e {
            jaq_core::load::Error::Lex(v) => v
                .iter()
                .map(|(expected, at)| format!("expected {} near {at:?}", expected.as_str()))
                .collect::<Vec<_>>(),
            jaq_core::load::Error::Parse(v) => v
                .iter()
                .map(|(expected, at)| format!("expected {} near {at:?}", expected.as_str()))
                .collect(),
            jaq_core::load::Error::Io(v) => v.iter().map(|(p, e)| format!("{p}: {e}")).collect(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn compile_errors<P>(errs: &jaq_core::compile::Errors<&str, P>) -> String {
    errs.iter()
        .flat_map(|(_, e)| {
            e.iter().map(|(text, undefined)| {
                let what = match undefined {
                    jaq_core::compile::Undefined::Mod => "module",
                    jaq_core::compile::Undefined::Var => "variable",
                    jaq_core::compile::Undefined::Label => "label",
                    jaq_core::compile::Undefined::Filter(_) => "filter",
                    _ => "name",
                };
                format!("undefined {what} {text}")
            })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// A JSON value as jaq holds it. jaq reads any `serde` document, so the value is the
/// deserializer of its own conversion.
fn to_val(v: &Value) -> Val {
    use serde::Deserialize;
    Val::deserialize(v.clone()).unwrap_or(Val::Null)
}

/// A jaq value as JSON. jaq prints a value as JSON text, and that text is the JSON. A number
/// that JSON cannot hold, such as `nan`, comes back as `null`.
fn to_json(v: &Val) -> Value {
    serde_json::from_str(&v.to_string()).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semver_numbers_reads_a_version_as_strata_does() {
        let jq = Jq::new();
        let numbers = |input: Value| {
            jq.first("t", "semver_numbers", &input, None)
                .expect("the filter runs")
        };
        assert_eq!(numbers(json!("4.10.3-rc.1")), json!([4, 10, 3]));
        assert_eq!(numbers(json!("v4.10")), json!([4, 10]));
        assert_eq!(numbers(json!("1.0.1+2")), json!([1, 0, 1]));
        assert_eq!(numbers(json!("LTS__24.x")), Value::Null);
        assert_eq!(numbers(json!("")), Value::Null);
        // a value that is no text is an error of the program
        let err = jq.run("t", "semver_numbers", &json!(4), None).unwrap_err();
        assert!(err.to_string().contains("the program failed"), "{err}");
    }

    #[test]
    fn count_of_writes_a_count_and_its_noun_as_the_reports_do() {
        let jq = Jq::new();
        let words = |count: Value, noun: &str| {
            jq.first("t", &format!("count_of(\"{noun}\")"), &count, None)
                .expect("the filter runs")
        };
        assert_eq!(words(json!(0), "key pair"), json!("no key pairs"));
        assert_eq!(words(json!(1), "key pair"), json!("1 key pair"));
        assert_eq!(words(json!(2), "key pair"), json!("2 key pairs"));
        assert_eq!(words(json!(12), "CPU"), json!("12 CPUs"));
        assert_eq!(
            words(json!(0), "free m5.large slot"),
            json!("no free m5.large slots")
        );
        // the count comes from a program
        let program = r#"[.a, .b] | map(length) as [$x, $y] | "\($x | count_of("host")), \($y | count_of("pool"))""#;
        let out = jq
            .first("t", program, &json!({"a": [1], "b": [1, 2]}), None)
            .unwrap();
        assert_eq!(out, json!("1 host, 2 pools"));
        // a count that is not a whole number of zero or more is an error of the program
        for bad in [json!(-1), json!(1.5), json!("2"), Value::Null] {
            let err = jq.run("t", "count_of(\"host\")", &bad, None).unwrap_err();
            assert!(
                err.to_string().contains("the program failed"),
                "{bad}: {err}"
            );
        }
        let err = jq.run("t", "count_of(1)", &json!(2), None).unwrap_err();
        assert!(err.to_string().contains("the program failed"), "{err}");
    }

    #[test]
    fn a_program_runs_on_a_document() {
        let jq = Jq::new();
        let world = json!({"entity": {"spec": {"machinepoolconfig": [
            {"poolConfig": {"name": "cp", "size": 3, "isControlPlane": true}},
            {"poolConfig": {"name": "worker", "size": 4, "isControlPlane": false}},
        ]}}});
        let out = jq
            .run(
                "t",
                r#"[.entity.spec.machinepoolconfig[] | select(.poolConfig.size % 3 != 0) | "pool \(.poolConfig.name) has size \(.poolConfig.size)"]"#,
                &world,
                None,
            )
            .unwrap();
        assert_eq!(out, vec![json!(["pool worker has size 4"])]);
    }

    #[test]
    fn a_program_is_compiled_one_time() {
        let jq = Jq::new();
        jq.run("t", ".a", &json!({"a": 1}), None).unwrap();
        jq.run("t", ".a", &json!({"a": 2}), None).unwrap();
        assert_eq!(jq.compiled.borrow().len(), 1);
    }

    #[test]
    fn a_program_keeps_one_form_without_the_item_and_one_with_it() {
        let jq = Jq::new();
        let item = json!({"uid": "h1"});
        jq.run("t", ".a", &json!({"a": 1}), None).unwrap();
        assert_eq!(
            jq.compiled.borrow()[".a"].each_ref().map(Option::is_some),
            [true, false]
        );
        jq.run("t", ".a", &json!({"a": 1}), Some(&item)).unwrap();
        assert_eq!(jq.compiled.borrow().len(), 1, "one key for the program");
        assert_eq!(
            jq.compiled.borrow()[".a"].each_ref().map(Option::is_some),
            [true, true]
        );
    }

    #[test]
    fn a_bad_program_names_the_rule() {
        let err = Jq::new()
            .run("eks-one-zone", ".a |", &json!({}), None)
            .unwrap_err();
        assert!(err.to_string().starts_with("rule eks-one-zone:"), "{err}");
        let err = Jq::new()
            .run("r", "nosuchfilter", &json!({}), None)
            .unwrap_err();
        assert!(err.to_string().contains("undefined filter"), "{err}");
    }

    #[test]
    fn a_program_reads_the_item_only_when_a_check_gives_one() {
        let jq = Jq::new();
        let world = json!({"hosts": ["h1", "h2"]});
        let item = json!({"uid": "h2"});
        let out = jq
            .run(
                "t",
                "[.hosts[] | select(. == $item.uid)]",
                &world,
                Some(&item),
            )
            .unwrap();
        assert_eq!(out, vec![json!(["h2"])]);
        assert_eq!(
            jq.render("t", "/v1/edgehosts/{{ $item.uid }}", &world, Some(&item))
                .unwrap(),
            "/v1/edgehosts/h2"
        );
        let err = jq
            .run("t", "$item.uid", &world, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("undefined variable $item"), "{err}");
    }

    #[test]
    fn a_template_fills_each_program() {
        let jq = Jq::new();
        let world = json!({"cloud": "gke", "entity": {"spec": {"n": 3}}});
        assert_eq!(
            jq.render(
                "t",
                "/v1/spectroclusters/{{ .cloud }}/validate",
                &world,
                None
            )
            .unwrap(),
            "/v1/spectroclusters/gke/validate"
        );
        assert_eq!(
            jq.render("t", "size {{.entity.spec.n}} {{ .missing }}!", &world, None)
                .unwrap(),
            "size 3 !"
        );
        assert!(jq.render("t", "{{ .cloud", &world, None).is_err());
    }

    #[test]
    fn regex_and_std_are_there() {
        let jq = Jq::new();
        let out = jq
            .run(
                "t",
                r#"[.m | test("AccessDenied")]"#,
                &json!({"m": "AccessDenied: x"}),
                None,
            )
            .unwrap();
        assert_eq!(out, vec![json!([true])]);
        let out = jq
            .run(
                "t",
                r#".ip | split("/")[0]"#,
                &json!({"ip": "10.0.0.0/16"}),
                None,
            )
            .unwrap();
        assert_eq!(out, vec![json!("10.0.0.0")]);
    }

    #[test]
    fn numbers_come_back_as_numbers() {
        let jq = Jq::new();
        let out = jq
            .run("t", ".n * 2, .f + 0.5", &json!({"n": 21, "f": 1.0}), None)
            .unwrap();
        assert_eq!(out, vec![json!(42), json!(1.5)]);
    }
}
