//! The facts about each pack of the world that strata derives. A rule reads them, and does not
//! write a rule of strata again in a program.

use serde_json::{Map, Value, json};
use strata::closed_set::ClosedSet;
use strata::json::{arr, opt_str, str_at, strings, text, value_at};
use strata::model::cloud::supports;
use strata::model::pack_state::PackState;

/// The `packFacts` of `world`: for the uid of each pack of `packs`, an object with
/// `supportsCloud`, `state`, and `retired`.
///
/// `supportsCloud` is true when the cloud types of the pack include the cloud type of the engine
/// (`routeCloud`), by the rule of Palette for the create of a profile. It is `null` when the world
/// has no engine. `state` is the `system_state` of the pack, or empty text for a pack that has
/// none. `retired` is true for a state that a profile cannot pin: `disabled` and `deleted`.
pub(super) fn pack_facts(world: &Value) -> Value {
    let route_cloud = opt_str(world, "/routeCloud");
    let facts: Map<String, Value> = value_at(world, "/packs")
        .as_object()
        .into_iter()
        .flatten()
        .map(|(uid, pack)| {
            let state = state_of(pack, uid);
            let retired = PackState::parse(&state).is_some_and(PackState::is_retired);
            let supported = route_cloud.map(|cloud| supports(&strings(pack, "/cloudTypes"), cloud));
            (
                uid.clone(),
                json!({"supportsCloud": supported, "state": state, "retired": retired}),
            )
        })
        .collect();
    Value::Object(facts)
}

/// The `system_state` of the pack `uid` in its document, or empty text.
fn state_of(pack: &Value, uid: &str) -> String {
    arr(pack, "/packValues")
        .iter()
        .find(|entry| str_at(entry, "/packUid") == uid)
        .map(|entry| text(entry, "/annotations/system_state"))
        .unwrap_or_default()
}
