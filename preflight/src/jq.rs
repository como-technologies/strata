//! The jq programs of the rules, run by jaq. A program compiles one time and runs on many
//! inputs. A template such as `/v1/spectroclusters/{{ .cloud }}/validate` fills each `{{ jq }}`
//! with the first output of that program, as text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use jaq_core::load::{Arena, File, Loader};
use jaq_core::{Compiler, Ctx, Vars, data};
use jaq_json::Val;
use serde_json::Value;

use crate::error::{Error, Result};

type Filter = jaq_core::Filter<data::JustLut<Val>>;

/// Compiles and runs jq programs, and keeps each compiled program for the next input.
#[derive(Default)]
pub struct Jq {
    compiled: RefCell<HashMap<String, Rc<Filter>>>,
}

impl Jq {
    /// A new engine with nothing compiled.
    pub fn new() -> Self {
        Self::default()
    }

    fn compile(&self, rule: &str, program: &str) -> Result<Rc<Filter>> {
        if let Some(f) = self.compiled.borrow().get(program) {
            return Ok(f.clone());
        }
        let defs = jaq_core::defs()
            .chain(jaq_std::defs())
            .chain(jaq_json::defs());
        let funs = jaq_core::funs()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs());
        let loader = Loader::new(defs);
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
        let filter = Compiler::default()
            .with_funs(funs)
            .compile(modules)
            .map_err(|errs| Error::Jq {
                rule: rule.to_string(),
                message: format!("the program does not compile: {}", compile_errors(&errs)),
            })?;
        let filter = Rc::new(filter);
        self.compiled
            .borrow_mut()
            .insert(program.to_string(), filter.clone());
        Ok(filter)
    }

    /// Run `program` on `input` and give every output, in order.
    pub fn run(&self, rule: &str, program: &str, input: &Value) -> Result<Vec<Value>> {
        let filter = self.compile(rule, program)?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
        let input = to_val(input);
        let mut out = Vec::new();
        for item in filter.id.run((ctx, input)) {
            match item.map_err(|e| e.get_err()) {
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
    pub fn first(&self, rule: &str, program: &str, input: &Value) -> Result<Value> {
        Ok(self
            .run(rule, program, input)?
            .into_iter()
            .next()
            .unwrap_or(Value::Null))
    }

    /// Fill each `{{ jq }}` of `template` with the first output of that program on `input`, as
    /// text: a string stays as it is, and another value is its JSON.
    pub fn render(&self, rule: &str, template: &str, input: &Value) -> Result<String> {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                return Err(Error::Jq {
                    rule: rule.to_string(),
                    message: format!("the template has a `{{{{` with no `}}}}`: {template}"),
                });
            };
            let program = after[..end].trim();
            let value = self.first(rule, program, input)?;
            match value {
                Value::String(s) => out.push_str(&s),
                Value::Null => {}
                other => out.push_str(&other.to_string()),
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        Ok(out)
    }
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
            )
            .unwrap();
        assert_eq!(out, vec![json!(["pool worker has size 4"])]);
    }

    #[test]
    fn a_program_is_compiled_one_time() {
        let jq = Jq::new();
        jq.run("t", ".a", &json!({"a": 1})).unwrap();
        jq.run("t", ".a", &json!({"a": 2})).unwrap();
        assert_eq!(jq.compiled.borrow().len(), 1);
    }

    #[test]
    fn a_bad_program_names_the_rule() {
        let err = Jq::new()
            .run("eks-one-zone", ".a |", &json!({}))
            .unwrap_err();
        assert!(err.to_string().starts_with("rule eks-one-zone:"), "{err}");
        let err = Jq::new().run("r", "nosuchfilter", &json!({})).unwrap_err();
        assert!(err.to_string().contains("undefined filter"), "{err}");
    }

    #[test]
    fn a_template_fills_each_program() {
        let jq = Jq::new();
        let world = json!({"cloud": "gke", "entity": {"spec": {"n": 3}}});
        assert_eq!(
            jq.render("t", "/v1/spectroclusters/{{ .cloud }}/validate", &world)
                .unwrap(),
            "/v1/spectroclusters/gke/validate"
        );
        assert_eq!(
            jq.render("t", "size {{.entity.spec.n}} {{ .missing }}!", &world)
                .unwrap(),
            "size 3 !"
        );
        assert!(jq.render("t", "{{ .cloud", &world).is_err());
    }

    #[test]
    fn regex_and_std_are_there() {
        let jq = Jq::new();
        let out = jq
            .run(
                "t",
                r#"[.m | test("AccessDenied")]"#,
                &json!({"m": "AccessDenied: x"}),
            )
            .unwrap();
        assert_eq!(out, vec![json!([true])]);
        let out = jq
            .run("t", r#".ip | split("/")[0]"#, &json!({"ip": "10.0.0.0/16"}))
            .unwrap();
        assert_eq!(out, vec![json!("10.0.0.0")]);
    }

    #[test]
    fn numbers_come_back_as_numbers() {
        let jq = Jq::new();
        let out = jq
            .run("t", ".n * 2, .f + 0.5", &json!({"n": 21, "f": 1.0}))
            .unwrap();
        assert_eq!(out, vec![json!(42), json!(1.5)]);
    }
}
