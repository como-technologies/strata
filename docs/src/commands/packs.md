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

A version with the cloud type `all` supports every cloud, so `--cloud aws` keeps
it. Palette gives `all` to a pack that supports every cloud, and its own filter
on one cloud keeps that pack: see
[The cloud type `all`](../internals/palette/packs.md#the-cloud-type-all). The
JSON keeps `cloudTypes` as Palette gives it.

A cloud that no version supports is an answer, and an empty one: exit `0` and
one line. A registry the key cannot see is an error that lists the visible
registries. A pack that no registry has is `pack-name-not-found`. With
`--registry`, a pack that this registry does not have is `pack-not-found`. Each
exits `2`.

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
versions, which [the live example checks](../development/live-runs.md#packssh).

A version that Spectro Cloud retired has its state in the last column:
`deprecated`, `disabled`, or `deleted`. When every version in a registry has the
same state, the heading says it too. The answer for the tags does not include
the disabled and the deleted versions. So when the listing shows such
a version, strata asks again with the `states` query, and each version gets its
tags. A profile cannot pin a disabled or a deleted version: see
[Pack states](../internals/palette/packs.md#pack-states).

A registry can answer 404 for the tags with and without the query. Strata then
lists the versions from the listing with no tags. The heading says the state
and `no tags`, or `no tags: the registry answered 404` when the versions have no
state. The other registries show as usual, and any other failure of that read
stops the command. The live example
[checks each state](../development/live-runs.md#pack-statessh).

| Exit | Meaning |
| --- | --- |
| `0` | The listing, which can be empty when `--cloud` keeps nothing. |
| `2` | Strata could not answer: no such registry, or no registry has the pack. |

## Options

```text
{{#include ../output/help-packs.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
