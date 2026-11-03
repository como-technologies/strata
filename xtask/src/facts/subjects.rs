//! Where the contract run finds the value of a placeholder of a request.
//!
//! A request of a fact has a placeholder for the object, such as `{uid}` in
//! `GET /v1/spectroclusters/{uid}`, or `{cloudAccountUid}` in the query of
//! `GET /v1/clouds/aws/regions?cloudAccountUid={cloudAccountUid}`. The tenant of the run is not the
//! tenant of the stand-in. So the run finds an object in the tenant: the first entry of the answer
//! of another fact. That answer is the listing of the objects, or the document of an object that
//! holds the value.
//!
//! The table here says which fact gives which value to which request. The check of the list reads
//! it too, so a source that serves no request, or two sources for one placeholder, are faults.

use std::collections::BTreeSet;

use serde_json::Value;
use strata::api::RawCluster;
use strata::json::{arr, str_at};

use super::model::{Fact, Kind, expand, names};

/// How the run reads the value of a placeholder from the answer of a fact.
pub(crate) enum Read {
    /// The text at a JSON pointer. The first entry of a list is the subject.
    Pointer(&'static str),
    /// The text at `value` in each entry of the array at `list`. The run tries the entries in
    /// order, until the rest of the request finds its values, and the entry holds what `holds` asks.
    Each {
        list: &'static str,
        value: &'static str,
        holds: Option<Hold>,
    },
    /// The cloud type of a cluster document. Strata reads it with one rule, so the run asks the
    /// route of the cloud config as strata does.
    CloudType,
}

impl Read {
    /// The values in `document`, in order. A value that the document lacks is empty text.
    pub(crate) fn values<'a>(&self, document: &'a Value) -> Vec<&'a str> {
        match self {
            Read::Pointer(pointer) => vec![str_at(document, pointer)],
            Read::Each { list, value, .. } => arr(document, list)
                .iter()
                .map(|entry| str_at(entry, value))
                .collect(),
            Read::CloudType => vec![RawCluster::cloud_type(document)],
        }
    }

    /// What the document lacks when [`Read::values`] gives no value but empty text.
    pub(crate) fn lacks(&self) -> String {
        match self {
            Read::Pointer(pointer) => format!("nothing at {pointer}"),
            Read::Each { list, value, .. } => format!("nothing at {value} in each entry of {list}"),
            Read::CloudType => "no cloud type".to_string(),
        }
    }
}

/// What an entry of a list must hold before the run takes it. The answer of a fact has a value at
/// `pointer`. The run finds the answer with the values bound so far. The regions need a VPC, so the
/// facts of one region all take the first region that holds one.
#[derive(Clone, Copy)]
pub(crate) struct Hold {
    /// The name of the placeholder that the value serves. The reason of a run that finds none names
    /// it.
    pub(crate) param: &'static str,
    /// The fact that gives the value.
    pub(crate) fact: &'static str,
    /// The JSON pointer of the value in the answer of the fact.
    pub(crate) pointer: &'static str,
}

impl Hold {
    /// How the run reads the value in the answer of the fact.
    pub(crate) const fn read(&self) -> Read {
        Read::Pointer(self.pointer)
    }
}

