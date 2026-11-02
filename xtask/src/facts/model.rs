//! The fact: one thing that Palette does and that strata depends on, as the list states it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use strata::api::Method;
use strata::closed_set::ClosedSet;

use crate::files::files_with_ext;

/// The schema id that each file of the list names, and that `xtask facts` prints.
pub const SCHEMA: &str = "strata-facts/1";

/// The directory of the list, from the root of the repository. One file holds the facts of one
/// part of the book.
pub const DIRECTORY: &str = "xtask/facts";

/// What a fact states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A request that strata sends: the fact names the request and what strata reads from the
    /// answer.
    Request,
    /// What Palette does after a change or for a given input, with no request of its own.
    Behavior,
    /// A check that strata makes because no route of Palette answers it: the fact names the
    /// error code that Palette gives at the create.
    Gap,
    /// A call of the tool of a cloud that a cloud-tier rule of strata-preflight depends on, or a
    /// table of cloud facts that such a rule holds. The contract run sends the call in the lab
    /// account.
    Cloud,
}

/// The scope in which the contract run sends the request of a fact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// The scope of the client: the project that it names, or the tenant when it names none. Most
    /// routes take it.
    #[default]
    Client,
    /// The tenant. The route is not an operation of the project scope, so Palette refuses a
    /// request that names a project with HTTP 403 and the code `ProjectUidIsNotEmpty`. The run
    /// sends such a request with no project, as strata does.
    Tenant,
}

/// One fact of the list.
///
/// A script reads the same fields from the JSON that `xtask facts` prints.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    /// The name of the fact: lower case words with dashes, and one in the whole list.
    pub id: String,
    /// What the fact states.
    pub kind: Kind,
    /// The method and the path of the request, such as `GET /v1/clusterprofiles/{uid}`, with the
    /// query that strata sends when the route needs one: `GET /v1/filters/metadata?filterType=tag`.
    /// A value of the query that varies is `{name}`, as in the path. Only a fact of the kind
    /// `request` has one.
    pub request: Option<String>,
    /// The scope in which the contract run sends the request. A route of the tenant says so, as
    /// the saved tag filters do.
    #[serde(default)]
    pub scope: Scope,
    /// The error codes that mean the tenant has no such object, such as `ResourceNotFound`. The
    /// route gives the record of an object that a cluster can lack. The contract run skips the
    /// fact when Palette answers HTTP 404 with one of these codes. A 404 with another code is a
    /// difference. Only a fact with a GET request has them, and a page of the fact states each one.
    #[serde(default)]
    pub absent_codes: Vec<String>,
    /// What strata reads from the answer or depends on. A fact of the kind `cloud` that no call
    /// proves needs no field here.
    #[serde(default)]
    pub reads: Reads,
    /// The routes of the stand-in that imitate the request, as paths with `{name}` for a segment
    /// that varies. The list is empty when the stand-in serves no document for the fact.
    #[serde(default)]
    pub stand_in: Vec<String>,
    /// The live checks that prove the fact, each as `script.sh:check`.
    #[serde(default)]
    pub live: Vec<String>,
    /// Why no live check proves the fact. The reason is in the list when `live` is empty, and
    /// not otherwise.
    pub unchecked: Option<String>,
    /// The pages of the book that state the fact, each as a path under `docs/src/` and an
    /// optional `#anchor`.
    pub pages: Vec<String>,
    /// The files of strata that use the fact, from the root of the repository.
    #[serde(default)]
    pub code: Vec<String>,
    /// The rules of strata-preflight that use the fact.
    #[serde(default)]
    pub rules: Vec<String>,
    /// The read-only call of a fact of the kind `cloud`, as the words of the command, with
    /// `{name}` for a value of the lab account: `[aws, ec2, describe-subnets, --region, "{region}"]`.
    /// The contract run sends it.
    #[serde(default)]
    pub call: Vec<String>,
    /// The table of cloud facts that a rule holds as text, when the fact proves one.
    pub table: Option<Table>,
    /// The route of Palette that answers a part of the call of a fact of the kind `cloud`, and the
    /// fields of the call that the route lacks.
    pub palette: Option<PaletteRoute>,
}

