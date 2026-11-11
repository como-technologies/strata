# strata compare

Compare two refs: profile versions, snapshot files, clusters, or pack versions
from a registry. `strata A B` is the same as `strata compare A B`.

Two pack refs answer what changed between two versions of a pack:
[What changed between two pack versions](../use-cases/pack-versions.md).

The examples use the project `dev`, which holds the versions 1.0.0, 1.1.0,
1.1.1, and 1.1.2 of `aws-stack`. Version 1.1.2 is 1.1.1 with one changed value.

## What compare reads

One page for each kind of question, with its output and the options that it
takes:

| Page | What it holds |
| --- | --- |
| [Two versions](compare/versions.md) | What changed between two profile versions, and the impact: how to read the report, `--full`, the images and the charts of a pack, a version that replaces nodes, and a version that only changes its numbers. |
| [Two clusters](compare/clusters.md) | The tiers of two clusters, a cluster against the update that waits for it, a profile version against a cluster that runs many profiles, and a cluster group against a member. |
| [Ask Palette if the change repaves](compare/repave.md) | `--repave`: the answer of Palette for one cluster, beside the level of the table. |
| [Ask Palette if it accepts the packs](compare/validate.md) | `--validate`: Palette checks the packs of the change on one cluster. |
| [A pull request comment and a script](compare/scripts.md) | `--format markdown` for a pull request, and `--json` for a job. |

## Compare two identical refs

```text
{{#include ../output/compare-identical.txt}}
```

Exit `0`.

## Two sides in two projects

The line `project` follows `root` when the two sides are in two projects. It
names the project of each side by uid, or `tenant scope` for a side with no
project. `--project-a` and `--project-b`, or their names, read each side in its
own project:

```text
{{#include ../output/projects-compare.txt}}
```

`a.projectUid` and `b.projectUid` of the JSON name the same projects. See
[Environments in the projects of one tenant](../use-cases/projects.md).

## Notes in the report

A `note` line follows `root` in these cases:

| Note | Cause | Page |
| --- | --- | --- |
| `the roots differ only by the tag name a pack is pinned under` | One pack build under two tag names. The result is `SAME CONTENT`. | [One pack build under two tag names](../use-cases/tag-name.md) |
| `the roots differ only by the packUid Palette mints for each profile's manifest pack` | One manifest in two profiles. The result is `SAME CONTENT`. | [One manifest in two profiles](../use-cases/manifest-pack.md) |
| `both sides are <name>@<version> (uid <uid>): the version was edited in place` | One uid and one version, with two roots. | [A published version is edited in place](../use-cases/edited-in-place.md) |
| `<ref>@draft is the draft of <name>@<version> (uid <uid>): its publish makes this change` | One side is the draft of the version on the other side, and the roots differ. | [Review a draft before its publish](../use-cases/draft.md) |
| `the content uses macro <m>: the root covers its name and not its value, so an edit of a macro does not show` | The values or the manifests of either side name a user macro. | [Macros](snapshot/cluster.md#macros) |
| `macro <m> has another value on each side: ...` | Two records with the digests of their macro values, and a macro that differs. `compare` exits `1`, also when the roots are equal. | [Macros](snapshot/cluster.md#macros) |
| `pack <key> names image <a> on one side and <b> on the other: the root does not cover the annotations` | A pack names its machine image in `imageId` on both sides, and the two differ. `compare` exits `1`, also when the roots are equal. | [The OS pack names a new machine image](../use-cases/os-image.md) |
| `INVALID on side B: <reason>`, on the line of a pack | Palette marks the pack invalid on that side: the registry deleted its tag. `side A`, `side B`, or `both sides`. `summary.packsInvalid` counts these packs. The exit code does not change. | [A registry deletes the tag of a pack](../use-cases/invalid-pack.md) |
| `pack <key> has no upstream digest: Palette gives none, so a change of the archive does not show` | A pack from a registry with `upstreamSource: none`, on either side. A line under the pack says it too. | [The content digest](../internals/hashes.md#the-content-digest) |
| `pack <key> has a masked value at <path>: an equal root does not show an equal secret` | A pack whose values hold `********` on either side. A line `masked: <path>` under the pack says it too. `packs[].masked` in the JSON gives each path. | [A secret changes in a pack value](../use-cases/masked-value.md) |
| `the impact table says <level>, and Palette says <repave or no repave> for this cluster: the table does not describe its upgrade engine` | `--repave`, and the level and Palette's answer disagree. | [Ask Palette if the change repaves](compare/repave.md) |
| `cluster <c> is behind: Palette says an update of <profile> waits for it, since <time>` | A cluster side whose record says that an update waits. The copy is not the profile. | [Whether an update waits](snapshot/cluster.md#whether-an-update-waits) |
| `a repave of cluster <c> waits for approval, since <time>: <code> <pack> (<keys>). Palette refuses an update of its profiles until a person approves it` | A cluster side whose record says that a repave waits. Two sides that say the same give one note. | [Whether a repave waits](snapshot/cluster.md#whether-a-repave-waits) |
| `cluster <c> runs <pack> <version>, and its profile declares <version>` | A cluster side whose `running` tier holds another version of a pack than its profile. | [Two clusters on one release differ](../use-cases/cluster-tiers.md) |
| `uid <uid> is <name>@<a> on one side and <name>@<b> on the other` | One uid under two versions. Strata prints one note for each such uid, also under `IDENTICAL`. `verify` omits it when its verdict already names the overwrite. | [A version bump overwrites a version](../use-cases/overwritten.md) |

When a pack build has a new tag name and a manifest pack has a new uid, one note
names both. When either side is a cluster, the edit note adds `or changed on a
cluster`: a person can edit the pack values on a cluster.

One more note comes after the pack lines, not after `root`. The line
`note: pack order changed between the two profiles` says that the two sides
list their packs in another order. The order is content, whether or not a pack
changed:
[What Palette does](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
The markdown report writes the same line as a quotation.

## What to expect

| Exit | Meaning |
| --- | --- |
| `0` | The roots are equal, and no macro value or `imageId` differs. |
| `1` | The roots differ, or a macro value or an `imageId` differs. `SAME CONTENT` also exits `1`. |
| `2` | Strata could not answer. |

## Options

```text
{{#include ../output/help-compare.txt}}
```

| Option | What it does | Page |
| --- | --- | --- |
| `--full` | Adds a unified diff of the raw values and of each changed manifest. With `--json`, it adds the field `unified` to the document. | [See the full diff](compare/versions.md#see-the-full-diff) |
| `--repave` | Asks Palette if the change repaves the cluster side. One side must be a cluster. | [Ask Palette if the change repaves](compare/repave.md) |
| `--validate` | Asks Palette if it accepts the packs of the change on the cluster side. One side must be a cluster. | [Ask Palette if it accepts the packs](compare/validate.md) |
| `--project`, `--project-name` | Reads each side that is not a record file in that project, in place of the configured project. `--project ''` is tenant scope. | [Two sides in two projects](#two-sides-in-two-projects) |
| `--project-a`, `--project-name-a`, `--project-b`, `--project-name-b` | Reads side A, the first ref, or side B, the second ref, in that project. The flag of a side has priority over `--project`. | [Two sides in two projects](#two-sides-in-two-projects) |
| `--format`, `--json`, `--compact`, `--color`, `--no-color`, `-q` (`--quiet`) | The output options that each report command takes. | [Output formats](./index.md#output-formats) |