/// Where the value of a placeholder comes from.
pub(crate) enum Origin {
    /// The answer of the fact `fact`, read with `read`.
    Fact { fact: &'static str, read: Read },
    /// The route takes no value of this kind. The run omits the pair from the query, as the rule
    /// of strata-preflight omits it.
    Omitted,
}

/// A source of the value of one placeholder, for the requests that start from one route.
pub(crate) struct Source {
    /// The route of the request, through the placeholder when it is in the path. A request is
    /// served when its path is this route, or goes on from it by whole segments. So the source
    /// for `{uid}` of `/v1/packs/{name}/registries/{uid}` serves no request of `/v1/packs/{uid}`,
    /// where `{uid}` names a pack.
    pub(crate) route: &'static str,
    /// The name of the placeholder.
    pub(crate) param: &'static str,
    /// Where the value comes from.
    pub(crate) origin: Origin,
}

/// The id of the fact that lists the clusters of the tenant.
pub(crate) const CLUSTER_LISTING: &str = "cluster-listing";

/// The id of the fact that gives the document of a cluster.
const CLUSTER_DOCUMENT: &str = "cluster-document";

/// The first VPC of a region. The security groups need it. The facts of one region need it too, so
/// the run takes for all of them the first region that holds a VPC.
const FIRST_VPC: Hold = Hold {
    param: "vpcId",
    fact: "validate-vpcs",
    pointer: "/vpcs/0/vpcId",
};

/// The regions of the account, in order. A region serves when it holds a VPC.
const REGIONS: Read = Read::Each {
    list: "/regions",
    value: "/name",
    holds: Some(FIRST_VPC),
};

/// The shortest way to the subject of each request that the run sends. The first entry of a
/// listing is the subject, so the placeholders of one request come from one object. The regions are
/// the exception: the run tries each region in order, until one holds a VPC.
pub(crate) const SOURCES: &[Source] = &[
    Source {
        route: "/v1/spectroclusters/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: CLUSTER_LISTING,
            read: Read::Pointer("/0/metadata/uid"),
        },
    },
    Source {
        route: "/v1/notifications/spectrocluster/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: CLUSTER_LISTING,
            read: Read::Pointer("/0/metadata/uid"),
        },
    },
    Source {
        route: "/v1/cloudconfigs/{cloudType}",
        param: "cloudType",
        origin: Origin::Fact {
            fact: CLUSTER_DOCUMENT,
            read: Read::CloudType,
        },
    },
    Source {
        route: "/v1/cloudconfigs/{cloudType}/{configUid}",
        param: "configUid",
        origin: Origin::Fact {
            fact: CLUSTER_DOCUMENT,
            read: Read::Pointer("/spec/cloudConfigRef/uid"),
        },
    },
    Source {
        route: "/v1/filters/tag/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: "tag-filter-listing",
            read: Read::Pointer("/items/0/metadata/uid"),
        },
    },
    // the AWS list routes ask for the account, the region, and the VPC. They take the first
    // account, and the first region of that account that lets the rest of the request find its
    // values. The routes of GCP and Azure have no source yet.
    Source {
        route: "/v1/clouds/aws/regions",
        param: "cloudAccountUid",
        origin: Origin::Fact {
            fact: "cloud-account-listing",
            read: Read::Pointer("/items/0/metadata/uid"),
        },
    },
    // only the route of Azure takes a subscription
    Source {
        route: "/v1/clouds/aws/regions",
        param: "subscriptionId",
        origin: Origin::Omitted,
    },
    Source {
        route: "/v1/clouds/aws/regions/{region}",
        param: "region",
        origin: Origin::Fact {
            fact: "validate-aws-regions",
            read: REGIONS,
        },
    },
    Source {
        route: "/v1/clouds/aws/securitygroups",
        param: "cloudAccountUid",
        origin: Origin::Fact {
            fact: "cloud-account-listing",
            read: Read::Pointer("/items/0/metadata/uid"),
        },
    },
    Source {
        route: "/v1/clouds/aws/securitygroups",
        param: "region",
        origin: Origin::Fact {
            fact: "validate-aws-regions",
            read: REGIONS,
        },
    },
    Source {
        route: "/v1/clouds/aws/securitygroups",
        param: "vpcId",
        origin: Origin::Fact {
            fact: FIRST_VPC.fact,
            read: FIRST_VPC.read(),
        },
    },
    Source {
        route: "/v1/clustergroups/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: "cluster-group-listing",
            read: Read::Pointer("/items/0/uid"),
        },
    },
    Source {
        route: "/v1/appProfiles/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: "app-profile-listing",
            read: Read::Pointer("/appProfiles/0/metadata/uid"),
        },
    },
    Source {
        route: "/v1/appProfiles/{uid}/tiers/{tierUid}",
        param: "tierUid",
        origin: Origin::Fact {
            fact: "app-profile-tiers",
            read: Read::Pointer("/spec/appTiers/0/metadata/uid"),
        },
    },
    Source {
        route: "/v1/appProfiles/{uid}/tiers/{tierUid}/manifests/{manifestUid}",
        param: "manifestUid",
        origin: Origin::Fact {
            fact: "app-profile-tiers",
            read: Read::Pointer("/spec/appTiers/0/spec/manifests/0/uid"),
        },
    },
    Source {
        route: "/v1/packs/{name}",
        param: "name",
        origin: Origin::Fact {
            fact: "pack-listing",
            read: Read::Pointer("/items/0/spec/name"),
        },
    },
    // the registry of the first pack: `{uid}` names a registry here, and a pack in
    // `GET /v1/packs/{uid}`, which has no source
    Source {
        route: "/v1/packs/{name}/registries/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: "pack-listing",
            read: Read::Pointer("/items/0/spec/registryUid"),
        },
    },
    Source {
        route: "/v1/clusterprofiles/{uid}",
        param: "uid",
        origin: Origin::Fact {
            fact: "profile-listing",
            read: Read::Pointer("/items/0/metadata/uid"),
        },
    },
];