/// A route of Palette beside the call of a cloud. The route lists a part of what a rule reads from
/// the call. The rest of the call stays in the cloud tier, because the route lacks it.
///
/// A fact of the kind `request` states a route and its query one time, so the route names that
/// fact. A route that no request fact states is written out. A route has exactly one of the two.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteRoute {
    /// The id of the fact of the kind `request` that states the route and its query.
    pub fact: Option<String>,
    /// The method and the path of a route that no fact states, as the book writes it.
    pub route: Option<String>,
    /// The fields of `reads` that the route does not list. Each one is why the rule stays in the
    /// cloud tier.
    pub lacks: Vec<String>,
}

/// A table of cloud facts that a rule of strata-preflight holds as text, such as the zone ids that
/// EKS refuses. The answer of the call of the fact must list each value, so a table that the cloud
/// no longer knows is a difference.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// The date on which the table was last proven, as `YYYY-MM-DD`.
    pub as_of: String,
    /// The values of the table. The answer of the call lists each one at the field of `reads`.
    pub holds: Vec<String>,
    /// True when each account sees its own values, as it sees its own zone ids. The answer of an
    /// account can lack a value of the table, and that is no difference: the account cannot check
    /// the value.
    #[serde(default)]
    pub per_account: bool,
}

/// The method and the rest of `request`, such as `GET` and `/v1/clusterprofiles/{uid}?filter=1`.
/// None when `request` has no space, or starts with a text that is no method.
pub fn split_request(request: &str) -> Option<(Method, &str)> {
    let (method, rest) = request.split_once(' ')?;
    Some((Method::parse(method)?, rest))
}

/// One part of a text of the list: a request, a route of the stand-in, or a word of a call.
///
/// A group in braces is a placeholder. The run finds the value of `{uid}` in the tenant, or in the
/// lab. `{aws,gcp,azure}` lists the alternatives of a request, and the run sends one request for
/// each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Text that stands as it is.
    Text(&'a str),
    /// `{name}`: a value that the run finds. A name is a letter, then letters, digits, and `_`.
    Name(&'a str),
    /// `{a,b}`: the names of the alternatives.
    Alternatives(Vec<&'a str>),
    /// A group in braces that is neither of the two, or a `{` with no `}`. It is the text from the
    /// `{` to the `}`. The run reads no value for it, so the check of the list refuses it.
    Unread(&'a str),
}

/// True when `name` is the name of a placeholder: a letter, then letters, digits, and `_`.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The parts of `text`, in order. This is the one reader of the placeholders of the list: the run
/// of a request, the call of a cloud, and the check of the list all read through it.
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            pieces.push(Piece::Text(&rest[..open]));
        }
        let group = &rest[open..];
        let Some(close) = group.find('}') else {
            pieces.push(Piece::Unread(group));
            return pieces;
        };
        let inner = &group[1..close];
        let names: Vec<&str> = inner.split(',').collect();
        pieces.push(if is_name(inner) {
            Piece::Name(inner)
        } else if names.len() > 1 && names.iter().all(|n| is_name(n)) {
            Piece::Alternatives(names)
        } else {
            Piece::Unread(&group[..=close])
        });
        rest = &group[close + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Text(rest));
    }
    pieces
}

/// The name of each `{name}` in `text`, in order. An alternative list and an unread group give none.
pub fn names(text: &str) -> Vec<&str> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Name(name) => Some(name),
            _ => None,
        })
        .collect()
}

/// The groups of `text` that no reader reads: see [`Piece::Unread`].
pub fn unread(text: &str) -> Vec<&str> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Unread(group) => Some(group),
            _ => None,
        })
        .collect()
}

/// True when `text` has a group in braces of any kind, also one that nothing reads.
pub fn has_placeholder(text: &str) -> bool {
    pieces(text)
        .iter()
        .any(|piece| !matches!(piece, Piece::Text(_)))
}

/// True when `segment` of a route is one `{name}` and nothing else.
pub fn is_placeholder(segment: &str) -> bool {
    matches!(pieces(segment).as_slice(), [Piece::Name(_)])
}

/// `text` with the value of each `{name}`. `value` gives the value of a name, and its error is the
/// error of the fill. An alternative list and an unread group stay as they are: [`expand`] takes
/// the alternatives first.
///
/// # Errors
///
/// The first error of `value`.
pub fn fill<E>(text: &str, mut value: impl FnMut(&str) -> Result<String, E>) -> Result<String, E> {
    let mut filled = String::new();
    for piece in pieces(text) {
        match piece {
            Piece::Text(text) | Piece::Unread(text) => filled.push_str(text),
            Piece::Name(name) => filled.push_str(&value(name)?),
            Piece::Alternatives(names) => filled.push_str(&format!("{{{}}}", names.join(","))),
        }
    }
    Ok(filled)
}

