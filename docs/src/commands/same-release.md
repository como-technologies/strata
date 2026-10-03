# strata same-release

Make sure that two or more records are one release. Strata compares each record
with the next one, in the order that you give.

The examples use the projects `dev` and `prod`, which each hold a copy of
`aws-stack@1.1.1`.

## Check a promotion

```text
{{#include ../output/promote-same.txt}}
```

Exit `0`: dev and prod have the same release. Give the records in promotion
order, for example `dev.json test.json prod.json`.

## A pair that fails

```text
{{#include ../output/promote-different.txt}}
```

Exit `1`. Each reason names the pack. For an `AUTHORED` change, the reason also
names the first changed value. See
[Promote a release to another project](../use-cases/promote.md).

## Which pairs pass

| Result | Cause | Passes |
| --- | --- | --- |
| `IDENTICAL` | The roots are equal, and no pack names another machine image. | yes |
| `SAME CONTENT` | The roots differ only by the tag name of a pack build, or by a pack uid that Palette mints. Palette mints the uid of each manifest pack, and the uid of a pack from a registry that it does not sync. A candidate does not have these uids. `mintedUidOnly` in the JSON names each such pack. | yes |
| `REGISTRY MOVE` | The roots differ only by packs from another registry, with the same tag, version, values, manifests, and params. The digest of each moved pack can differ. | only with `--allow registry-move` |
| `DIFFERENT` | Any other difference. | no |
| `NO DIGEST` | The pair passes on its roots, and a pack from a registry has no upstream digest on one side, so an equal root does not show an equal archive. | only with `--allow no-digest` |

```text
{{#include ../output/tag-same.txt}}
{{#include ../output/manifest-same.txt}}
{{#include ../output/move-allow.txt}}
```

A registry move can change the pack type, because the type follows the
registry: `spectro`, `helm`, or `oci`. A `manifest` pack comes from no
registry, so it never moves. Without `--allow registry-move`, the reason names
the move, so you can decide to allow it. See
[A pack comes from another registry](../use-cases/registry-move.md).

A move from a registry that gives a digest to one that gives none is a loss of
what the root can show. A pair that differs for another reason stays
`DIFFERENT`, and its reasons name each pack with no digest too. `--allow` takes
more than one value: `--allow registry-move --allow no-digest`. See
[The content digest](../internals/hashes.md#the-content-digest).

## A pack names another machine image

The root does not cover the annotations of a pack. So two records can have one
root, and their OS pack can name two machine images in `imageId`. Strata
compares the `imageId` of each pack that the two records hold. When the two
images differ, the pair is `DIFFERENT` under each policy, also when the roots
are equal. The reason names the pack and the two images:

```text
{{#include ../output/os-image-same-release.txt}}
```

A side with no `imageId` is not compared. A record written before strata kept
the annotations has none. See
[The OS pack names a new machine image](../use-cases/os-image.md).

## A macro has another value in each project

The values and the manifests of a version can read a macro, as
`{{.spectro.macro.<name>}}`. Each project gives the macro its own value. The
root covers the name of the macro and not its value. So one release can run
with a different configuration in each project.

Record each side with `snapshot --macros`. Then each record holds the digests
of its macro values, and strata compares them. A macro with another value on
each side gets a line under its pair:

```text
{{#include ../output/promote-macro-changed.txt}}
```

The line does not change the result, and the exit stays `0`. The release is the
text, and each project gives the value. `pairs[].macrosChanged` in the JSON
names each macro:

```text
{{#include ../output/promote-macro-changed-json.txt}}
```

When a record holds no digests, strata cannot compare the values. The line
under the pair names each macro that the content uses.
`pairs[].macrosNotCompared` in the JSON gives the same names:

```text
{{#include ../output/promote-macro-not-compared.txt}}
```

See
[A macro has another value in each project](../use-cases/promote.md#a-macro-has-another-value-in-each-project).

## A masked value

A pack whose values hold `********` gets a line under its pair: `pack <key> has
a masked value at <path>: an equal root does not show an equal secret`. The line
does not change the result, and `pairs[].masked` in the JSON gives the same
fact. See [A secret changes in a pack value](../use-cases/masked-value.md).

## Refs

Each side is a ref: a snapshot file, a profile uid, `<name>@<version>`,
`<name>@latest`, a cluster, or `pack:<name>@<version>`. Strata reads a uid, a
name, or a cluster in the configured project. To compare two projects, give two
files. [Refs](../reference/refs.md) lists each form.

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | Each pair passes. |
| `1` | A pair fails. |
| `2` | Strata could not read a record. |

`--json` writes [`strata-chain/1`](../reference/documents.md#strata-chain1).

## Options

```text
{{#include ../output/help-same-release.txt}}
```

`--format`, `--json`, `--compact`, `--color`, `--no-color`, and `-q`
(`--quiet`) are the output options that each report command takes: see
[Output formats](./index.md#output-formats).
