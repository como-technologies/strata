//! The subject of a request: the object of the tenant that the run asks about.
//!
//! A request of a fact has a placeholder for the object, such as `{uid}` in
//! `GET /v1/spectroclusters/{uid}`. The tenant of the run is not the tenant of the stand-in. So the
//! run finds an object in the tenant: the first entry of the answer of another fact. The table of
//! [`crate::facts::subjects`] says which fact gives which value. This module sends the requests
//! that find the values, and keeps each answer.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::Value;
use strata::api::PaletteApi;
use strata::error::Error;
use strata::model::validation::PaletteRefusal;
use strata::text::query_value;

use crate::facts::subjects::{DEPTH, Origin, Read, request_of, source, unsourced};
use crate::facts::{Fact, Scope, fill, names};

/// The status of the answer of Palette to a read of the VPCs of a region that AWS refuses.
const VPCS_READ_STATUS: u16 = 500;

/// The code of the answer of Palette to a read of the VPCs of a region that AWS refuses. Palette
/// gives the same status and a code of its own to a region with no metadata of its instance types,
/// `CloudInstanceMetadataNotFound`. That answer ends the entry of the one fact that asks for the
/// types, and the search of a region does not read it.
const VPCS_READ_ERROR: &str = "AwsVpcsReadError";

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The status of the answer, when Palette answered.
    pub status: Option<u16>,
    /// The code in the body of the answer, when the body is JSON with a `code` of text or of a
    /// number.
    pub code: Option<String>,
    /// What went wrong, in one line.
    pub message: String,
}

impl Failure {
    fn of(error: &Error) -> Failure {
        let (status, code) = match error {
            Error::Api { status, body, .. } => (
                Some(*status),
                PaletteRefusal::from_error_body(body).map(|r| r.code),
            ),
            _ => (None, None),
        };
        Failure {
            status,
            code,
            message: error.to_string(),
        }
    }

    /// The code of `absent` that Palette answered with HTTP 404: the tenant has no such object. A
    /// 404 with another code gives none, and so does another status with one of these codes.
    pub fn absent<'a>(&self, absent: &'a [String]) -> Option<&'a str> {
        let code = self.code.as_deref().filter(|_| self.status == Some(404))?;
        absent.iter().map(String::as_str).find(|a| *a == code)
    }

    /// True when AWS refused the read of the VPCs of a region: HTTP 500 with the code
    /// [`VPCS_READ_ERROR`]. The list of the regions holds only the regions that answer a dry run with
    /// the key of the account. A region that is off is not in it. AWS can still refuse the read for
    /// a region of the list, and another region can serve.
    fn refused_region(&self) -> bool {
        self.status == Some(VPCS_READ_STATUS) && self.code.as_deref() == Some(VPCS_READ_ERROR)
    }
}

/// The answer of a request: the document, or the failure.
pub type Answer = Rc<Result<Value, Failure>>;

/// A tenant that the run asks. It keeps each answer, so the listing that finds a subject is read
/// once for all the facts that use it.
///
/// It holds two clients of the tenant. `client` names the project of the environment, if any.
/// `tenant` names no project, for the facts of the scope [`Scope::Tenant`]: Palette refuses a
/// request that names a project on such a route.
pub struct Probe<'a, A> {
    client: &'a A,
    tenant: &'a A,
    answers: RefCell<BTreeMap<(Scope, String), Answer>>,
}

impl<'a, A: PaletteApi> Probe<'a, A> {
    /// A probe of the tenant that `client` and `tenant` talk to.
    pub fn new(client: &'a A, tenant: &'a A) -> Self {
        Probe {
            client,
            tenant,
            answers: RefCell::new(BTreeMap::new()),
        }
    }

    /// The answer to `GET path`, sent in `scope`.
    pub fn get(&self, scope: Scope, path: &str) -> Answer {
        let key = (scope, path.to_string());
        if let Some(known) = self.answers.borrow().get(&key) {
            return Rc::clone(known);
        }
        let api = match scope {
            Scope::Client => self.client,
            Scope::Tenant => self.tenant,
        };
        let answer = Rc::new(api.get_json(path).map_err(|e| Failure::of(&e)));
        self.answers.borrow_mut().insert(key, Rc::clone(&answer));
        answer
    }
}

