//! The subject of a request: the object of the tenant that the run asks about.
//!
//! A request of a fact has a placeholder for the object, such as `{uid}` in
//! `GET /v1/spectroclusters/{uid}`, or `{cloudAccountUid}` in the query of
//! `GET /v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}`. The tenant of the run is not the
//! tenant of the stand-in. So the run finds an object in the tenant: the first entry of the answer
//! of another fact. That answer is the listing of the objects, or the document of an object that
//! holds the value.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::Value;
use strata::api::PaletteApi;
use strata::error::Error;
use strata::json::str_at;

use crate::facts::{Fact, Scope};

/// Where the value of a placeholder comes from.
pub(super) struct Source {
    /// The start of the path that has the placeholder, before the query. With `param`, it names the
    /// placeholder.
    pub(super) prefix: &'static str,
    /// The name of the placeholder.
    pub(super) param: &'static str,
    /// The id of the fact whose answer holds the value.
    pub(super) fact: &'static str,
    /// The pointer of the value in that answer. The first entry of a list is the subject.
    pub(super) pointer: &'static str,
}

/// The shortest way to the subject of each request that the run sends. The first entry of a
/// listing is the subject, so the placeholders of one request come from one object.
pub(super) const SOURCES: &[Source] = &[
    Source {
        prefix: "/v1/spectroclusters/",
        param: "uid",
        fact: "cluster-listing",
        pointer: "/0/metadata/uid",
    },
    Source {
        prefix: "/v1/notifications/spectrocluster/",
        param: "uid",
        fact: "cluster-listing",
        pointer: "/0/metadata/uid",
    },
    Source {
        prefix: "/v1/cloudconfigs/",
        param: "cloudType",
        fact: "cluster-document",
        pointer: "/spec/cloudType",
    },
    Source {
        prefix: "/v1/cloudconfigs/",
        param: "configUid",
        fact: "cluster-document",
        pointer: "/spec/cloudConfigRef/uid",
    },
    Source {
        prefix: "/v1/filters/tag/",
        param: "uid",
        fact: "tag-filter-listing",
        pointer: "/items/0/metadata/uid",
    },
    // the three AWS list routes ask for the account, the region, and the VPC. They take the first
    // account, the first region of that account, and the first VPC of that region.
    Source {
        prefix: "/v1/clouds/aws/regions",
        param: "cloudAccountUid",
        fact: "cloud-account-listing",
        pointer: "/items/0/metadata/uid",
    },
    Source {
        prefix: "/v1/clouds/aws/regions/",
        param: "region",
        fact: "validate-aws-regions",
        pointer: "/regions/0/name",
    },
    Source {
        prefix: "/v1/clouds/aws/securitygroups",
        param: "cloudAccountUid",
        fact: "cloud-account-listing",
        pointer: "/items/0/metadata/uid",
    },
    Source {
        prefix: "/v1/clouds/aws/securitygroups",
        param: "region",
        fact: "validate-aws-regions",
        pointer: "/regions/0/name",
    },
    Source {
        prefix: "/v1/clouds/aws/securitygroups",
        param: "vpcId",
        fact: "validate-vpcs",
        pointer: "/vpcs/0/vpcId",
    },
    Source {
        prefix: "/v1/clustergroups/",
        param: "uid",
        fact: "cluster-group-listing",
        pointer: "/items/0/uid",
    },
    Source {
        prefix: "/v1/appProfiles/",
        param: "uid",
        fact: "app-profile-listing",
        pointer: "/appProfiles/0/metadata/uid",
    },
    Source {
        prefix: "/v1/appProfiles/",
        param: "tierUid",
        fact: "app-profile-tiers",
        pointer: "/spec/appTiers/0/metadata/uid",
    },
    Source {
        prefix: "/v1/appProfiles/",
        param: "manifestUid",
        fact: "app-profile-tiers",
        pointer: "/spec/appTiers/0/spec/manifests/0/uid",
    },
    Source {
        prefix: "/v1/packs/",
        param: "name",
        fact: "pack-listing",
        pointer: "/items/0/spec/name",
    },
    Source {
        prefix: "/v1/packs/",
        param: "uid",
        fact: "pack-listing",
        pointer: "/items/0/spec/registryUid",
    },
    Source {
        prefix: "/v1/clusterprofiles/",
        param: "uid",
        fact: "profile-listing",
        pointer: "/items/0/metadata/uid",
    },
];

