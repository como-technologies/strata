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
| `IDENTICAL` | The roots are equal. | yes |
| `SAME CONTENT` | The roots differ only by the tag name of a pack build, or by the uid that Palette gives each manifest pack. | yes |
| `REGISTRY MOVE` | The roots differ only by packs from another registry, with the same tag, version, values, manifests, and params. | only with `--allow registry-move` |
| `DIFFERENT` | Any other difference. | no |

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

## Refs

Each side is a ref: a snapshot file, a profile uid, `<name>@<version>`,
`<name>@latest`, or a cluster. Strata reads a uid, a name, or a cluster in the
configured project. To compare two projects, give two files.

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
