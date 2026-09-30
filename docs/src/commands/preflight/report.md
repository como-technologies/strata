# The report

The report as a document for a job, and as files on disk for a person who did
not run the command. [strata preflight](../preflight.md) gives the exit codes
and the options.

## The report as JSON

```text
{{#include ../../output/preflight-json.txt}}
```

One entry for each rule that applies: `id`, `tier`, `level`, `severity`,
`verdict`, `messages`, `elapsedMs`, `ran`, `reproduce`, `fix`, `evidence`, and
`confidence`, the counts of the runs of the rule on this engine with the 95%
lower bound of each rate. The summary and the section scorecard:

```text
{{#include ../../output/preflight-json-summary.txt}}
```

[JSON documents](../../reference/documents.md#strata-preflight1) lists each field.
`strata preflight --schema report` prints the JSON Schema.

## The report on disk

`--out DIR` writes the report in three forms, and the two files that the
`reproduce:` lines read:

```text
{{#include ../../output/preflight-out.txt}}
```

`report.html` is one page with the scorecard, each result, and the reproduce
lines, for a person who did not run the command. `world.json` is the document
each rule reads, with no secret in it; `entity.json` is the body of the create.
