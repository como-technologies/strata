# strata report

Print the values that Palette gave and that this release of strata does not know. When a read of a
document gives such a value, strata writes a note and appends a line to a log on your machine.
`strata report` reads that log. It sends nothing: you read the report, and you decide whether to
send it. A run of another command can send the report by itself, when you choose it with the setting
`STRATA_REPORT`. See [What strata can report](../reference/report.md).

The examples use a tenant where the profile `aws-stack@1.1.12` has a type that this release does
not know. Two runs of `strata list profiles --packs` noted the type.

## The report

```text
{{#include ../output/unknown-value-report.txt}}
```

Exit `0`. Strata merges the equal lines of the log. A line of the report gives the set, the
value, and the route that gave the value. It also gives how many times the log holds the line, and
where the line comes from. A refusal of Palette also shows the status and the error code. A log
with no line gives a report with no value.

## The report as markdown and as JSON

`--format markdown` prints the report as the body of an issue:

```text
{{#include ../output/unknown-value-report-markdown.txt}}
```

`--json` prints [`strata-report/1`](../reference/documents.md#strata-report1). Each entry has the
fields of the log and a `count`:

```text
{{#include ../output/unknown-value-report-json.txt}}
```

## A link that opens a new issue

`--link` prints the address of a new issue of strata, with the title and the body filled in:

```text
{{#include ../output/unknown-value-report-link.txt}}
```

The address is one long line, and the example shows its start. The body is the markdown report.
Open the address in a browser, read each field, and send the issue from your own account. Strata
makes no request. A report that is too long for an address gives a `usage` error that says to
paste the markdown report. `--link` takes no `--format` and no `--json`.

## The lines that you sent

A report holds each line of the log, unless you marked lines as sent. `--sent` prints the report
and marks its lines as sent. The next report then holds only the new lines:

```text
{{#include ../output/unknown-value-report-sent.txt}}
```

`--all` holds each line of the log again, also the lines that you marked:

```text
{{#include ../output/unknown-value-report-all.txt}}
```

Strata marks a line by itself in one case only: when the endpoint took a report that strata sent
by the setting `STRATA_REPORT`. See
[Send the report by itself](../reference/report.md#send-the-report-by-itself).

## A line that is not valid

A log that you edited can hold a line that strata did not write. A line with a field outside the
list of [`strata-unknown/1`](../reference/documents.md#strata-unknown1) is one case. Another case
is a route that is not a template of a route, such as the path of a real request. `strata report` then stops with the error
`log-invalid`, prints nothing, and marks nothing. The message gives the number of the line. See
[Errors](../reference/errors.md).

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | Strata printed the report, or a link. A report with no value is also exit `0`. |
| `2` | The log has a line that is not valid, the link is too long, strata knows no state folder, or strata cannot read a file. |

Standard error stays empty. The hint of [What strata can report](../reference/report.md#the-hint)
belongs to the note of an unknown value, and this command writes no note.

## Options

```text
{{#include ../output/help-report.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q` (`--quiet`) are the output
options that each report command takes: see [Output formats](./index.md#output-formats).
`--quiet` changes nothing for this command, because it writes no progress note.