/// The most facts that one subject chains. The security groups take the VPC, which takes the
/// region, which takes the account.
const DEPTH: usize = 3;

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The status of the answer, when Palette answered.
    pub status: Option<u16>,
    /// What went wrong, in one line.
    pub message: String,
}

impl Failure {
    fn of(error: &Error) -> Failure {
        let status = match error {
            Error::Api { status, .. } => Some(*status),
            _ => None,
        };
        Failure {
            status,
            message: error.to_string(),
        }
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

/// The path of `template` for a subject of the tenant: a value for each placeholder.
pub fn find<A: PaletteApi>(probe: &Probe<A>, facts: &[Fact], template: &str) -> Subject {
    fill(probe, facts, template, 0)
}

/// The placeholders of `template` that no [`Source`] gives a value for.
pub(super) fn unsourced(template: &str) -> Vec<String> {
    placeholders(template)
        .filter(|param| source(template, param).is_none())
        .map(str::to_string)
        .collect()
}

/// Why the run has no subject for a placeholder that no [`Source`] gives a value for.
fn no_source(param: &str) -> String {
    format!("the run has no way to find a value for `{{{param}}}`")
}

/// The names of the placeholders of `template`, in the path and in the query.
fn placeholders(template: &str) -> impl Iterator<Item = &str> {
    template
        .split('{')
        .skip(1)
        .filter_map(|rest| Some(rest.split_once('}')?.0))
}

fn source(template: &str, param: &str) -> Option<&'static Source> {
    SOURCES
        .iter()
        .find(|s| template.starts_with(s.prefix) && s.param == param)
}

/// The fact with the id `id`, when it has a request of the method GET.
pub(super) fn request_of<'a>(facts: &'a [Fact], id: &str) -> Option<&'a str> {
    facts.iter().find(|f| f.id == id)?.get_path()
}

/// The scope of the fact with the id `id`. A list that has no such fact gives the scope of the
/// client: [`request_of`] finds no request for it, so the run sends none.
fn scope_of(facts: &[Fact], id: &str) -> Scope {
    facts
        .iter()
        .find(|f| f.id == id)
        .map_or_else(Scope::default, |f| f.scope)
}

fn fill<A: PaletteApi>(probe: &Probe<A>, facts: &[Fact], template: &str, depth: usize) -> Subject {
    // no request goes out for a path that has a placeholder with no source
    if let Some(param) = unsourced(template).first() {
        return Subject::Missing(no_source(param));
    }
    let mut path = String::new();
    let mut rest = template;
    // a placeholder has no `{` or `}` inside, so each one ends at the next `}`
    while let Some((before, after)) = rest.split_once('{') {
        let Some((param, tail)) = after.split_once('}') else {
            break;
        };
        path.push_str(before);
        match value(probe, facts, template, param, depth) {
            Ok(text) => path.push_str(&text),
            Err(subject) => return subject,
        }
        rest = tail;
    }
    path.push_str(rest);
    Subject::Path(path)
}

/// The value of the placeholder `param` of `template`, or why the run has none.
fn value<A: PaletteApi>(
    probe: &Probe<A>,
    facts: &[Fact],
    template: &str,
    param: &str,
    depth: usize,
) -> Result<String, Subject> {
    let missing = |why: String| Subject::Missing(why);
    let Some(source) = source(template, param) else {
        return Err(missing(no_source(param)));
    };
    let Some(request) = request_of(facts, source.fact).filter(|_| depth < DEPTH) else {
        return Err(missing(format!(
            "the fact `{}` is not a request that finds a value for `{{{param}}}`",
            source.fact
        )));
    };
    let path = match fill(probe, facts, request, depth + 1) {
        Subject::Path(path) => path,
        other => return Err(other),
    };
    // the listing that holds the subject is sent in its own scope, as the run sends the fact
    let answer = probe.get(scope_of(facts, source.fact), &path);
    let document = match answer.as_ref() {
        Ok(document) => document,
        Err(failure) => {
            return Err(Subject::Failed(Failure {
                status: failure.status,
                message: format!("GET {path}: {}", failure.message),
            }));
        }
    };
    let found = str_at(document, source.pointer);
    if found.is_empty() {
        return Err(missing(format!(
            "the tenant has no object for `{{{param}}}`: the answer to GET {path} holds nothing at {}",
            source.pointer
        )));
    }
    if !found
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._~:".contains(c))
    {
        return Err(missing(format!(
            "the value for `{{{param}}}` has a character that a path cannot hold"
        )));
    }
    Ok(found.to_string())
}
