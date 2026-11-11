# strata preflight

Check that Palette can build a cluster, before the build. The input is the body
of the create, `POST /v1/spectroclusters/{cloudType}`, or a cluster as Palette
holds it now. Strata runs each rule of a catalog against it and reports what
stops the build, in seconds, with the command that shows the same thing by
hand.

`strata preflight` runs the `strata-preflight` binary beside it. A release
ships both, and the installer puts both in one directory. The binary also runs
on its own, with the same arguments.

```bash
strata preflight cluster.json --cloud eks      # the body of a create
strata preflight cluster:payments              # an existing cluster, as Palette holds it
strata preflight cluster:payments change.json  # the cluster with a change of its pools or profiles
strata preflight --watch cluster:payments      # during the build
```

The examples use the project `dev`, which holds `aws-stack@1.1.1`, and the body
of a create of an AWS cluster named `payments` with two pools of one `t3.xlarge`.

## What preflight reads and writes

One page for each thing that the command reads or writes, with its output and
the options that it takes:

| Page | What it holds |
| --- | --- |
| [The static tier and the live tier](preflight/tiers.md) | The checks of the body of a create, or of a cluster as Palette holds it now: without a key, and with the answers of Palette. |
| [The report](preflight/report.md) | The report as JSON, and the files that `--out` and `--world` write. |
| [The rules and the custom probes](preflight/rules.md) | The catalog of the rules, and a file of probes of your own. |
| [The watch](preflight/watch.md) | A cluster during its build, with `--watch`. |
| [Confidence and promotion](preflight/promotion.md) | The runs of the ledger against the level of each rule, with `--promotion`. |

## Exit codes, and `--strict`

| Exit | Meaning |
| --- | --- |
| `0` | Each counted check passed. A failed probe is not counted. |
| `1` | A counted check failed, or the readiness is under `--ready`. |
| `2` | Strata could not answer: no key for a live tier, Palette did not answer, or a program failed. |

`--strict` counts a failed probe as a failed check:

```text
{{#include ../output/preflight-strict.txt}}
```

`--ready 80` is a gate on the readiness: the checks that passed, over the
checks that apply to the entity. A check that did not run counts against it: a
tier that was not asked for (`--tier`, `--offline`), a Palette check with no
key, or a command under `--no-exec`. So the static tier alone reads as a low
readiness, and the report says how many checks did not run and why. Exit `1`
when the readiness is under the percentage. A row with no check in
strata-preflight (`NOT CHECKED`) is outside the count; the report prints that
count beside it. The last lines of the report say where `--out` wrote the
reports and the world, or that no file was written. They also say how to
write the world when the run did not.

## Options

```text
{{#include ../output/help-preflight.txt}}
```

| Option | What it does | Page |
| --- | --- | --- |
| `--cloud` | Names the engine of a file. A cluster names its own. | |
| `--release` | Names the Palette release, such as `4.10` or `4.10.12`. A text that is not a version, such as `4.11.x`, is a `usage` error with exit `2`. A tenant key cannot read the release: the one route that gives it refuses a call with a key. With none, a rule of one release applies. With `--promotion`, only the runs of this release count. | [The release of Palette](../internals/palette/limits.md#the-release-of-palette), [The runs of one release](preflight/promotion.md#the-runs-of-one-release) |
| `--records` | Gives the records of the profiles as files, for a run with no key. With none, strata records each profile from Palette. | |
| `--probes` | Adds the rules of a file of custom probes. | [Custom probes](preflight/rules.md#custom-probes) |
| `--tier` | Selects the tiers: `static`, `palette`, `cloud`, as a list. | [The static tier and the live tier](preflight/tiers.md) |
| `--rule` | Selects rules by id. An id that is not in the catalog is a `usage` error with exit `2`. | [The rules](preflight/rules.md#the-rules) |
| `--strict` | Counts a failed probe as a failed check. | |
| `--ready` | Exits `1` when the readiness is under the percentage. | |
| `--offline` | Reads nothing from Palette. | |
| `--no-exec` | Runs no command of a cloud tool. | |
| `--watch` | Watches the build of a cluster. | [The watch](preflight/watch.md) |
| `--every`, `--limit` | Set the watch: the seconds between two readings, and the longest watch. | [The watch](preflight/watch.md) |
| `--rules` | Lists each rule that applies to an engine, and runs nothing. | [The rules](preflight/rules.md#the-rules) |
| `--promotion` | Compares the level of each rule with the runs of the ledger. With `--release`, it counts only the runs of that release. | [Confidence and promotion](preflight/promotion.md) |
| `--ledger` | Adds the runs of a company to the runs of the lab. | [Confidence and promotion](preflight/promotion.md) |
| `--schema` | Prints the JSON Schema of the report, the rules, the ledger, or the promotion report. | |
| `--out` | Writes the report in three forms into a directory. | [The report on disk](preflight/report.md#the-report-on-disk) |
| `--world` | With `--out`, also writes the two files that the `reproduce:` lines read. They hold the values of the variables. | [The report on disk](preflight/report.md#the-report-on-disk) |
| `--format`, `--json`, `--compact`, `--no-color`, `--color`, `-q` (`--quiet`) | As for the other commands, with `html` as one more format. | [Output formats](./index.md#output-formats) |
| `-v`, `--verbose` | Prints, after each check, what it ran, the answer, the time, and the verdict. | [The static tier](preflight/tiers.md#the-static-tier-no-key-no-network) |
| `-V`, `--version` | Prints the version of `strata-preflight`, which a release keeps equal to the version of `strata`. | |
