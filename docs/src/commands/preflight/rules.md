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

`--rule ID` runs only the rule with that id. Give it again for more rules.
An id that is not in the catalog is a `usage` error with exit `2`, and strata
sends no request. The message names the id and tells how to list the ids. A rule
that left the catalog is such an id. See [the checks that the route answers](../../reference/preflight-rules.md#the-checks-that-the-route-answers).

```text
{{#include ../../output/preflight-rule-unknown.txt}}
```

A rule that reads a list has an `each` in its check. The check of Palette makes one
request for each item of that list. Here the edge host rule reads each host of
each pool:

```text
{{#include ../../output/preflight-rules-each.txt}}
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

Strata reads the file before the first request. A file with a program that
cannot run fails with the kind `catalog` and exit `2`. Such a program does not
compile, reads `$item` where the check has no item, or nests more than 64
levels. A level is a bracket, or an operator in a chain such as `a | b | c`. The
programs of the catalog have fewer than 32 levels. See
[A request for each item](../../reference/preflight-rules.md#a-request-for-each-item).
