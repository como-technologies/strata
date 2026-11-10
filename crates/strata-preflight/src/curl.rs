//! The `curl` to Palette that a person runs by hand. A `reproduce` line of a rule starts with it,
//! through the filter [`PALETTE_CURL`](crate::jq::PALETTE_CURL), and so does each line that the
//! report adds for a request of a check.

use strata::api::{API_KEY_HEADER, PROJECT_UID_HEADER};
use strata::config::{KEY_VARS, PROJECT_VARS};

/// The start of a `curl` to Palette: the command, and the headers of the key and of the project.
///
/// The key goes to `curl` in a config that `printf` writes into `<( )`. `printf` is a builtin of
/// the shell, and the shell runs it in a copy of itself. So no process gets the key as an
/// argument, and the list of processes does not show it. The names of the headers and of the
/// variables are the ones that strata reads. The line needs a shell with `<( )`, such as bash.
pub(crate) fn palette_curl() -> String {
    format!(
        "curl -s --config <(printf 'header = \"{API_KEY_HEADER}: %s\"\\n' \"${key}\") -H \"{PROJECT_UID_HEADER}: ${project}\"",
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
        assert!(start.starts_with("curl -s --config <(printf "), "{start}");
        for part in [
            format!(
                "'header = \"{API_KEY_HEADER}: %s\"\\n' \"${}\")",
                KEY_VARS[0]
            ),
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
