# Settings

Strata reads each variable from the environment. The key, the address, and the
project also come from `~/.palette.env`, when the environment does not set
them.

| Variable | Default | Function |
| --- | --- | --- |
| `PALETTE_API_KEY` | none, required | The API key. |
| `SPECTROCLOUD_APIKEY` | | The API key, when `PALETTE_API_KEY` is not set. |
| `PALETTE_API` | `https://api.spectrocloud.com` | The Palette address. |
| `SPECTROCLOUD_HOST` | | The Palette address, when `PALETTE_API` is not set. An address with no scheme gets `https://`, in either variable. |
| `PALETTE_PROJECT_UID` | tenant scope | The project, by uid. |
| `PALETTE_PROJECT` | tenant scope | The project, by name, when `PALETTE_PROJECT_UID` is not set. An unknown name is the error `project-not-found`, which lists the projects that the key can see. |
| `STRATA_RETRIES` | `3` | The number of retries after HTTP 5xx, HTTP 429, or a transport error. Strata sends no other request again. |
| `STRATA_MAX_RETRY_AFTER` | `300` | The longest `Retry-After` wait, in seconds. A longer one is the error `retry-after-too-long`, while a retry remains. On the last attempt, the error is `api`. |
| `STRATA_QUIET` | not set | Any value stops the progress messages, as `--quiet` does. Environment only. |
| `NO_COLOR` | not set | Removes the color of the terminal report. Environment only. |
| `CLICOLOR_FORCE` | not set | Forces color, also for a pipe. `NO_COLOR` has priority. Environment only. |
| `STRATA_BIN` | `strata` on `PATH` | The binary that the scripts in `scripts/` run. |

A value that strata cannot read, such as `STRATA_RETRIES=lots`, is the error
`invalid-setting`.

The wait before a retry starts at one second, doubles, and stops at 30 seconds.
Strata obeys a `Retry-After` in seconds, and ignores one that gives a date.

## Command options

`--project <uid>` and `--project-name <name>` replace the configured project for
one command. `--project ''` is tenant scope. `list` and `verify` take these
options.

## The address in a pipeline

Strata reads the address from `PALETTE_API`, then `SPECTROCLOUD_HOST`, and from
no other variable. If the tools around it name the address differently, a job
that sets only that name leaves strata on the default address, and strata says
nothing: the requests go to `https://api.spectrocloud.com`. Export
`SPECTROCLOUD_HOST` as well, which the Spectro Cloud provider reads too, so one
value gives strata and OpenTofu one Palette. The key reads the same way, from
`PALETTE_API_KEY`, then `SPECTROCLOUD_APIKEY`.

## The live example

The scripts in `examples/live/` read the same key and address. They pass the
address to OpenTofu as `SPECTROCLOUD_HOST`, so strata and OpenTofu use the same
Palette. `TOFU_VERSION` sets the version that `just tofu-install` installs. See
[The live example](../development/live-example.md).
