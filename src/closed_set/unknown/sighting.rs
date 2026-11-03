//! What a line of the log and an entry of the report both state about an unknown value, and the
//! checks of it.
//!
//! A [`Sighting`] is the four facts of one unknown value: the version of strata that read it, the
//! set, the value, and the route. The log line [`Line`](super::log::Line) and the report entry
//! [`Entry`](super::report::Entry) each hold one, so a field, a check, and an order have one
//! definition. A check refuses a value that no closed set of Palette holds, so the document cannot
//! carry free text.

use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::TOOL_VERSION;
use crate::reference::version;
use crate::text::cut_with;

/// The most characters of a value that the note and the log show. A value of a closed set is a
/// short word, so a longer text is not a value of a set.
const LONGEST_VALUE: usize = 64;

/// The most characters of the version of strata and of the name of a set.
const LONGEST_FIELD: usize = 64;

/// The most characters of the template of a route.
const LONGEST_ROUTE: usize = 200;

/// The mark at the end of a value that [`shown`] cut.
const CUT_MARK: &str = "...";

/// The start of each route that strata reads.
const ROUTE_PREFIX: &str = "/v1/";

/// The fewest characters of a uid. A uid of Palette has 24 hexadecimal digits, and a UUID has more.
const UID_LENGTH: usize = 24;

/// One unknown value that Palette gave: where it came from, and what it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Sighting {
    /// The version of strata that read the value.
    pub strata: String,
    /// The name of the closed set, as `strata schema` names its definition.
    pub set: String,
    /// The text of the value.
    pub value: String,
    /// The template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`.
    pub route: String,
}

impl Sighting {
    /// The sighting of the value `value` of the set `set`, which the route `route` gave, as this
    /// version of strata reads it.
    pub(crate) fn new(set: &str, value: &str, route: &str) -> Sighting {
        Sighting {
            strata: TOOL_VERSION.to_string(),
            set: set.to_string(),
            value: value.to_string(),
            route: route.to_string(),
        }
    }

    /// What is wrong with the sighting, or `None`. Each field is a short word, and the route is the
    /// template of a route. Strata never writes a sighting with a fault.
    pub(super) fn fault(&self) -> Option<String> {
        word_fault("strata", &self.strata, is_version_char)
            .or_else(|| word_fault("set", &self.set, |c| c.is_ascii_alphanumeric()))
            .or_else(|| value_fault(&self.value))
            .or_else(|| route_fault(&self.route))
            .or_else(|| {
                (self.route.chars().count() > LONGEST_ROUTE)
                    .then(|| format!("the route has more than {LONGEST_ROUTE} characters"))
            })
    }

    /// The order of two sightings: the set, the value, the route, and the version of strata. Two
    /// sightings that are equal are next to each other.
    pub(super) fn order(&self, other: &Sighting) -> Ordering {
        (&self.set, &self.value, &self.route)
            .cmp(&(&other.set, &other.value, &other.route))
            .then_with(|| version::compare(&self.strata, &other.strata))
            // two texts can have one version order, such as `1.0.1+2` and `1.0.1`. The text decides
            // that tie only, so that equal sightings stay next to each other
            .then_with(|| self.strata.cmp(&other.strata))
    }
}

/// True for a character that a value can hold: printable ASCII with no space, from `!` to `~`. So a
/// value holds no control character and no format character, such as U+200B or U+202E. It holds no
/// letter that looks like another. It holds no space that a code span of markdown drops at its
/// ends. Two issues of two values cannot look equal.
fn is_value_char(c: char) -> bool {
    c.is_ascii_graphic()
}

/// What is wrong with `value` as the value of a closed set, or `None`. A value is one short word: it
/// has [`LONGEST_VALUE`] characters or fewer, and holds only the characters of [`is_value_char`].
pub(super) fn value_fault(value: &str) -> Option<String> {
    if value.is_empty()
        || value.chars().count() > LONGEST_VALUE
        || !value.chars().all(is_value_char)
    {
        return Some(format!(
            "the value is not a short word of {LONGEST_VALUE} printable ASCII characters or fewer, with no space"
        ));
    }
    None
}

/// `value` as the note and the log show it: [`LONGEST_VALUE`] characters at most, with a `?` in
/// place of each character that [`value_fault`] refuses. A longer value ends with `...`. So a value
/// cannot make the terminal do more than show text, and every value that strata writes passes the
/// check.
pub(super) fn shown(value: &str) -> String {
    let printable: String = value
        .chars()
        .map(|c| if is_value_char(c) { c } else { '?' })
        .collect();
    cut_with(&printable, LONGEST_VALUE, CUT_MARK)
}

/// What is wrong with `route` as the template of a route, or `None`. A template starts with
/// [`ROUTE_PREFIX`], holds only the characters of [`is_route_char`], and none of its words is a uid.
pub(super) fn route_fault(route: &str) -> Option<String> {
    if !route.starts_with(ROUTE_PREFIX)
        || !route.chars().all(is_route_char)
        || route.split('/').any(looks_like_uid)
    {
        return Some("the route is not a template of a route".to_string());
    }
    None
}

/// True for a character of a route template: a letter or a digit of ASCII, or one of `/`, `_`, `-`,
/// `.`, `{`, and `}`. So a template holds no space, no quote, no mark of markdown, and no line
/// break.
fn is_route_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '{' | '}')
}

/// True when `segment` is as long as a uid and holds only hexadecimal digits and hyphens. A word
/// of a route holds other letters, and a placeholder such as `{uid}` holds braces.
fn looks_like_uid(segment: &str) -> bool {
    segment.len() >= UID_LENGTH && segment.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// What is wrong with the text `text` of the field `name`, or `None`. A field of this kind is not
/// empty, has [`LONGEST_FIELD`] characters or fewer, and holds only characters that `allowed` names.
fn word_fault(name: &str, text: &str, allowed: fn(char) -> bool) -> Option<String> {
    (text.is_empty() || text.chars().count() > LONGEST_FIELD || !text.chars().all(allowed))
        .then(|| format!("the {name} is not a short word of {LONGEST_FIELD} characters or fewer"))
}

/// True for a character of a version: a letter, a digit, or one of `.`, `-`, `+`, and `_`.
fn is_version_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_')
}
