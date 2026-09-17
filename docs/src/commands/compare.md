# strata compare

Compare two refs: profile versions, snapshot files, clusters, or pack versions
from a registry. `strata A B` is the same as `strata compare A B`.

Two pack refs answer what changed between two versions of a pack:
[What changed between two pack versions](../use-cases/pack-versions.md).

The examples use the project `dev`, which holds the versions 1.0.0, 1.1.0,
1.1.1, and 1.1.2 of `aws-stack`. Version 1.1.2 is 1.1.1 with one changed value.

## Compare two versions

```text
{{#include ../output/compare-values.txt}}
```

Read the report from the top:

1. The two sides, with their uids.
2. `root`: the two roots and the result. `IDENTICAL`, `SAME CONTENT`, or
   `CHANGED`.
3. `consumers`: how many clusters used each side when strata read it. A cluster
   side shows `0`: a cluster has no consumers.
4. `variables`: the profile variables.
5. One line for each pack, in layer order. A changed pack shows its
   [change flags](../reference/impact.md): `IDENTITY`, `UPSTREAM`, `AUTHORED`.
   Below it are the changed values and the impact on running clusters.

Here a value of the CSI pack changed. The impact is `cluster-wide`: each node
gets the change.

## See the full diff

`--full` adds a unified diff of the raw values and of each changed manifest:

```text
{{#include ../output/compare-full.txt}}
```

The text report cuts a value longer than 120 characters. `--full` and the JSON
document keep each full value.

## A version that replaces nodes

```text
{{#include ../output/compare-repave.txt:1:25}}
...
```

`IDENTITY` on the OS or Kubernetes layer means a new pack build: Palette
replaces the nodes (`repave`). Schedule a maintenance window for this release.

## Compare two identical refs

```text
{{#include ../output/compare-identical.txt}}
```

Exit `0`.

## A pull request comment

`--format markdown` writes the same comparison as tables, with one
collapsible block for each changed pack:

````text
{{#include ../output/compare-markdown.txt:1:18}}
...
````

`scripts/pr-comment.sh` posts it: see
[Pull request comments](../automation/pr-comments.md).

## A script

`--json` writes [`strata-diff/1`](../reference/documents.md#strata-diff1).
`summary` gives the result in one object:

```text
{{#include ../output/compare-json.txt}}
```

## Notes in the report

A `note` line follows `root` in these cases:

| Note | Cause | Page |
| --- | --- | --- |
| `the roots differ only by the tag name a pack is pinned under` | One pack build under two tag names. The result is `SAME CONTENT`. | [One pack build under two tag names](../use-cases/tag-name.md) |
| `the roots differ only by the packUid Palette mints for each profile's manifest pack` | One manifest in two profiles. The result is `SAME CONTENT`. | [One manifest in two profiles](../use-cases/manifest-pack.md) |
| `both sides are <name>@<version> (uid <uid>): the version was edited in place` | One uid and one version, with two roots. | [A published version is edited in place](../use-cases/edited-in-place.md) |
| `uid <uid> is <name>@<a> on one side and <name>@<b> on the other` | One uid under two versions. Strata prints one note for each such uid, also under `IDENTICAL`. `verify` leaves it out when its verdict already names the overwrite. | [A version bump overwrites a version](../use-cases/overwritten.md) |
| `pack order changed between the two profiles` | The two sides apply their packs in a different order. | |

When a pack build has a new tag name and a manifest pack has a new uid, one note
names both. When either side is a cluster, the edit note adds `or changed on a
cluster`: a person can edit the pack values on a cluster.

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | The roots are equal. |
| `1` | The roots differ. `SAME CONTENT` also exits `1`. |
| `2` | Strata could not answer. |

## Options

```text
{{#include ../output/help-compare.txt}}
```