/// What the run found for the subject of a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Subject {
    /// The path of the request, with a value for each placeholder.
    Path(String),
    /// The tenant has no such object, or the run has no way to find one. The text says why.
    Missing(String),
    /// A request that finds the object failed.
    Failed(Failure),
}

/// The value of each placeholder of one request, by name. A name with no value is a pair that the
/// route does not take, and the run omits it from the query.
type Bindings = BTreeMap<String, Option<String>>;

/// The path of `template` for a subject of the tenant: a value for each placeholder.
///
/// A placeholder with more than one value, such as the region, takes the first value that holds
/// what its source asks. The rest of the request must find its values for that value too. A region
/// holds a VPC, so each request of a region takes the first region that lists one. The run tries
/// the next region when one holds none, and when AWS refuses the read of its VPCs.
pub fn find<A: PaletteApi>(probe: &Probe<A>, facts: &[Fact], template: &str) -> Subject {
    fill_template(probe, facts, template, &Bindings::new(), 0)
}

/// The path of `template` when `bound` already holds the values of some placeholders.
fn fill_template<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    template: &str,
    bound: &Bindings,
    depth: usize,
) -> Subject {
    // the run sends no request for a path that has a placeholder with no source
    if let Some(param) = unsourced(template).first() {
        return Subject::Missing(no_source(param));
    }
    let params = names(template);
    let mut bindings = bound.clone();
    match bind(probe, facts, template, &params, &mut bindings, depth) {
        Ok(()) => Subject::Path(render(template, &bindings)),
        Err(subject) => subject,
    }
}

/// Why the run has no subject for a placeholder that no source gives a value for.
fn no_source(param: &str) -> String {
    format!("the run has no way to find a value for `{{{param}}}`")
}

/// The scope of the fact with the id `id`. A list that has no such fact gives the scope of the
/// client: [`request_of`] finds no request for it, so the run sends none.
fn scope_of(facts: &[Fact], id: &str) -> Scope {
    facts
        .iter()
        .find(|f| f.id == id)
        .map_or_else(Scope::default, |f| f.scope)
}

/// `template` with the value of each placeholder in `bindings`. The pair of a query for a name with
/// no value goes out, and so does the `?` when no pair stays.
fn render(template: &str, bindings: &Bindings) -> String {
    let (path, query) = template.split_once('?').unwrap_or((template, ""));
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let value = pair.split_once('=').map_or("", |(_, value)| value);
            !matches!(names(value).as_slice(), [name] if matches!(bindings.get(*name), Some(None)))
        })
        .collect();
    let text = if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    };
    let filled = fill(&text, |name| match bindings.get(name) {
        Some(Some(value)) => Ok(value.clone()),
        _ => Err(name.to_string()),
    });
    // each name has a value: `bind` gave one to all
    filled.unwrap_or(text)
}

/// Give a value to each placeholder of `params`, in order, and keep the first choice that lets the
/// rest find theirs. The error is why the first choice failed.
fn bind<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    template: &str,
    params: &[&str],
    bindings: &mut Bindings,
    depth: usize,
) -> Result<(), Subject> {
    let Some((param, rest)) = params.split_first() else {
        return Ok(());
    };
    if bindings.contains_key(*param) {
        return bind(probe, facts, template, rest, bindings, depth);
    }
    let found = candidates(probe, facts, template, param, bindings, depth)?;
    let tried = found.len();
    let mut first_failure = None;
    for candidate in found {
        bindings.insert((*param).to_string(), candidate);
        let served = holds(probe, facts, template, param, bindings, depth)
            .and_then(|()| bind(probe, facts, template, rest, bindings, depth));
        match served {
            Ok(()) => return Ok(()),
            // the next value can still serve: a region with no VPC is no end of the search. Neither
            // is a region that AWS refuses
            Err(subject) if next_value_can_serve(&subject) => {
                first_failure.get_or_insert(subject);
                bindings.remove(*param);
            }
            Err(other) => return Err(other),
        }
    }
    let tried_note = format!("the run tried {tried} values for `{{{param}}}`");
    Err(match first_failure {
        // the first failure says why; the count says that the run tried the others
        Some(Subject::Missing(why)) if tried > 1 => {
            Subject::Missing(format!("{why}; {tried_note}"))
        }
        Some(Subject::Failed(failure)) if tried > 1 => Subject::Failed(Failure {
            message: format!("{}; {tried_note}", failure.message),
            ..failure
        }),
        Some(subject) => subject,
        None => Subject::Missing(no_source(param)),
    })
}