/// True when the source for the route `route` serves the request `template`. The path of the
/// request is the route, or it continues from the route by whole segments.
fn serves(route: &str, template: &str) -> bool {
    let path = template.split('?').next().unwrap_or(template);
    path == route
        || path
            .strip_prefix(route)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The sources of `table` for the placeholder `param` of the request `template`. A list of one is
/// right.
fn sources_of<'t>(
    table: &'t [Source],
    template: &str,
    param: &str,
) -> impl Iterator<Item = &'t Source> {
    table
        .iter()
        .filter(move |s| s.param == param && serves(s.route, template))
}

/// The source for the placeholder `param` of the request `template`, when one exists.
pub(crate) fn source(template: &str, param: &str) -> Option<&'static Source> {
    sources_of(SOURCES, template, param).next()
}

/// The placeholders of `template` that no [`Source`] gives a value for.
pub(crate) fn unsourced(template: &str) -> Vec<String> {
    names(template)
        .into_iter()
        .filter(|param| source(template, param).is_none())
        .map(str::to_string)
        .collect()
}

/// The path and the query of the GET request of the fact with the id `id`.
pub(crate) fn request_of<'a>(facts: &'a [Fact], id: &str) -> Option<&'a str> {
    facts.iter().find(|f| f.id == id)?.get_path()
}

/// The GET requests of the facts of the kind `request`, each choice of the alternatives apart, with
/// the id of the fact.
fn requests(facts: &[Fact]) -> Vec<(&str, String)> {
    facts
        .iter()
        .filter(|fact| fact.kind == Kind::Request)
        .filter_map(|fact| Some((fact.id.as_str(), fact.get_path()?)))
        .flat_map(|(id, path)| expand(path).into_iter().map(move |(_, text)| (id, text)))
        .collect()
}

/// A placeholder of a request has one source at most. Two sources read one placeholder in two
/// ways, as `{uid}` is a registry in `/v1/packs/{name}/registries/{uid}` and a pack in
/// `/v1/packs/{uid}`.
pub(crate) fn check(facts: &[Fact]) -> Vec<String> {
    ambiguous(SOURCES, facts)
}

