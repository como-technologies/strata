# What changed between two pack versions

## The problem

Palette ships a new version of a pack, and a bump pull request moves a profile
to it. Before the review, you want to know what the pack itself changed: the
images it runs, the values it ships, and whether the change replaces nodes. The
Palette UI shows the values of each version next to each other. It writes no
record, so nothing of that view is left when you close the page.

A pack version is a ref: `pack:<name>@<version>`. Strata reads it from the
registry and hashes it as it hashes that pack inside a profile.

## 1. Compare two versions

```text
{{#include ../output/pack-compare.txt}}
```

`IDENTITY` says the layer points at another pack build, `UPSTREAM` that the
content digest moved, and `AUTHORED` that the values the registry ships
changed. The lines under the values name each changed key. Here they are the
image tags of the pack, its list of images, and one configuration file. Exit
`1`: the versions differ.

A version bump with no values change shows the first two flags alone, and the
impact of the layer:

```text
{{#include ../output/pack-os.txt}}
```

## 2. Name the registry when two registries have the pack

```text
{{#include ../output/pack-ambiguous.txt}}
```

Exit `2`. Registries hold copies of the same pack, so a ref that names none is
an error that lists them. `pack:<registry>/<name>@<version>` picks one, by the
registry's name or its uid.

Comparing the two copies says whether they are the same build:

```text
{{#include ../output/pack-registries.txt}}
```

`IDENTITY` and `UPSTREAM` with no `AUTHORED`: the same values from another
registry, under another pack uid and digest. That is the pattern of
[A pack comes from another registry](./registry-move.md).

## 3. Which versions the registries have

```text
{{#include ../output/packs-latest.txt}}
```

`strata packs <name> --latest` gives the newest version in each registry, with
the tags that point at it, which is the question a bump asks: is the pin the
current version? Without `--latest` the command lists every version. See
[packs](../commands/packs.md).

## 4. Record a version, and check it later

```text
{{#include ../output/pack-record.txt}}
```

`pack:<name>@latest` takes the highest version in the registry. The record is
a snapshot like any other: keep it, and `verify` reads the registry again.

```text
{{#include ../output/pack-verify.txt}}
```

Exit `0`: the registry still serves that content under that version. `DRIFT`
and exit `1` say the pack version changed after you recorded it, which a
published build must never do.

## What a pack record does not carry

A pack record holds the values the registry ships, not the values a profile
sets on top of them, and no manifests or params, because those belong to a
profile. To compare what a profile does with a pack, compare the profile
versions: [Review a change before you publish it](./review.md).

## What to do

- Pin the version, not a floating tag, when the build must not move. A tag can
  point at another build later.
- Record the pack, and verify the record on a schedule. `verify` reads the
  registry again, and answers `DRIFT` with exit `1`.
- Name the registry with `--registry` when two registries hold the pack name.

## Palette behavior

A registry answers `@latest` with the highest version it holds, and `strata
packs` lists the versions newest first, with the tags that point at each. Two
registries can hold one pack name. See
[What a registry answers about a pack](../internals/palette.md#what-a-registry-answers-about-a-pack).
