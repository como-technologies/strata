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

`--latest` keeps one line for each registry that has the pack. That line is the
version that `pack:<name>@latest` records: the highest version that is not a
pre-release. This is the question a bump asks: the pin is current when it names
that version.

A pre-release is below its version. A registry that holds pre-releases alone
gives its highest pre-release. `strata behind` does not name a pre-release as a
newer version of a pin that is not a pre-release: see
[A pre-release](./behind.md#a-pre-release).

Palette marks one version of each pack in each registry as the latest. Palette
orders by semantic version, and strata does too: a pre-release or a rebuild such
as `1.21.1-rev1` is below `1.21.1`. So the version that strata marks is the
version that Palette marks:

```text
{{#include ../output/packs-palette-latest.txt}}
```

The JSON gives `paletteLatest: true` on the version that Palette marks. Strata
omits the field on each other version:

```text
{{#include ../output/packs-palette-latest-json.txt}}
```

`--latest` keeps one line for each registry. It keeps a second line, with the
mark `latest in Palette`, only when Palette marks another version. This can
happen in two cases. The highest version is a pre-release, such as `1.31.0-rc.1`
above `1.29.0`: `@latest` skips it, and Palette marks the highest version. Or a
version does not parse as a semantic version, such as a version with four
numbers.

`strata behind` names the version of Palette on its bump line: see
[The pins that are behind](./behind.md#the-pins-that-are-behind). See
[The version that Palette calls the latest](../internals/palette/packs.md#the-version-that-palette-calls-the-latest).

## One registry, or one cloud

```text
{{#include ../output/packs-cloud.txt}}
```

| Option | Function |
| --- | --- |
| `--registry NAME` | Only this registry, by its name or its uid. |
| `--cloud TYPE` | Keep the versions that support this cloud, such as `aws`. |
| `--latest` | Only the version that `@latest` names in each registry, and the version that Palette calls the latest when it is a different one. |

A version supports a cloud by the rule of Palette for a pack in a profile: see
[A pack that supports a cloud](../internals/palette/packs.md#a-pack-that-supports-a-cloud).
So `--cloud aws` keeps a version with the cloud type `all`, and a version with
no cloud type. `--cloud all` keeps each version. The JSON keeps `cloudTypes` as
Palette gives it.

A cloud that no version supports is an answer, and an empty one: exit `0` and
one line, `no version of <name> supports <cloud>`. Without `--cloud`, a pack
with no version that Palette serves is an empty answer too: `Palette serves no
version of <name>`.
One example is a pack with no FIPS compliant version.

A registry the key cannot see is an error that lists the visible registries. A
pack that no registry has is `pack-name-not-found`. With `--registry`, a pack
that this registry does not have is `pack-not-found`. Each exits `2`.

When one of the registries that strata asked is a registry that Palette does
not sync, the error names it. Palette lists a pack of such a registry only while
a profile pins the pack: see
[A pack that Palette makes at the create](../internals/palette/packs.md#a-pack-that-palette-makes-at-the-create).

```text
{{#include ../output/nodigest-chart-unsynced.txt}}
```

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

A version that Spectro Cloud retired has its state `deprecated`, `disabled`, or
`deleted` in the last column of the terminal. In markdown, the state is after
the version. When every version in a registry has the same state, the heading
says it too. The answer for the tags does not include the disabled and the
deleted versions. So when the listing shows such a version, strata asks again
with the `states` query, and each version gets its tags.

A version that Palette refuses in a profile, with the mark `disabled` of
Spectro Cloud, also says `disabled`. Palette serves its tags, so strata asks
nothing more for it. A profile cannot hold a disabled or a deleted version: see
[Pack states](../internals/palette/packs.md#pack-states).

A registry can answer 404 for the tags with and without the query. That answer
has no version. So strata lists only the disabled and the deleted versions of
the listing, with no tags. The heading says the state and `no tags`, or
`no tags: the registry answered 404` when the versions differ in state. The
other registries show as usual, and any other failure of that read stops the
command. The live example
[checks each state](../development/live-runs.md#pack-statessh).

Some registries give no digest for a pack: a Helm registry with
synchronization off, and an OCI registry with the provider type `zarf`. A
version with no digest has `no digest` in the last column of the terminal, and
after the version in markdown. When no version in the registry has a digest,
the heading says `no digests` too:

```text
{{#include ../output/nodigest-chart-packs.txt}}
```

Strata cannot see a change of the archive of such a version. `verify` reports
a record of it as `UNVERIFIED`, and `same-release` refuses it without
`--allow no-digest`. See [A pack has no digest](../use-cases/no-digest.md). The
live example
[checks a pack of a Zarf registry](../development/live-runs.md#registry-kindssh).

Palette applies the FIPS rule of the tenant to the answer of the registry, and
not to the listing. So in a FIPS tenant, the listing holds a version that
Palette refuses in a profile, and the registry gives that version no tag.
Strata omits each version that has no tag in the answer of the registry,
unless Spectro Cloud retired it as `disabled` or `deleted`. When no version
of the pack is FIPS compliant, the registry answers 404, and strata shows no
version of that registry. The count of versions in the heading does not
include a version that strata omits. See
[The FIPS rule](../internals/palette/packs.md#the-fips-rule).

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
