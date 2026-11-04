# Scripts and exit codes

A script reads the exit status and the JSON documents. The terminal text and the
markdown can change in any release. The exit status, the documents, and the
error kinds do not. See [Versions and compatibility](../reference/compatibility.md).

## Branch on the exit status

| Exit | Meaning |
| --- | --- |
| `0` | The same, unchanged, or not in use. |
| `1` | Different, drifted, unverified, behind, or in use. |
| `2` | Strata could not answer. Treat it as a failed job, never as "no change". |

`strata preflight` follows the same rule: `0` when each counted check passed,
`1` when one failed or the readiness is under `--ready`, `2` when it could not
answer. See [Preflight in a pipeline](./preflight.md).

```bash
strata consumers aws-stack@1.0.0 && delete_the_version
```

## Read one field

```text
{{#include ../output/auto-max-impact.txt}}
```

## Enforce a rule

Write the report to a file. Then test it with `jq -e`, which exits `1` when the
test is false:

```text
{{#include ../output/auto-gate.txt}}
```

The first command exits `1`, because the two versions differ. The gate reads the
status of `jq`, not of `strata`, so do not join the two with `&&`: that never
reaches the test. Under `set -e`, run the comparison with `|| true`, or read its
status on purpose.

A `jq` older than 1.7 exits with `0` for an empty file, for example the report
of a `strata` that failed. The gate then passes. Make sure that the file has
content first:

```bash
test -s report.json && jq -e '.summary.maxImpact != "repave"' report.json
```

Some rules that teams use:

```bash
# a repave needs a second approver: pr-comment.sh adds the label on both hosts
scripts/ci/pr-comment.sh A B --threshold repave --label needs-second-approver

# the layers that replace nodes
jq -r '.packs[] | select(.impact.level == "repave") | "\(.name): \(.impact.text)"' report.json

# no variable changes in a patch release
jq -e '.summary.variablesChanged | not' report.json

# no pack comes or goes in a patch release
jq -e '.summary.packsAdded == 0 and .summary.packsRemoved == 0' report.json
```

[JSON documents](../reference/documents.md) lists each field.
`strata schema <kind>` prints the JSON Schema.

## Read an error

With `--json --quiet`, an error is one JSON line on standard error, and the exit
status is `2`. `--quiet` is what keeps the progress notes off standard error; a
script that omits it gets the notes and the JSON line together:

```text
{{#include ../output/auto-error.txt}}
```

Without `--json`, the line is the message after `strata:`:

```text
{{#include ../output/auto-error-text.txt}}
```

`kind` is stable. [Errors](../reference/errors.md) lists each kind and its
fields.

## Progress messages

Strata writes progress messages to standard error. `--quiet` stops them.
[Settings](../reference/settings.md) names the variable that does the same.

## Send the report of a new value

`STRATA_REPORT=1` in a job sends the report at the end of a run that wrote a line to the log. The
log is in the state folder of the runner, so save the folder when the runner does not stay. A
reusable workflow of GitHub gets no variable from the workflow of the caller, so set the variable in
a job of your own. See [The files](../reference/report.md#the-files).

## The scripts in this repository

Each release carries these beside the binaries, so a CI job downloads one
release and has everything.

| Script | Does |
| --- | --- |
| [`pr-comment.sh`](./pr-comments.md) | One comment on a pull request or a merge request, and a label from the impact. |
| [`merge-gate.sh`](./merge-gate.md) | The check that the merge button waits for: the records of the merge result. |
| [`verify-report.sh`](./drift-issues.md) | One issue per record that drifted, is unverified, or could not be checked. |
| [`upstream-report.sh`](./upstream-issues.md) | One issue per pack that a record pins behind a newer version of its registry. |
| [`preflight.sh`](./preflight.md#on-another-host) | `strata preflight` with its options from variables, and the report as a comment. |
| [`observer-records.sh`](../development/observer-deploy.md#the-records) | The records of the observer as four Kubernetes Secrets, on standard output. |
| [`install-strata.sh`](./gitlab.md#install-strata-in-a-job) | One release, with the signature and the checksums checked. |

They run on GitHub and on GitLab. `examples/ci/github/strata.yml` and
`examples/ci/gitlab/strata.yml` are the two drop-ins that call them. See
[GitLab CI](./gitlab.md).
