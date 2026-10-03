# A record with no network

A record can come from the saved answers of Palette, or from a candidate file
of a version that is not in Palette yet. Neither needs a key.

## The saved answers of Palette

`--raw-dir` saves the API responses and the time of the read. `--from-raw`
builds the same snapshot from them later, with no network and no key. The two
options exclude each other. An optional ref after `--from-raw` fills
`source.ref`. For a directory of a cluster, a ref of the form
`cluster:<uid>/<profile>` also selects that one profile. Strata checks the ref
against nothing else in the directory. The record keeps the time of the read,
not the time of the rebuild, so `verify` reports the version as unchanged since
Palette answered:

```text
{{#include ../../output/snapshot-raw.txt}}
```

`scope.json` keeps the project. A directory with no `scope.json` builds a
snapshot at tenant scope.

`notifications.json` keeps the notifications of a cluster. Palette serves the
full values of a pack in each event, and these values can hold a password.
Before the save, strata writes `********` in place of each `packValues`,
`appTierValues`, and `appTierProperties` that is not empty. Strata masks all of
the values, not only the fields that `passwordFields` names. Strata reads none
of these fields, so `--from-raw` builds the same record.

`repave.json` keeps the repave that waits for approval, when one waits. Each
reason of Palette holds the values of the pack on each side of the change, with
the value of each macro in its place. A macro value can be a secret. Before the
save, strata writes `********` in place of each value that is not empty. Strata
reads only the code, the message, the pack name, and the keys, so `--from-raw`
builds the same record.

## A version before you publish it

`--from-candidate` builds a snapshot from a candidate file. See
[strata candidate](../candidate.md) and
[Review a change before you publish it](../../use-cases/review.md).
