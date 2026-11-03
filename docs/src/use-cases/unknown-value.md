# Palette gives a value that strata does not know

## The problem

A job of the loop writes a note on standard error. It says that Palette gave a value that this
release of strata does not know. The job passes, and each document is right, because strata keeps
the text of the value. But your release of strata is out of date in one place, and the maintainers
of strata cannot see your tenant. They find the new value only when somebody tells them.

**Who runs this.** A person, when the note shows in a terminal or in the log of a job.

**Follow along.** `just playground unknown-value` serves this tenant. Before you run the commands,
set `XDG_STATE_HOME` to a new folder, so that the example keeps its own log:
`export XDG_STATE_HOME=$(mktemp -d)`. This release names the real endpoint of the maintainers. The
playground also prints `STRATA_REPORT_URL`, the address of a stand-in on your machine. Export it, so
that the send in step 5 reaches no real endpoint.

## 1. The note

The profile `aws-stack@1.1.12` has a type that this release does not know. Any command that reads
it writes the note:

```text
{{#include ../output/unknown-value-note.txt}}
```

Each run also appends a line to the log in the state folder. See
[The log of the unknown values](../reference/compatibility.md#the-log-of-the-unknown-values).

## 2. The report

`strata report` merges the equal lines of the log. Two runs noted the type, so the count is 2:

```text
{{#include ../output/unknown-value-report.txt}}
```

The report holds the fields of the log and a count. It holds no name and no uid. The profile
`aws-stack@1.1.12` is in the document, and it is not in the report. See
[What strata can report](../reference/report.md) for each field and for what strata never reports.

## 3. Read what you send

The markdown report is the text of the issue. Read it before you send it:

```text
{{#include ../output/unknown-value-report-markdown.txt}}
```

`strata report --link` prints the address of a new issue with this text in its body. Open it in a
browser. The browser shows the title and the body, and you send the issue from your own account.
With the link, strata sends nothing.

## 4. Mark the lines as sent

After you send the issue, mark the lines of the report as sent. The next report then holds only
the values that came after:

```text
{{#include ../output/unknown-value-report-sent.txt}}
```

Strata marks the lines after it printed the report or the link. A link that is too long gives an
error and marks nothing, so `strata report --format markdown` still holds the lines.

`strata report --all` holds each line of the log again:

```text
{{#include ../output/unknown-value-report-all.txt}}
```

## 5. Send the report by itself

To skip the steps 3 and 4, let strata send the report. The send is off by default. Set the
variable in the environment of the job or in `~/.palette.env`:

```bash
export STRATA_REPORT=1
```

At the end of a run that saw a new value, strata posts the report of the lines that you did not
mark as sent. The post has one try and a timeout of 3 seconds. It never changes the output or
the exit code of the run. When the endpoint takes the report, strata marks the lines as sent:

```text
{{#include ../output/unknown-value-send.txt}}
```

The report holds nothing new now:

```text
{{#include ../output/unknown-value-send-marked.txt}}
```

A run in a terminal asks you one question when no setting and no answer exist. It asks at the end
of the first run that wrote a line. Answer `y` to send. Any other answer, also an empty line,
refuses. Strata keeps your answer and asks no more.

A job in CI asks nothing. Set `STRATA_REPORT=1` in the job when you want the send. Set
`STRATA_REPORT=0` to stop the send and the question. The page
[What strata can report](../reference/report.md#send-the-report-by-itself) gives the question,
where strata asks and sends, and what the endpoint keeps.

## 6. What the maintainers do

A maintainer proves the value in the lab. Then the maintainer adds the value to the closed set of
strata, or writes why the set stays as it is. See
[What to do with a new value](../development/facts.md#what-to-do-with-a-new-value). Until a
release of strata names the value, your release keeps the text and writes the note.

A job in CI has no terminal, so it prints no hint. The log is in the state folder of the runner, so
save the folder when the runner does not stay.