/// True when the next value of a placeholder can still give a subject, after `subject` stopped the
/// first. This is the case when the tenant has no object. It is also the case when AWS refuses the
/// read of the VPCs of a region: see [`Failure::refused_region`]. Palette gives no 4xx for that
/// read, so any other failure says something about the route and not about the value. A 401, a 429,
/// another status, or a failure of the network ends the search.
fn next_value_can_serve(subject: &Subject) -> bool {
    match subject {
        Subject::Missing(_) => true,
        Subject::Failed(failure) => failure.refused_region(),
        Subject::Path(_) => false,
    }
}

/// Ok when the value just bound to `param` holds what its source asks of an entry. A source with no
/// such ask serves each value. See [`Hold`](crate::facts::subjects::Hold).
fn holds<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    template: &str,
    param: &str,
    bindings: &Bindings,
    depth: usize,
) -> Result<(), Subject> {
    let Some(Origin::Fact {
        read: Read::Each {
            holds: Some(hold), ..
        },
        ..
    }) = source(template, param).map(|s| &s.origin)
    else {
        return Ok(());
    };
    listed(
        probe,
        facts,
        hold.fact,
        &hold.read(),
        hold.param,
        bindings,
        depth,
    )
    .map(drop)
}

/// The values that the tenant offers for the placeholder `param` of `template`, in order, or why
/// the run has none. `None` is a pair that the route does not take.
fn candidates<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    template: &str,
    param: &str,
    bindings: &Bindings,
    depth: usize,
) -> Result<Vec<Option<String>>, Subject> {
    let Some(source) = source(template, param) else {
        return Err(Subject::Missing(no_source(param)));
    };
    let Origin::Fact { fact, read } = &source.origin else {
        return Ok(vec![None]);
    };
    Ok(listed(probe, facts, fact, read, param, bindings, depth)?
        .into_iter()
        .map(Some)
        .collect())
}

/// The values of `param` in the answer of the fact `fact`, read with `read`, in order. The request
/// of the fact takes the values in `bindings`.
fn listed<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    fact: &str,
    read: &Read,
    param: &str,
    bindings: &Bindings,
    depth: usize,
) -> Result<Vec<String>, Subject> {
    let missing = |why: String| Subject::Missing(why);
    let Some(request) = request_of(facts, fact).filter(|_| depth < DEPTH) else {
        return Err(missing(format!(
            "the fact `{fact}` is not a request that finds a value for `{{{param}}}`"
        )));
    };
    // the request that lists the objects holds the values that are bound already: the VPCs of
    // the region that this request names
    let path = match fill_template(probe, facts, request, bindings, depth + 1) {
        Subject::Path(path) => path,
        other => return Err(other),
    };
    // the listing that holds the subject is sent in its own scope, as the run sends the fact
    let answer = probe.get(scope_of(facts, fact), &path);
    let document = match answer.as_ref() {
        Ok(document) => document,
        Err(failure) => {
            return Err(Subject::Failed(Failure {
                message: format!("GET {path}: {}", failure.message),
                ..failure.clone()
            }));
        }
    };
    let found: Vec<&str> = read
        .values(document)
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect();
    if found.is_empty() {
        return Err(missing(format!(
            "the tenant has no object for `{{{param}}}`: the answer to GET {path} holds {}",
            read.lacks()
        )));
    }
    // the library escapes a value of a route as this call does, so the run sends what strata sends
    Ok(found.into_iter().map(query_value).collect())
}
