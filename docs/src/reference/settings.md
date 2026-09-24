# Settings

Strata reads each variable from the environment. The key, the address, the
project, `STRATA_RETRIES`, and `STRATA_MAX_RETRY_AFTER` also come from
`~/.palette.env`, when the environment does not set them. A line of that file
is `KEY=VALUE`. Strata accepts `export ` before the key, and quotes around the
value. A line that starts with `#` is a comment. An empty value counts as no
line.

| Variable | Default | Function |
| --- | --- | --- |
| `PALETTE_API_KEY` | none, required | The API key. |
| `SPECTROCLOUD_APIKEY` | | The API key, when `PALETTE_API_KEY` is not set. |
| `PALETTE_API` | `https://api.spectrocloud.com` | The Palette address. |
| `SPECTROCLOUD_HOST` | | The Palette address, when `PALETTE_API` is not set. An address with no scheme gets `https://`, in either variable. |
| `PALETTE_PROJECT_UID` | tenant scope | The project, by uid. An empty value is no uid, also when `~/.palette.env` sets one. `PALETTE_PROJECT` then decides. |
| `PALETTE_PROJECT` | tenant scope | The project, by name, when `PALETTE_PROJECT_UID` is not set. An empty value is tenant scope, also when `~/.palette.env` sets a name. An unknown name is the error `project-not-found`, which lists the projects that the key can see. |
| `STRATA_RETRIES` | `3` | The number of retries after HTTP 5xx, HTTP 429, or a transport error. Strata sends no other request again. |
| `STRATA_MAX_RETRY_AFTER` | `300` | The longest `Retry-After` wait, in seconds. A longer one is the error `retry-after-too-long`, while a retry remains. On the last attempt, the error is `api`. |
| `STRATA_QUIET` | not set | Any value stops the progress messages, as `--quiet` does. Environment only. |
| `NO_COLOR` | not set | A value removes the color of the terminal report under `--color auto`, the default. Environment only. |
| `CLICOLOR_FORCE` | not set | A value forces the color under `--color auto`, the default, also for a pipe. `NO_COLOR` has priority. Environment only. See [Report formats](./formats.md#colors). |
| `STRATA_BIN` | `strata` on `PATH` | The binary that the scripts in `scripts/ci/` run. The live example and `examples/fetch-examples.sh` read it too. Their default is the build under `target/`. |

A value that strata cannot read, such as `STRATA_RETRIES=lots`, is the error
`invalid-setting`.

The wait before a retry starts at one second, doubles, and stops at 30 seconds.
Strata obeys a `Retry-After` in seconds, and ignores one that gives a date.

Each request times out after two minutes. No variable changes that time. A
request that times out is the error `transport`.

## Command options

`--project <uid>` and `--project-name <name>` replace the configured project for
one command. `--project ''` is tenant scope. `list` and `verify` take these
options.

## The address in a pipeline

Strata reads the address from `PALETTE_API`, then from `SPECTROCLOUD_HOST`.
Strata reads no other variable for the address. The tools around strata can use
a different name. A job that sets only that name gives strata no address, and
strata sends its requests to `https://api.spectrocloud.com`. It gives no
warning. Export `SPECTROCLOUD_HOST` also. The Spectro Cloud provider reads that
variable, so one value gives strata and Terraform the same Palette. Strata reads
the key in the same way, from `PALETTE_API_KEY`, then `SPECTROCLOUD_APIKEY`.

## The live example

The scripts in `examples/live/` read the same key and address. They pass the
address to OpenTofu as `SPECTROCLOUD_HOST`, so strata and OpenTofu use the same
Palette. `TOFU_VERSION` sets the version that `just tofu-install` installs. See
[The live example](../development/live-example.md).