/// One text for each choice of the alternatives of `text`: the choices, and the text with them.
/// A text with no alternative gives itself, with no choice. A text with two lists gives one text
/// for each pair of choices.
pub fn expand(text: &str) -> Vec<(Vec<&str>, String)> {
    let mut expanded: Vec<(Vec<&str>, String)> = vec![(Vec::new(), String::new())];
    for piece in pieces(text) {
        expanded = match piece {
            Piece::Text(text) | Piece::Unread(text) => expanded
                .into_iter()
                .map(|(choices, mut done)| {
                    done.push_str(text);
                    (choices, done)
                })
                .collect(),
            Piece::Name(name) => expanded
                .into_iter()
                .map(|(choices, done)| (choices, format!("{done}{{{name}}}")))
                .collect(),
            Piece::Alternatives(names) => expanded
                .into_iter()
                .flat_map(|(choices, done)| {
                    names.iter().map(move |name| {
                        let mut choices = choices.clone();
                        choices.push(*name);
                        (choices, format!("{done}{name}"))
                    })
                })
                .collect(),
        };
    }
    expanded
}

impl Fact {
    /// The method and the path of the request, with its query, such as `GET` and
    /// `/v1/clusterprofiles/{uid}`. None when the fact has no request, or its request starts with
    /// a text that is no method.
    pub fn parts(&self) -> Option<(Method, &str)> {
        split_request(self.request.as_deref()?)
    }

    /// The path and the query of the request, when its method is GET.
    pub fn get_path(&self) -> Option<&str> {
        match self.parts()? {
            (Method::Get, path) => Some(path),
            _ => None,
        }
    }
}

/// What strata reads from the answer of Palette, or depends on: the fields, the error codes, the
/// values of a closed set, and the names that strata needs. A fact states at least one of the
/// four.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reads {
    /// A field of the answer, as the path from the root of the document, with `[]` for an array
    /// and `*` for the values of an object:
    /// `spec.clusterProfileTemplates[].packs`.
    #[serde(default)]
    pub fields: Vec<String>,
    /// An error code that Palette gives, such as `ClusterProfilePackNotAllowed`. The code is on a
    /// page of the fact.
    #[serde(default)]
    pub codes: Vec<String>,
    /// The values of a closed set that strata names, by the field that holds them:
    /// `status.state` and the states that strata tells apart. Palette gives no other value.
    #[serde(default)]
    pub values: BTreeMap<String, Vec<String>>,
    /// The closed set of strata that a list of `values` is, by the same field: the name of its type,
    /// such as `ClusterState`. The list and the type must agree, so a new value in one shows in
    /// the check. A list that is a part of a set, or a set that has no type, names none.
    #[serde(default)]
    pub sets: BTreeMap<String, String>,
    /// The names that strata needs from a set that Palette can extend, by the field that holds
    /// them: `schema.properties[].name` and the properties that strata sends in a search. Each
    /// name must be in the answer. Another name is no fault.
    #[serde(default)]
    pub names: BTreeMap<String, Vec<String>>,
}

impl Reads {
    /// True when the fact states no field, no code, no value, and no name.
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty() && !self.reads_a_document()
    }

    /// True when the fact states a field, a value, or a name of the answer. A fact that states
    /// error codes alone reads no document.
    pub fn reads_a_document(&self) -> bool {
        !(self.fields.is_empty() && self.values.is_empty() && self.names.is_empty())
    }
}

/// A file of the list.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    schema: String,
    facts: Vec<Fact>,
}

/// Each fact of the files in `dir`, in the order of the file names and then of the file, and a
/// message for each file that cannot be read. The facts of the other files stay, so one fault in
/// one file does not hide the faults of the others.
pub fn load(dir: &Path) -> (Vec<Fact>, Vec<String>) {
    let mut facts = Vec::new();
    let mut bad = Vec::new();
    let files = files_with_ext(dir, "yaml");
    if files.is_empty() {
        bad.push(format!("{}: no file of facts", dir.display()));
    }
    for file in files {
        let name = file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let part = std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_yaml::from_str::<Part>(&text).map_err(|e| e.to_string()));
        match part {
            Ok(part) if part.schema == SCHEMA => facts.extend(part.facts),
            Ok(part) => bad.push(format!(
                "{name}: the schema is `{}`, and it must be `{SCHEMA}`",
                part.schema
            )),
            Err(e) => bad.push(format!("{name}: {e}")),
        }
    }
    (facts, bad)
}