/// The placeholders of the requests of `facts` that a source of `table` reads in a wrong way.
/// There are two cases: more than one source reads the placeholder, or a source stops before a
/// placeholder in the path of the request. The second is a source for a prefix, and not for the
/// request. It serves each request that continues from the prefix, and `{uid}` can mean another
/// object in each of them.
fn ambiguous(table: &[Source], facts: &[Fact]) -> Vec<String> {
    let mut bad = Vec::new();
    for (id, template) in requests(facts) {
        let path = template.split('?').next().unwrap_or(&template);
        for param in names(&template) {
            let sources: Vec<&Source> = sources_of(table, &template, param).collect();
            if sources.len() > 1 {
                bad.push(format!(
                    "fact `{id}`: {} sources read `{{{param}}}` of the request `{template}`: keep one",
                    sources.len()
                ));
            }
            let held = format!("{{{param}}}");
            for source in sources
                .iter()
                .filter(|s| path.contains(&held) && !s.route.contains(&held))
            {
                bad.push(format!(
                    "fact `{id}`: the source of `{{{param}}}` names the route `{}`, which stops before the placeholder of the request `{template}`: name the route through it",
                    source.route
                ));
            }
        }
    }
    bad
}

/// The table of the sources against the list of the repository. A fault is a source that serves no
/// request of the list, or a source that reads a fact which is no GET request. The route of a
/// request changes, and the source then goes stale.
pub(crate) fn check_table(facts: &[Fact]) -> Vec<String> {
    stale(SOURCES, facts)
}

/// The sources of `table` that serve no request of `facts`, or read a fact that is no GET request.
fn stale(table: &[Source], facts: &[Fact]) -> Vec<String> {
    let mut bad = Vec::new();
    let requests = requests(facts);
    for source in table {
        let serves_one = requests
            .iter()
            .any(|(_, t)| serves(source.route, t) && names(t).contains(&source.param));
        if !serves_one {
            bad.push(format!(
                "the source for `{{{}}}` of `{}` serves no request of the list",
                source.param, source.route
            ));
        }
        if let Origin::Fact { fact, read } = &source.origin {
            let held = match read {
                Read::Each {
                    holds: Some(hold), ..
                } => Some(hold.fact),
                _ => None,
            };
            for named in std::iter::once(*fact).chain(held) {
                if request_of(facts, named).is_none() {
                    bad.push(format!(
                        "the source for `{{{}}}` of `{}` reads the fact `{named}`, and it is no GET request of the list",
                        source.param, source.route
                    ));
                }
            }
        }
    }
    bad
}

/// The most facts that one subject chains. The security groups take the VPC, which takes the
/// region, which takes the account.
pub(crate) const DEPTH: usize = 3;

/// True when the request `template` takes a value from a cluster of the tenant. A source reads the
/// listing of the clusters, or it reads a fact that needs one in turn.
fn needs_cluster(facts: &[Fact], template: &str, depth: usize) -> bool {
    names(template)
        .into_iter()
        .filter_map(|param| match &source(template, param)?.origin {
            Origin::Fact { fact, .. } => Some(*fact),
            Origin::Omitted => None,
        })
        .any(|fact| {
            fact == CLUSTER_LISTING
                || (depth < DEPTH
                    && request_of(facts, fact)
                        .is_some_and(|request| needs_cluster(facts, request, depth + 1)))
        })
}

/// The ids of the facts that the run must check in a window with a cluster. Each fact is a GET
/// request that reads a document, that the stand-in serves, and that takes its subject from a
/// cluster of the tenant. A fact that names the codes of an object that can be absent is out. A
/// cluster can lack such an object, and the run then skips the fact with a reason.
pub(crate) fn floor(facts: &[Fact]) -> BTreeSet<&str> {
    facts
        .iter()
        .filter(|fact| fact.kind == Kind::Request)
        .filter(|fact| fact.reads.reads_a_document() && !fact.stand_in.is_empty())
        .filter(|fact| fact.absent_codes.is_empty())
        .filter(|fact| {
            fact.get_path().is_some_and(|path| {
                expand(path)
                    .iter()
                    .any(|(_, text)| needs_cluster(facts, text, 0))
            })
        })
        .map(|fact| fact.id.as_str())
        .collect()
}

#[cfg(test)]
mod tests;
