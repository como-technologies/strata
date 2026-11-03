//! The closed sets of strata, and the lists of their values in the facts.
//!
//! A fact lists the values of a closed set in `reads.values`, and names the type of the set in
//! `reads.sets`. [`check`] compares the list with the values of the type, so a value that one has
//! and the other lacks is a fault. The type is the one copy of the values.
//!
//! Strata notes a value of a closed set that this release does not know at each read with
//! `opt_set`. The contract run counts the same value when a GET fact lists the set. [`reads`]
//! names each read of strata, so that [`check`] makes sure that each one has its list in a fact.
//! Without this tie, a new value shows in the note of a user, and the contract run finds none.

use std::sync::LazyLock;

use regex::Regex;
use strata::closed_set::ClosedSet;
use strata::model::cluster_state::ClusterState;
use strata::model::oci_provider::OciProvider;
use strata::model::pack_state::PackState;
use strata::model::pack_type::PackType;
use strata::model::profile_type::ProfileType;
use strata::model::scope::ProfileScope;

use super::sources::Sources;

/// The values of a closed set that Palette never gives: a word of strata. A fact lists the set
/// without them, each as the set and the value. A listing of packs never gives the state `absent`:
/// `behind` gives it to a version that the registry no longer lists. A cluster profile never has the
/// type `app` or `pack`: a record of an app profile and a record of a pack version have them.
const STRATA_ONLY: &[(&str, &str)] = &[
    ("PackState", "absent"),
    ("ProfileType", "app"),
    ("ProfileType", "pack"),
];

/// A closed set of strata: the name of its type and each value, as the documents name it.
#[derive(Clone, Debug)]
pub struct Set {
    /// The name of the type, such as `ProfileType`.
    pub name: String,
    /// Each value of the set.
    pub values: Vec<String>,
}

impl Set {
    fn of<T: ClosedSet>() -> Set {
        let path = std::any::type_name::<T>();
        Set {
            name: path.rsplit("::").next().unwrap_or(path).to_string(),
            values: T::ALL.iter().map(|v| v.as_str().to_string()).collect(),
        }
    }
}

/// The closed sets that a fact can name in `reads.sets`.
pub fn closed_sets() -> Vec<Set> {
    vec![
        Set::of::<ClusterState>(),
        Set::of::<PackState>(),
        Set::of::<PackType>(),
        Set::of::<OciProvider>(),
        Set::of::<ProfileScope>(),
        Set::of::<ProfileType>(),
    ]
}

/// One read of strata that notes an unknown value, and the field of the fact that lists its set.
#[derive(Clone, Debug)]
pub struct SetRead {
    /// The name of the set, such as `ProfileType`.
    pub set: String,
    /// The id of the GET fact that lists the set.
    pub fact: &'static str,
    /// The field of the fact that holds the value, as the key of `reads.values`.
    pub field: &'static str,
}

impl SetRead {
    fn of<T: ClosedSet>(fact: &'static str, field: &'static str) -> SetRead {
        SetRead {
            set: Set::of::<T>().name,
            fact,
            field,
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

/// Each list of values that names its set is the set: the type has no value that the list lacks,
/// except a word of strata, and the list has no value that the type lacks. A name that is no type
/// of [`closed_sets`], or a name at a field that has no list, is a fault too.
fn named(sources: &Sources) -> Vec<String> {
    let mut bad = Vec::new();
    for fact in &sources.facts {
        for (field, name) in &fact.reads.sets {
            let id = &fact.id;
            let Some(list) = fact.reads.values.get(field) else {
                bad.push(format!(
                    "fact `{id}`: `reads.sets` names {name} at `{field}`, and `reads.values` has no list there"
                ));
                continue;
            };
            let Some(set) = sources.closed_sets.iter().find(|s| &s.name == name) else {
                bad.push(format!(
                    "fact `{id}`: `reads.sets` names {name} at `{field}`, and it is no closed set that `closed_sets()` of `xtask/src/facts/sets.rs` holds"
                ));
                continue;
            };
            bad.extend(agree(id, field, list, set));
        }
    }
    bad
}

/// One message for each value that the `list` of the fact `id` at `field` and the `set` do not
/// share. A word of strata is the set alone: [`STRATA_ONLY`].
fn agree(id: &str, field: &str, list: &[String], set: &Set) -> Vec<String> {
    let name = &set.name;
    let strata_only = |value: &str| STRATA_ONLY.contains(&(name.as_str(), value));
    let extra = list
        .iter()
        .filter(|value| !set.values.contains(value))
        .map(|value| {
            format!(
                "fact `{id}`: it lists `{value}` at `{field}`, and the set {name} has no such value: add it to the type, or take it from the list"
            )
        });
    let lacking = set
        .values
        .iter()
        .filter(|value| !list.contains(value) && !strata_only(value))
        .map(|value| {
            format!(
                "fact `{id}`: the set {name} has `{value}`, and the list at `{field}` lacks it: add it to the list, or name it in `STRATA_ONLY` when only strata gives it"
            )
        });
    extra.chain(lacking).collect()
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
    bad.extend(named(sources));
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
    if !found.reads.values.contains_key(field) {
        return vec![format!(
            "fact `{fact}`: strata notes a value of {set} at `{field}`, so `reads.values` must list it, or the contract run counts no new value"
        )];
    }
    // `named` compares the list with the set, once the fact names it
    if found.reads.sets.get(field) != Some(set) {
        return vec![format!(
            "fact `{fact}`: strata notes a value of {set} at `{field}`, so `reads.sets` must name {set} there"
        )];
    }
    Vec::new()
}
