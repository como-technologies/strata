//! The check of a rule: its four kinds, and what each adds to the world before its program runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strata::api::Method;
use strata::closed_set::impl_closed_set;
use strata::closed_set::legend::Described;

use super::Tier;

/// The method of a check of Palette that names none: a read.
fn read() -> Method {
    Method::Get
}

/// A check of Palette, as the rule file gives it. [`Check::Palette`] holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct PaletteCheck {
    /// `GET` or `POST`.
    #[serde(default = "read")]
    pub method: Method,
    /// The path, with `{{ jq }}` templates over the world: `/v1/spectroclusters/{{ .cloud }}/validate`.
    /// With `each`, a template reads `$item`.
    pub path: String,
    /// For a POST: a jq program over the world that gives the body. Default `.entity`. With
    /// `each`, the program reads `$item`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// A jq program over the world that gives a list. The check makes its request one time for
    /// each item, and `$item` is the item in `path` and `body`. With no value, the check makes
    /// one request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub each: Option<String>,
    /// The `code` values of Palette that the route answers with a 5xx, as a refusal of the
    /// request. A 5xx with one of these codes is an answer in `.response`, with `ok: false`, as a
    /// 4xx is. A 5xx with another code, or with no code, is an error. With no value, each 5xx is
    /// an error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refusal_codes: Vec<String>,
    /// What Palette saves at the request, when the route saves something. The line of the report
    /// for the request says it in a comment, and so does each line of the rule that sends it.
    /// With no value, the route saves nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saves: Option<Saves>,
    /// The program over the world with `.response`. With `each`, it runs one time over the
    /// world with `.responses`: one `{item, response}` for each item, in the order of the list.
    pub expr: String,
}

/// What Palette saves at a request that only asks a question. The route of the request checks a
/// body, and it saves a record too.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Saves {
    /// The address of the console of a pack of virtual machines, at
    /// `POST /v1/spectroclusters/{uid}/validate/repave`. Palette saves it for each such pack that
    /// the cluster runs, and for each one that the body holds. The address comes from the values
    /// of the pack, or Palette makes one.
    ConsoleAddress,
}

impl_closed_set!(Saves {
    ConsoleAddress => "console-address",
});

impl Described for Saves {
    fn describe(self) -> &'static str {
        match self {
            Saves::ConsoleAddress => {
                "for a cluster or a change with the pack virtual-machine-orchestrator, spectro-vm-dashboard, or vm-migration-assistant, Palette saves the address of the console of that pack"
            }
        }
    }
}

/// A command on this machine, as the rule file gives it. [`Check::Exec`] holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct ExecCheck {
    /// The command and its arguments, each with `{{ jq }}` templates over the world.
    pub argv: Vec<String>,
    /// The tools the command needs, by name, checked on the PATH first.
    #[serde(default)]
    pub needs: Vec<String>,
    /// Seconds before the command is stopped. Default 60.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// A read of Palette that the check makes before the command. With no value, the check
    /// reads no answer of Palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<PaletteRead>,
    /// A second command, such as the read of a quota. It runs after `argv`, with the same `needs`
    /// and the same `timeout`, and its output is `.also` in the world. A failure of this command
    /// is an answer: the program reads `.also.exit`. With no value, the check runs one command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// The program over the world with `.exec`, with `.also` when `also` is set, and with
    /// `.response` when `palette` is set.
    pub expr: String,
}

/// A read of Palette that an exec check makes before its command: one `GET` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(inline)]
pub struct PaletteRead {
    /// The path, with `{{ jq }}` templates over the world, such as
    /// `/v1/clouds/{{ .accountKind }}/regions/{{ .entity.spec.cloudConfig.region }}/instancetypes`.
    pub path: String,
}

/// The check of a rule: how strata-preflight finds the fault. Each kind runs a jq program over
/// the world document (see [`crate::world`]), and the kinds differ in what they add to it first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Check {
    /// A jq program over the world alone: the entity, the change, the records, the release.
    Static {
        /// The program. Its output is `true`/`false`, a list of failure messages, or
        /// `{pass, message}`.
        expr: String,
    },
    /// One request to Palette, then a jq program over the world with `.response` added:
    /// `{ok, status, body}`. A 4xx is an answer, with `ok: false` and its `V1Error` body. So is a
    /// 5xx with a code in `refusal_codes`. A 204 is `ok: true` with a `null` body.
    ///
    /// With `each`, the request runs one time for each item of a list. The program runs one time
    /// over the world with `.responses` added instead of `.response`.
    Palette(PaletteCheck),
    /// One command on this machine, such as the tool of the cloud, then a jq program over the
    /// world with `.exec` added: `{exit, stdout, stderr}`. `stdout` is JSON when it parses, and
    /// the text otherwise. The command runs when each tool of `needs` is on the PATH. When one is
    /// not, the result is `NOT CHECKED` with the command to run by hand.
    ///
    /// With `palette`, the check reads Palette first, and the program also reads `.response`, as
    /// in a check of Palette. The check is `NOT RUN` when no key is there.
    ///
    /// With `also`, a second command runs after the first, and the program also reads `.also`.
    Exec(ExecCheck),
    /// During a build, every `every` seconds: read the cluster, its summary, and its events. Then
    /// run the program over the world with `.cluster`, `.summary`, `.events`, and `.elapsed`
    /// (seconds) added. A failure stops the watch with its evidence.
    Watch {
        /// The program. It fails when the cluster is in a known stuck state.
        expr: String,
    },
}

/// The kind of a check, as the rule files spell it in `kind`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum CheckKind {
    /// A jq program over the world alone.
    Static,
    /// One request to Palette, or one for each item.
    Palette,
    /// One command on this machine.
    Exec,
    /// The conditions and the events of the cluster during the build.
    Watch,
}

impl_closed_set!(CheckKind {
    Static => "static",
    Palette => "palette",
    Exec => "exec",
    Watch => "watch",
});

impl CheckKind {
    /// The tier that a check of this kind belongs to.
    pub fn tier(self) -> Tier {
        match self {
            CheckKind::Static => Tier::Static,
            CheckKind::Palette => Tier::Palette,
            CheckKind::Exec => Tier::Cloud,
            CheckKind::Watch => Tier::Watch,
        }
    }
}

impl Check {
    /// The kind of the check.
    pub fn kind(&self) -> CheckKind {
        match self {
            Check::Static { .. } => CheckKind::Static,
            Check::Palette(_) => CheckKind::Palette,
            Check::Exec(_) => CheckKind::Exec,
            Check::Watch { .. } => CheckKind::Watch,
        }
    }

    /// The tier a check of this kind belongs to.
    pub fn tier(&self) -> Tier {
        self.kind().tier()
    }

    /// The main program of the check.
    pub fn expr(&self) -> &str {
        match self {
            Check::Static { expr }
            | Check::Palette(PaletteCheck { expr, .. })
            | Check::Exec(ExecCheck { expr, .. })
            | Check::Watch { expr } => expr,
        }
    }
}
