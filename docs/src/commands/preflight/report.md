# The report

The report as a document for a job, and as files on disk for a person who did
not run the command. [strata preflight](../preflight.md) gives the exit codes
and the options.

## The report as JSON

```text
{{#include ../../output/preflight-json.txt}}
```

One entry for each rule that applies: `id`, `tier`, `level`, `severity`,
`verdict`, `messages`, `elapsedMs`, `ran`, `requests`, `reproduce`, `fix`, `evidence`, and
`confidence`, the counts of the runs of the rule on this engine with the 95%
lower bound of each rate. The summary and the section scorecard:

```text
{{#include ../../output/preflight-json-summary.txt}}
```

A check that gave `error` at a request has `failedRequest`, the request to
Palette that failed. One denial of the key stops each check that sends the
same request. So the terminal, the markdown, and the HTML form show the error
once, under `Requests that failed`, with the checks that it stopped. The JSON
keeps the messages of each check. See [The key of the live
tier](./tiers.md#the-key-of-the-live-tier).

The summary puts each result in one count. `failed` is the counted failures, `probeFailed` the probes
that failed, and `warned` the warnings. Each line of the scorecard has the same three counts, and
the counts of the sections add to the counts of the summary. `counted` is the number of counted
checks that ran: `0` gives the verdict `NOT COUNTED`.

[JSON documents](../../reference/documents.md#strata-preflight1) lists each field.
`strata preflight --schema report` prints the JSON Schema.

## The report on disk

`--out DIR` writes the report in three forms:

```text
{{#include ../../output/preflight-out.txt}}
```

`report.html` is one page with the scorecard, each result, and the reproduce
lines, for a person who did not run the command. `report.md` and `report.json`
are the markdown and the JSON forms.

`--world` also writes `world.json` and `entity.json`, which the `reproduce:`
lines read. These two files hold the values of the variables. Here the cluster
`payments` gives its variable `platform_owner` the value `team-payments`:

```text
{{#include ../../output/preflight-out-values.txt}}
```

| File | What it holds |
| --- | --- |
| `report.md`, `report.json`, `report.html` | The results, their messages, and the reproduce lines. The message of a check of Palette holds the code and the message of the answer of Palette. The message of an error holds the start of the body of the answer. A message holds a mark in place of each value of a variable of the entity: see [The values in a message](#the-values-in-a-message). |
| `entity.json` | The body of the create. For a file, it is the body in the file, merged with the change when the command has one. For `cluster:UID`, strata makes the body from the cluster. It holds the name, the labels, and the annotations, the cloud config, the pools, and the values of the packs of each profile. Each profile also holds the `name` and the `value` of each of its variables. Strata replaces no field of this file. |
| `world.json` | The document that each rule reads. It holds the entity, the entity before a change, the records of the profiles, the packs, and the cloud account. It also holds `validateBody`, the body for the validate route, and with `--watch` the last reading. So it holds each value of a variable in `entity` and in `validateBody`, and in `before` with a change. Strata writes `<redacted>` in place of each field that holds a credential, such as `secretKey`, `clientSecret`, or `password`, and in place of `externalId`. |

For `cluster:UID`, Palette gives the value of a variable that is not sensitive,
and a mask in place of a sensitive value. So the two files hold the mask of a
sensitive variable, and the value of each other variable. See
[A cluster as Palette holds it](./tiers.md#a-cluster-as-palette-holds-it).
A pack value can hold a secret too, and so can a record: see
[Where to keep a record](../snapshot/profile-version.md#where-to-keep-a-record).

> **Give `--world` only when each person who can read the folder can see the
> values of the variables.** A job keeps the folder as an artifact, and each
> reader of the job can download it. See
> [The files of the job](../../automation/preflight.md#the-files-of-the-job).

A reproduce line names the folder of `world.json` and `entity.json`. It writes
`request.json` and `body.json` into that folder, beside the values, and not into
the current directory. `request.json` is a body for a validate route, and
`body.json` is the answer of Palette. Strata does not read an answer for a
value, so it keeps both files in the one place that holds the values.

When the run did not write the two files, the lines name the folder `$OUT`. The
last line of the report then says how to write them. Run the command again with
`--out DIR --world`. Then run the lines with `OUT=DIR`.

The terminal, the markdown, and the HTML form write a failed rule with the same
fix, as `fix: <text> (<owner>)`. A rule that is not checked, or that gave an error,
found no fault, so it shows no fix. The three forms write the confidence of a rule
in the same words: `catch <rate>, clean <rate>`.

## The values in a message

Palette can quote the value of a cluster variable in a refusal. It puts the
value in place of each `{{.spectro.var.NAME}}` reference before it checks the
body. For example, these refusals quote such a value:

| Code | What the message quotes |
| --- | --- |
| `K8SCidrOverlap` | The pod CIDR or the service CIDR of the Kubernetes pack, when it overlaps the CIDR of the VPC. |
| `CidrParseError` | A value that is not a CIDR. |
| `InvalidDropDownVariable` | A label that is not an option of a dropdown variable. |

So strata writes a mark in place of each value of a variable of the entity, in
each message of the report. The mark names the variable, such as
`[value of pod_cidr]`. Each form of the report has the mark: the terminal, the
markdown, the JSON, and the HTML. The values of the entity before a change get
a mark too. To see a value, give `--world` and read `entity.json`.

The message of an error holds the start of the body of the answer. Strata
writes the marks into the body before it cuts the body. So the cut leaves no
part of a value.

The mask matches whole values only. A text is part of a longer word when a
letter or a digit touches it. A `-`, `_`, `.`, `/`, `:`, or `@` beside it counts
as a letter when a letter or a digit is on its other side. So the
value `3` gets a mark in `size 3` and in `'3'`, and not in `t3.xlarge`,
`HTTP 403`, or `10.0.0.3`. A word that is the same as a value gets the mark
too, also when it comes from another field. A value with no letter and no digit
gets no mark, such as the mask `********` of a sensitive variable.

The lines of `--verbose` go to standard error, and they can hold a value of a
variable. In a job, they go to the log: see
[The files of the job](../../automation/preflight.md#the-files-of-the-job).
