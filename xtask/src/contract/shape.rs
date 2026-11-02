//! The shape of a document: the type of the value at a field of a fact, and the text of each value
//! at a field. The contract run compares the shape of an answer of Palette with the shape of the
//! documents of the stand-in. It never compares the values of two tenants.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;

/// The JSON type of a value, as the contract run tells the types apart.
///
/// A field that is missing and a field that is `null` are both [`JsonType::Absent`]. Palette writes
/// `null` for a list with no entry, and a missing field for the same thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JsonType {
    /// The field is missing, or `null`.
    Absent,
    /// `true` or `false`.
    Boolean,
    /// An integer or a decimal number.
    Number,
    /// Text.
    String,
    /// A list.
    Array,
    /// An object.
    Object,
}

impl JsonType {
    /// The name of the type, as the document writes it.
    pub fn name(self) -> String {
        // serde writes a unit variant as its name, so the one table of the names is the derive; a
        // unit variant always serializes, so the empty text is never the answer
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// The type of the value at `value`; `None` is a missing field.
    fn of(value: Option<&Value>) -> JsonType {
        match value {
            None | Some(Value::Null) => JsonType::Absent,
            Some(Value::Bool(_)) => JsonType::Boolean,
            Some(Value::Number(_)) => JsonType::Number,
            Some(Value::String(_)) => JsonType::String,
            Some(Value::Array(_)) => JsonType::Array,
            Some(Value::Object(_)) => JsonType::Object,
        }
    }
}

/// One step of a field path.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    /// The value of a key of an object.
    Key(String),
    /// Each entry of a list: `[]`.
    Each,
    /// Each value of an object: `*`.
    Values,
}

/// A field of a fact, parsed: `spec.clusterProfileTemplates[].packs`. The list of the facts
/// describes the syntax: `[]` marks a list, and `*` marks the values of an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    steps: Vec<Step>,
}

impl Path {
    /// Parse the field `text`.
    pub fn parse(text: &str) -> Path {
        let mut steps = Vec::new();
        for part in text.split('.') {
            let mut name = part;
            let mut lists = 0;
            while let Some(shorter) = name.strip_suffix("[]") {
                name = shorter;
                lists += 1;
            }
            match name {
                "" => {}
                "*" => steps.push(Step::Values),
                key => steps.push(Step::Key(key.to_string())),
            }
            steps.extend(std::iter::repeat_n(Step::Each, lists));
        }
        Path { steps }
    }

    /// The type of the value at each place that this path reaches in `doc`.
    ///
    /// A list with no entry reaches no place. A key that is missing reaches one place, with the
    /// type [`JsonType::Absent`].
    pub fn types(&self, doc: &Value) -> BTreeSet<JsonType> {
        let mut found = BTreeSet::new();
        for root in self.roots(doc) {
            self.walk(Some(root), &self.steps, &mut |value| {
                found.insert(JsonType::of(value));
            });
        }
        found
    }

    /// The text of each value that this path reaches in `doc`. A value that is no text, or has no
    /// characters, is omitted.
    pub fn texts(&self, doc: &Value) -> BTreeSet<String> {
        self.collect(doc, |value| match value {
            Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
    }

    /// The text of each text and each number that this path reaches in `doc`. A number with no
    /// fraction is its integer, so `10.0` is `10`.
    pub fn scalars(&self, doc: &Value) -> BTreeSet<String> {
        self.collect(doc, |value| match value {
            Some(Value::Number(number)) => Some(match number.as_f64() {
                Some(float) if float.fract() == 0.0 => format!("{float:.0}"),
                _ => number.to_string(),
            }),
            Some(Value::String(text)) if !text.is_empty() => Some(text.clone()),
            _ => None,
        })
    }

    /// The set of what `pick` makes of the value at each place that this path reaches in `doc`.
    fn collect(
        &self,
        doc: &Value,
        pick: impl Fn(Option<&Value>) -> Option<String>,
    ) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for root in self.roots(doc) {
            self.walk(Some(root), &self.steps, &mut |value| {
                found.extend(pick(value));
            });
        }
        found
    }

    /// The values that the path starts from. The answer of some routes is a list. The facts write its
    /// fields without a leading `[]`, so a path that starts with a key reads each entry of the list.
    fn roots<'a>(&self, doc: &'a Value) -> Vec<&'a Value> {
        match (doc, self.steps.first()) {
            (Value::Array(entries), Some(Step::Key(_) | Step::Values)) => entries.iter().collect(),
            _ => vec![doc],
        }
    }

    /// Call `at` with the value at each place that `rest` reaches from `value`.
    fn walk<'a>(
        &self,
        value: Option<&'a Value>,
        rest: &[Step],
        at: &mut impl FnMut(Option<&'a Value>),
    ) {
        let Some((step, tail)) = rest.split_first() else {
            return at(value);
        };
        match (step, value) {
            (Step::Key(key), Some(Value::Object(map))) => self.walk(map.get(key), tail, at),
            (Step::Key(_), _) => at(None),
            (Step::Each, Some(Value::Array(entries))) => entries
                .iter()
                .for_each(|entry| self.walk(Some(entry), tail, at)),
            (Step::Values, Some(Value::Object(map))) => map
                .values()
                .for_each(|entry| self.walk(Some(entry), tail, at)),
            (Step::Each | Step::Values, _) => {}
        }
    }
}
