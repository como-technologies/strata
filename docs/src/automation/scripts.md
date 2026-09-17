# Scripts and exit codes

A script reads the exit status and the JSON documents. The terminal text and the
markdown can change in any release. The exit status, the documents, and the
error kinds do not. See [Versions and compatibility](../reference/compatibility.md).

## Branch on the exit status

| Exit | Meaning |
| --- | --- |
| `0` | The same, unchanged, or not in use. |
| `1` | Different, drifted, or in use. |
| `2` | Strata could not answer. Treat it as a failed job, never as "no change". |

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

Some rules that teams use:

```bash
# a repave needs a second approver
jq -e '.summary.maxImpact != "repave"' report.json || gh pr edit "$PR" --add-label repave

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

With `--json`, an error is one JSON line on standard error, and the exit status
is `2`:

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

Strata writes progress messages to standard error. `-q`, `--quiet`, or
`STRATA_QUIET=1` stops them.
