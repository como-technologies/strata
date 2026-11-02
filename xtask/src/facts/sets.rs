//! The reads of strata that note a value of a closed set that this release does not know.
//!
//! Strata notes such a value at each read with `opt_set`. The contract run counts the same value
//! when a GET fact lists the set in `reads.values`. [`reads`] names each read of strata, so that
//! [`check`] makes sure that each one has its list in a fact, and that no list names a value that
//! strata does not know. Without this tie, a new value shows in the note of a user, and the
//! contract run finds none.

use std::sync::LazyLock;

use regex::Regex;
use strata::closed_set::ClosedSet;
use strata::model::oci_provider::OciProvider;
use strata::model::pack_state::PackState;
use strata::model::profile_type::ProfileType;

use super::model::Fact;
use super::sources::Sources;

/// One read of strata that notes an unknown value, and the field of the fact that lists its set.
#[derive(Clone, Debug)]
pub struct SetRead {
    /// The name of the set, such as `ProfileType`.
    pub set: String,
    /// The id of the GET fact that lists the set.
    pub fact: &'static str,
    /// The field of the fact that holds the value, as the key of `reads.values`.
    pub field: &'static str,
    /// Each value of the set, as the documents name it.
    pub values: Vec<String>,
}

impl SetRead {
    fn of<T: ClosedSet>(fact: &'static str, field: &'static str) -> SetRead {
        let path = std::any::type_name::<T>();
        SetRead {
            set: path.rsplit("::").next().unwrap_or(path).to_string(),
            fact,
            field,
            values: T::ALL.iter().map(|v| v.as_str().to_string()).collect(),
        }
    }
}

/// The reads of strata that note an unknown value, one for each call of `opt_set` in the code.
/// The call that reads the provider of a pack ref serves a profile and a cluster. Its entry names
/// the fact of the profile.
pub fn reads() -> Vec<SetRead> {
    vec![
        SetRead::of::<PackState>("pack-listing", "items[].spec.annotations.system_state"),
        SetRead::of::<OciProvider>("oci-registry-summary", "items[].spec.providerType"),
        SetRead::of::<ProfileType>("profile-document", "spec.published.type"),
        SetRead::of::<OciProvider>(
            "profile-document",
            "spec.published.packs[].annotations.ociRegistryProviderType",
        ),
    ]
}

/// The calls of `opt_set` in `text`, the code of strata with no comment and no test.
fn calls(text: &str) -> usize {
    static CALL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\bopt_set(::<[^>]*>)?\(").expect("a pattern"));
    CALL.find_iter(text).count()
}

/// Each read of strata that notes an unknown value has a list of values in a GET fact, the list
/// names only values that strata knows, and the code holds no read that the table lacks.
pub fn check(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    let sites: usize = sources.cloud.code.values().map(|text| calls(text)).sum();
    if sites != sources.set_reads.len() {
        bad.push(format!(
            "the code of strata reads {sites} sets with `opt_set`, and `reads()` of `xtask/src/facts/sets.rs` names {}: add each read to the table, and its values to a GET fact",
            sources.set_reads.len()
        ));
    }
    for read in &sources.set_reads {
        bad.extend(listed(sources, read));
    }
    bad
}

/// The faults of the list of one read.
fn listed(sources: &Sources, read: &SetRead) -> Vec<String> {
    let (set, fact, field) = (&read.set, read.fact, read.field);
    let Some(found) = sources.facts.iter().find(|f| f.id == fact) else {
        return vec![format!("the read of {set} at `{field}`: no fact `{fact}`")];
    };
    if found.get_path().is_none() {
        return vec![format!(
            "fact `{fact}`: it lists {set} at `{field}`, so it must be a GET request that the contract run sends"
        )];
    }
    let Some(values) = found.reads.values.get(field) else {
        return vec![format!(
            "fact `{fact}`: strata notes a value of {set} at `{field}`, so `reads.values` must list it, or the contract run counts no new value"
        )];
    };
    unknown_to_strata(found, read, values)
}

/// One message for each value of `values` that the set of `read` does not name.
fn unknown_to_strata(fact: &Fact, read: &SetRead, values: &[String]) -> Vec<String> {
    values
        .iter()
        .filter(|value| !read.values.contains(value))
        .map(|value| {
            format!(
                "fact `{}`: it lists `{value}` at `{}`, and the set {} of strata has no such value, so strata notes it and the contract run does not",
                fact.id, read.field, read.set
            )
        })
        .collect()
}
