# The rules and the custom probes

The catalog that a check runs, and a file of probes of your own.
[strata preflight](../preflight.md) gives the exit codes and the options.

## The rules

```text
{{#include ../../output/preflight-rules.txt}}
```

`--rules` lists each rule that applies to an engine, with its tier, its level,
its severity, its status, and its engines. `--rules --json` gives the same list
as a document, with the check of each rule, so a pipeline reads the catalog
without this book:

```text
{{#include ../../output/preflight-rules-json.txt}}
```

The rules are data in the binary, one file for each section under
`preflight/rules/`. [The rule format](../../reference/preflight-rules.md) states
the format.

## Custom probes

`--probes FILE` adds the rules of a file in the same format: the allowed
regions, the subnets of a landing zone, the tags that a policy needs, the cloud
account of each environment. A custom probe reads the same world document, runs
the same way, and follows the same promotion. Strata ships no custom probe and
assumes no topology.
