//! The `curl` to Palette that a person runs by hand, and what Palette saves at its request.
//!
//! A `reproduce` line of a rule starts with it, through the filter
//! [`PALETTE_CURL`](crate::jq::PALETTE_CURL), and so does each line that the report adds for a
//! request of a check.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strata::api::{API_KEY_HEADER, PROJECT_UID_HEADER};
use strata::closed_set::impl_closed_set;
use strata::config::{KEY_VARS, PROJECT_VARS};

/// What Palette saves at a request that only asks a question. The route of the request checks a
/// body, and it saves a record too.
///
/// A check of kind `palette` names it in `saves`. A line that sends the request ends with
/// [`Saves::comment`], so a person knows it before the line runs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Saves {
    /// The address that the console of a pack of virtual machines forwards to, at
    /// `POST /v1/spectroclusters/{uid}/validate/repave`. Palette saves it from the cluster, and
    /// then from the body. The address comes from the values of the pack, or Palette makes one.
    /// Palette saves nothing while the load balancer of the pack has no address.
    ConsoleAddress,
}

impl_closed_set!(Saves {
    ConsoleAddress => "console-address",
});

impl Saves {
    /// The end of a line that sends a request at which Palette saves this: three spaces, then a
    /// comment that says what Palette saves.
    pub fn comment(self) -> &'static str {
        match self {
            Saves::ConsoleAddress => {
                "   # for a cluster or a change with the pack virtual-machine-orchestrator, spectro-vm-dashboard, or vm-migration-assistant, Palette can save the address that the console of that pack forwards to, and the request can move it"
            }
        }
    }
}

/// What the start of a `curl` prints when the key holds a line break, and it then runs no `curl`.
/// `examples/live/palette-curl.sh` stops with the same words, and a test of the catalog runs both.
pub const KEY_LINE_BREAK: &str = "the key of Palette holds a line break";

/// The start of a `curl` to Palette: a check of the key, the command, and the headers of the key
/// and of the project.
///
/// The key goes to `curl` in a config that `printf` writes into `<( )`. `printf` is a builtin of
/// the shell, and the shell runs it in a copy of itself. So no process gets the key as an
/// argument, and the list of processes does not show it. The config reads a `\` and a `"` in a
/// quoted value as an escape, so the line puts a `\` before each one. A line break in the key can
/// add a line to the config. So a key with a line break stops the line before `curl`, with
/// [`KEY_LINE_BREAK`]. `examples/live/palette-curl.sh` does the same. The names of the headers and
/// of the variables are the ones that strata reads. The line needs bash or zsh, for `[[ ]]` and
/// `<( )`.
pub(crate) fn palette_curl() -> String {
    format!(
        "{{ [[ ${key} != *[$'\\r\\n']* ]] || ! echo '{KEY_LINE_BREAK}' >&2; }} && curl -s --config <(k=${{{key}//\\\\/\\\\\\\\}}; k=${{k//\\\"/\\\\\\\"}}; printf 'header = \"{API_KEY_HEADER}: %s\"\\n' \"$k\") -H \"{PROJECT_UID_HEADER}: ${project}\"",
        key = KEY_VARS[0],
        project = PROJECT_VARS[0]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_of_a_curl_sends_the_key_in_a_config_and_the_project_in_a_header() {
        let start = palette_curl();
        assert!(
            start.starts_with(&format!("{{ [[ ${} != *[$'\\r\\n']* ]]", KEY_VARS[0])),
            "{start}"
        );
        for part in [
            format!("|| ! echo '{KEY_LINE_BREAK}' >&2; }} && curl -s --config <("),
            format!("printf 'header = \"{API_KEY_HEADER}: %s\"\\n' \"$k\")",),
            format!(" -H \"{PROJECT_UID_HEADER}: ${}\"", PROJECT_VARS[0]),
        ] {
            assert!(start.contains(&part), "{part} is not in {start}");
        }
        assert!(
            !start.contains(&format!("-H \"{API_KEY_HEADER}")),
            "the key is in an argument: {start}"
        );
    }
}
