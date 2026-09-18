# strata verify

Make sure that each record agrees with Palette now. For each record, strata
reads the uid and the project from the file, fetches that uid again, and
compares the roots.

The examples use the project `dev`, which holds `aws-stack@1.1.1`, and the cluster
`payments` that runs it.

## Check one record

```text
{{#include ../output/edited-verify-ok.txt}}
```

One file prints the full report and a verdict line. Exit `0`.

## A record that drifted

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. A person edited the published version in place: same uid, same
version, other content. See
[A published version is edited in place](../use-cases/edited-in-place.md).

## Check many records

Give two or more files, or a pattern in quotes. Strata expands the pattern, so
it works in any shell or scheduler:

```text
{{#include ../output/edited-bulk.txt}}
```

Each record gets one line: `OK`, `DRIFT`, or `ERROR`. A record that strata
cannot read or check is an `ERROR` line, and strata checks the other records.

```text
{{#include ../output/edited-bulk-json.txt}}
```

`--summary` prints the summary also for one file, so a script always reads one
document shape.

## The verdicts

| Verdict | Cause | Exit |
| --- | --- | --- |
| `OK: published version <v> is unchanged since <time>` | The root is the same. | `0` |
| `OK: cluster <c> running <v> is unchanged since <time>` | A cluster record: the cluster runs the recorded release. | `0` |
| `DRIFT: cluster <c> running <v> differs from the copy recorded <time>` | A cluster record: the cluster runs other content now. | `1` |
| `DRIFT: published version <v> differs from the copy recorded <time>` | An edit in place. | `1` |
| `DRIFT: published version <v> now names <v2>: the version recorded <time> was overwritten in place` | The uid holds another version now. | `1` |
| `DRIFT: cluster <c> runs <v>, but uid <uid> now names <version>: the version was overwritten in place` | A cluster record: its profile uid holds another version now. | `1` |
| `DRIFT: cluster <c> runs <v>, but uid <uid> no longer exists` | A cluster record: its profile uid is gone. | `1` |

Each verdict of a cluster record ends with `; N variable value changed` when the
cluster gives a variable another value than the record holds.

A renamed tag in place is `DRIFT` too, with the result `SAME CONTENT` and no
node impact. It changes no build, but it edits a published version. See
[One pack build under two tag names](../use-cases/tag-name.md).

For a cluster record, strata reads each profile that the cluster runs, one
request for each. See [A version bump overwrites a version](../use-cases/overwritten.md).

## Run it on a schedule

`scripts/ci/verify-report.sh` runs `verify` over each record, and keeps one GitHub
issue open for each record that drifted. See
[Drift issues](../automation/drift-issues.md).

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | Each record is unchanged. |
| `1` | A record drifted. |
| `2` | Strata could not check a record. |

- Strata checks each record in the project that the record names. A record from
  tenant scope has no project, so strata checks it in the configured project.
- `--project <uid>` or `--project-name <name>` checks each record in that
  project, not in the project of the record.
- `--format markdown` writes one table for many files, and the full report for
  one file.
- `--json` writes [`strata-diff/1`](../reference/documents.md#strata-diff1) for
  one file, and [`strata-verify-summary/1`](../reference/documents.md#strata-verify-summary1)
  for many.

## Options

```text
{{#include ../output/help-verify.txt}}
```
