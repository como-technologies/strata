# strata preflight

Check that Palette can build a cluster, before the build. The input is the body
of the create, `POST /v1/spectroclusters/{cloudType}`, or a cluster as Palette
holds it now. Strata runs each rule of a catalog against it and reports what
will stop the build, in seconds, with the command that shows the same thing by
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

## The static tier: no key, no network

```text
{{#include ../output/preflight-static.txt}}
```

`--verbose` prints, after each check, what it ran, the answer, the time, and
the verdict: the request to Palette with its HTTP status, the command with its
exit code, or `jq over the world` for a static check. It is the live form of
the reproduce lines. It wins over `STRATA_QUIET`; `--quiet` wins over both.

```text
{{#include ../output/preflight-verbose.txt}}
```

Each rule is one line: the result, the id, what the rule checks, and the time.
A failed rule adds its messages, a `reproduce:` line that shows the same thing
with `jq`, and the fix for good with its owner. `NOT CHECKED` is a rule that
strata cannot run here, with the commands to run by hand.

The result names the level of the rule. `PROBE PASS` and `PROBE FAIL` are
probes: strata knows the fault, and the runs do not yet prove the check. `PASS`
and `FAIL` are preflight checks: the runs prove them. A failed probe does not
change the exit code; a failed preflight check makes it `1`. See
[Two levels: probe and preflight check](../internals/preflight.md#two-levels-probe-and-preflight-check).

## The live tier: the answers of Palette

With a key, strata records each profile of the entity, reads the cloud account
and the pack of each layer, and calls the validate routes of Palette:

```text
{{#include ../output/preflight-live.txt}}
```

`POST /v1/spectroclusters/aws/validate` answered that the workers have too few
cores for the packs. Only the console calls this route; a create through the
provider or the API skips it, and finds the same fault after a full apply. See
[The validate routes](../internals/palette/validate-routes.md).

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
reports and the world, or that no file was written.

## The report as JSON

```text
{{#include ../output/preflight-json.txt}}
```

One entry for each rule that applies: `id`, `tier`, `level`, `severity`,
`verdict`, `messages`, `elapsedMs`, `ran`, `reproduce`, `fix`, `evidence`, and
`confidence`, the counts of the runs of the rule on this engine with the 95%
lower bound of each rate. The summary and the section scorecard:

```text
{{#include ../output/preflight-json-summary.txt}}
```

[JSON documents](../reference/documents.md#strata-preflight1) lists each field.
`strata preflight --schema report` prints the JSON Schema.

## The report on disk

`--out DIR` writes the report in three forms, and the two files that the
`reproduce:` lines read:

```text
{{#include ../output/preflight-out.txt}}
```

`report.html` is one page with the scorecard, each result, and the reproduce
lines, for a person who did not run the command. `world.json` is the document
each rule reads, with no secret in it; `entity.json` is the body of the create.

## The rules

```text
{{#include ../output/preflight-rules.txt}}
```

`--rules` lists each rule that applies to an engine, with its tier, its level,
its severity, its status, and its engines. `--rules --json` gives the same list
as a document, with the check of each rule, so a pipeline reads the catalog
without this book:

```text
{{#include ../output/preflight-rules-json.txt}}
```

The rules are data in the binary, one file for each section under
`preflight/rules/`. [The rule format](../reference/preflight-rules.md) states
the format.

## Custom probes

`--probes FILE` adds the rules of a file in the same format: the allowed
regions, the subnets of a landing zone, the tags that a policy needs, the cloud
account of each environment. A custom probe reads the same world document, runs
the same way, and follows the same promotion. Strata ships no custom probe and
assumes no topology.

## The watch

`--watch cluster:UID` reads the cluster, its summary, and its events every
`--every` seconds, and judges each watch rule. It ends at the first counted
failure (a rule of severity `fail` at the preflight level, or any failed probe
with `--strict`), with the event that shows the fault, or when the cluster is
`Running` with each counted rule passed, or at `--limit`. A failed probe or a
warning is in the report, and the watch reads on. With `--out`, `world.json`
holds the last reading: the conditions and the events the rules saw. A build
that Palette cannot finish stops in minutes, not at the timeout of the client.

## Confidence and promotion

```text
{{#include ../output/preflight-promotion.txt}}
```

`--promotion` compares the level of each rule with the runs of the ledger:
which probes meet the bar, and which preflight checks no longer do. Exit `1`
when a level disagrees with the ledger. `--ledger FILE` adds the runs of a
company to the runs of the lab. See
[Confidence](../internals/preflight.md#confidence).

## Options

```text
{{#include ../output/help-preflight.txt}}
```

`--cloud` names the engine of a file; a cluster names its own. `--release`
names the Palette release, which a tenant key cannot read: with none, a rule
of one release applies. `--records` gives the records of the profiles as files,
for a run with no key; with none, strata records each profile from Palette.
`--tier` selects the tiers: `static`, `palette`, `cloud`, as a list. `--rule`
selects rules by id. `--offline` reads nothing from Palette, and `--no-exec`
runs no command of a cloud tool. `--every` and `--limit` set the watch.
`--schema` prints the JSON Schema of the report, the rules, the ledger, or the
promotion report. `--format`, `--json`, `--compact`, `--no-color`, and
`--quiet` are as for the other commands, with `html` as one more format.
`--version` prints the version of `strata-preflight`, which a release keeps
equal to the version of `strata`.
