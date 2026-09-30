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

## A version before you publish it

`--from-candidate` builds a snapshot from a candidate file. See
[strata candidate](../candidate.md) and
[Review a change before you publish it](../../use-cases/review.md).
