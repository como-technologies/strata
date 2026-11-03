# What strata can report

Strata publishes nothing by itself: it reads your tenant, and it writes files on your machine.
One thing can leave the machine, and only you can choose to send it. It is a report of the values
that Palette gave and that your release of strata does not know. You read the report, and you
decide how to send it: by hand, with the setting `STRATA_REPORT`, or with a yes to one question.
The setting is off by default, and strata asks the question only in a terminal.
[`strata report`](../commands/report.md) prints the report, and sends no request.

## Why a report exists

Palette can add a value to a closed set between two releases of strata, such as a new type of a
profile. Strata reads the value with no error, and it writes a note. The maintainers of strata
test each release against their own tenant. They cannot see your tenants, your clouds, or your
Palette release. A report tells them that a release of strata is out of date in one place.

A report states that strata is out of date: a value or a shape that strata does not know. It is
never a report of a fault of Palette, or of how a team uses Palette.

## How strata makes a report

1. A run reads a document of Palette, and the document holds a value that the release does not
   know. Strata writes the note on standard error, and it appends one line to the log. See
   [The note for a value that Palette gives](./compatibility.md#the-note-for-a-value-that-palette-gives).
2. `strata report` reads the log. It merges equal lines, and it counts them.
3. You read the report. To send it, you open the link of `strata report --link`, or you paste
   `strata report --format markdown` in a new issue of strata.

Each step is on your machine until you open the link or paste the text. The log stays in the state
folder, and no command sends it.

## The fields of a report

A report is the document `strata-report/1`. It has the fields `schema` and `unknown`. The field
`unknown` has one entry for each different line of the log. An entry has only these fields:

| Field | What it holds |
| --- | --- |
| `strata` | The version of strata that read the value. |
| `set` | The name of the closed set, as `strata schema` names its definition, such as `ProfileType`. |
| `value` | The text of the value. It is one word of printable ASCII, from `!` to `~`, with no space. Strata writes a `?` for each space and for each other character, such as a control character or a zero width space. Strata cuts a value of more than 64 characters, and ends it with `...`. |
| `route` | The template of the route that gave the value, such as `/v1/clusterprofiles/{uid}`. It is never the path of a real request. It starts with `/v1/`, and holds only letters, digits, and the characters `/`, `_`, `-`, `.`, `{`, and `}`. It has 200 characters or fewer. |
| `count` | How many lines of the log are this entry. |

These are the fields of [`strata-unknown/1`](./documents.md#strata-unknown1), the line of the log,
and the field `count`. The report has no time and no `tool` block, because the log has none.
[`strata-report/1`](./documents.md#strata-report1) gives the JSON. `strata schema report` prints its
JSON Schema.

## What strata never reports

A report never holds one of these:

- The name or the uid of a profile, a cluster, a project, a registry, or any other object.
- The address of Palette, the name of a host, or the name of a machine.
- The API key, the tenant, or the project.
- The text of a document or of an error message from Palette.
- The name, the values, or the manifests of a pack.
- A time, or the name of the person who ran strata.

Two rules keep this true. A line of the log has a fixed list of fields, and a test fails on each
other field. The report and the endpoint refuse a field outside the list in the same way.
`strata report` also refuses the whole log when one line is not valid. A line is not valid in
these cases:

- It has a field outside the list.
- A field is missing, or it has the wrong type.
- Its `schema` is not `strata-unknown/1`, or its `strata` or `set` is not a short word.
- Its route is not a template of a route. The `route` field in the table above gives the rule.
- Its value is not a short word. It is empty, or it has more than 64 characters. Or it has a space,
  or a character other than printable ASCII.

The command then stops with the error `log-invalid`, prints nothing, and marks nothing. See
[Errors](./errors.md). The send of a run does not stop on such a line: see
[What a run sends](#what-a-run-sends).

## Send a report

`strata report --link` prints the address of a new issue of strata. The address holds the title,
and the report as markdown in the body. Open the address in a browser, read each field, and send
the issue from your own account. With the link, strata sends nothing itself. To send with no
action of yours, see [Send the report by itself](#send-the-report-by-itself).

When the report is too long for an address, `strata report --link` stops with a `usage` error. Run
`strata report --format markdown`, and paste the text in a new issue.

A maintainer proves the value in the lab. Then the maintainer adds it to the closed set of
strata, or writes why not. See [What to do with a new value](../development/facts.md#what-to-do-with-a-new-value).

## The lines that you sent

The log is a file of yours, and no report changes it. A report holds each line of the log, unless
you marked lines as sent. `strata report --sent` prints the report and marks its lines as sent.
The next report then holds only the lines that came after. `strata report --all` holds each line
again.

Strata marks a line by itself in one case only: when the endpoint took a report that strata sent
by itself. See [Send the report by itself](#send-the-report-by-itself).

## Send the report by itself

The send is off by default. Strata sends the report by itself only when you choose it, and nothing
else starts a send. These settings decide. Set them in the environment or in `~/.palette.env`. The
environment has priority. See [Settings](./settings.md).

| Setting | What it does |
| --- | --- |
| `STRATA_REPORT=1` | Strata sends the report at the end of each run that wrote a line to the log. |
| `STRATA_REPORT=0` | Strata sends nothing and asks nothing. |
| `STRATA_REPORT_URL` | The address of the endpoint. The default is the endpoint of the maintainers, `https://strata-report-4ud4ygs3ua-uc.a.run.app`, which this release names. Set it to send to another endpoint. |

Strata reads no other value of `STRATA_REPORT`. Any other value sends nothing, and strata prints a
warning. A run of `strata preflight --offline` reads nothing from Palette, so it writes no line and
sends nothing.

### Where strata asks and sends

Strata asks and sends in one place only: at the end of a run of the `strata` command, after the
result. The place is the same for each command. A run that wrote no line to the log asks nothing
and sends nothing. The separate command `strata-preflight` never asks and never sends.

| The run | What strata does at its end |
| --- | --- |
| `STRATA_REPORT=1`, or the saved answer yes | It posts the report to the endpoint, and writes a note when the endpoint took it. |
| `STRATA_REPORT=0`, or the saved answer no | It sends nothing and asks nothing. |
| No setting and no answer, in a terminal | It asks the question one time. |
| No setting and no answer, in CI or without a terminal | It sends nothing and asks nothing. |

### The question

When no setting and no answer exist, a run in a terminal asks one question. It asks at the end of
the first run that wrote a line to the log. The question starts with the words Keep strata
informed. It names the page of the book that says what the send holds. It ends with `[y/N]`.

The default answer is no. Strata keeps your answer in the file `strata-unknown.answer` of the
[state folder](#the-files), and it never asks again. The setting `STRATA_REPORT` has priority over
the answer. To get the question again, delete the file.

Strata asks nothing in three cases:

- The variable `CI` has a value.
- Standard input or standard error is not a terminal.
- The run has `--quiet` or `STRATA_QUIET`.

### What a run sends

At the end of a run that wrote a line, strata posts the report of the lines that you did not mark
as sent. The report is the document `strata-report/1` that `strata report --json` prints, and
nothing else. The post has one try and a timeout of 3 seconds. The run prints its output first, so
a slow endpoint delays no output.

The post also holds a key in the header `Idempotency-Key`. The key is a hash of the report and of
the number of lines of the log before it. A second try of the same lines has the same key, so the
endpoint counts them one time. The key holds no name, no uid, and no host.

```text
{{#include ../output/unknown-value-send.txt}}
```

When the endpoint takes the report, strata marks the lines as sent. A report has 100 entries or
fewer and 20 different values or fewer, so a long log goes in more than one report. Strata marks
the lines of each report that the endpoint took. The next report holds only the lines that came
after:

```text
{{#include ../output/unknown-value-send-marked.txt}}
```

A line of the log that is not valid does not stop the send. Strata skips the line and sends the
valid lines. The mark counts each line that is not blank, valid or not, as it did in release 0.27.0.
So a skipped line is under the mark after the send. Strata writes a warning that gives the count of
the skipped lines and the number of the first one.

`strata report` still stops on such a line. Delete the line to stop the warning. `--quiet` hides
the warning.

A line that you delete before the mark moves the mark. The next line moves up, and the mark covers
it, so the send skips it. For each line that you delete before the mark, lower the number in the
file `strata-unknown.sent` by 1.

A send never changes the output or the exit code of a run. When a report fails, strata marks no
line of it. Strata writes a warning, such as
`strata: warning: could not send the report to <address>: <reason>`. `--quiet` hides the warning.
The next send holds those lines again.

The endpoint opens 20 new issues in an hour for all senders. When the hour has no room for any
value of a report, the endpoint answers 429 with the time to wait. The next send after that time
works.

When the hour has room for some values, the endpoint answers 202 and lists the entries that wait.
Strata marks the lines before the first line of a value that waits, and it stops. It writes a
warning that gives the count of the lines that wait. The next send holds that line and each line
after it. So a value that the endpoint took after that line counts one more time. `--quiet` hides
the warning.

## What the endpoint keeps

The endpoint takes `strata-report/1` alone. It refuses a report with a field outside the list. It
refuses a value that is not one word of 64 printable ASCII characters or fewer, with no space. It
refuses a route that is not a template of a route, and a body of more than 64 KiB. See [The endpoint of the report](../development/report-endpoint.md) for the service.

The endpoint keeps these things, and no other:

- **One issue of strata for each set and value.** The issue has the label `unknown-value`. It holds
  the count of the lines and the count of the reports. It lists the releases of strata and the
  routes that the reports named. It holds no report. A later report of the same value changes the
  issue, and opens no new one.
- **A hash of the address of the sender, in memory, for one hour.** It limits the rate. An IPv6
  address counts by its /64 network. The process makes the salt of the hash when it starts, and
  writes the hash nowhere. The service drops an old hash soon after the hour, and a restart
  forgets it.
- **A hash of the key and the report, in memory, for one day.** A second try of the same lines
  counts one time. The service keeps the hash only for a report that it took whole. The hash holds
  no text of the report. The service drops it soon after the day.

The service writes no address and no report to its log. The maintainers also exclude the request
log of the platform, because that log holds the address. An issue stays until a maintainer deletes it.

## The files

All files are in the state folder: `$XDG_STATE_HOME/strata/`, or `~/.local/state/strata/` when the
variable is not set. See [Settings](./settings.md).

| File | What it holds |
| --- | --- |
| `strata-unknown.jsonl` | The log: one line of `strata-unknown/1` for each new value in a run. |
| `strata-unknown.sent` | The number of lines of the log that you marked as sent. |
| `strata-unknown.hint` | The time of the last hint, as a time in RFC 3339. |
| `strata-unknown.answer` | Your answer to the question of the send: `1` for yes, and `0` for no. |

Delete a file to start again. A mark that is past the end of the log counts for nothing, so the
report of a new log holds each line.

## The hint

The note of an unknown value can end with the words `strata report` shows what you can send. This
hint shows one time in a day at most. It shows only when standard error is a terminal. It never
shows with `--quiet` or `STRATA_QUIET`. A run that hides the hint does not record the time, so a
later run of that day can show it. A job in CI runs with no terminal, so it prints no hint.
