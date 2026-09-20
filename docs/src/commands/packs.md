# strata packs

`strata packs <name>` lists the versions that the registries offer for one
pack, newest first, with the tags that point at each version.

```text
{{#include ../output/packs-list.txt:1:8}}
...
```

The columns are the version, the pack uid of that build, the pack type, the
clouds it supports, and every tag that points at it. The exact tag comes first,
then the floating tags above it: `1.65.0`, then `1.65.x`, then `1.x`.

## Which version is the newest

```text
{{#include ../output/packs-latest.txt}}
```

`--latest` keeps one line for each registry that has the pack. This is the
question a bump asks: the pin is current when it names the version that
`--latest` prints.

## One registry, or one cloud

```text
{{#include ../output/packs-cloud.txt}}
```

| Option | Function |
| --- | --- |
| `--registry NAME` | Only this registry, by its name or its uid. |
| `--cloud TYPE` | Keep the versions that support this cloud, such as `aws`. |
| `--latest` | Only the newest version of each registry. |

A cloud that no version supports is an answer, and an empty one: exit `0` and
one line. A registry the key cannot see is an error that lists the visible
registries, and a pack that no registry has is `pack-not-found`. Both exit `2`.

## For a script

```text
{{#include ../output/packs-json.txt}}
```

`--json` writes [`strata-packs/1`](../reference/documents.md#strata-packs1).
`strata schema packs` prints its JSON Schema.

## What to expect

Strata reads the paged pack listing for the versions, their digests and their
types, and each registry's own answer for the tags. The listing repeats a
version across pages, and `packs` counts it once. A pack with more than a
hundred versions lists completely: `kubernetes` in `Public Repo` had 133
versions on [run `20260918T022255Z`](../development/live-runs.md#packssh).

| Exit | Meaning |
| --- | --- |
| `0` | The listing, which can be empty when `--cloud` keeps nothing. |
| `2` | Strata could not answer: no such registry, or no registry has the pack. |

## Options

```text
{{#include ../output/help-packs.txt}}
```
