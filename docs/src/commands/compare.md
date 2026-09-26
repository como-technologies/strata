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
3. `consumers`: how many clusters used each side when strata read it. Only a
   profile version has consumers. Against a cluster or a pack, the line gives
   the profile side alone, and it tells you if that cluster is one of the
   consumers. Between two clusters, or two packs, there is no line.
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

The text report cuts a long value. `--full` and the JSON document keep each full
value. [Report formats](../reference/formats.md#the-terminal-report) gives the
length at which the report cuts.

## The images and the charts of a pack

The values of a pack declare the images that it pulls, `pack.content.images`,
and an add-on pack the Helm charts, `pack.content.charts`. When one of those
lists changes, the pack shows `IMAGES` or `CHARTS` beside `AUTHORED`, a line
names each image or chart that moved, and an `images` line under `root` counts
them across the packs:

```text
{{#include ../output/pack-compare-images.txt}}
```

Between two profile versions, `--json` counts them in `summary`, and flags each
pack:

```text
{{#include ../output/compare-images-count.txt}}
```

Palette mirrors the images of a pack version under a path that holds the
version, such as `…/packs/csi-aws-ebs/1.65.0/aws-ebs-csi-driver`. Strata reads a
path segment that is the version of the pack as `<version>`, so one image keeps
one repository across versions, and a version bump shows each image as
retagged:
`~ image …/csi-aws-ebs/<version>/aws-ebs-csi-driver: v1.63.1 -> v1.65.0`.

The list holds a tag, and not a digest: an image pushed again under one tag
does not show. See [The change flags](../reference/impact.md#the-change-flags).

## A version that replaces nodes

```text
{{#include ../output/compare-repave.txt:1:25}}
...
```

`IDENTITY` with a new pack uid on the OS or Kubernetes layer means a new pack
build: on a kubeadm cluster, Palette replaces the nodes (`repave`). Schedule a
maintenance window for this release. The level is a hint from a table that
describes one upgrade engine: see
[Ask Palette if the change repaves](#ask-palette-if-the-change-repaves). `IDENTITY` with the same pack uid and another tag name replaces no
node: see
[One pack build under two tag names](../use-cases/tag-name.md).

## Ask Palette if the change repaves

The `repave` level comes from a table, and Palette does not use that table. For
a cluster, Palette answers the question itself. `--repave` sends it the
change from what the cluster runs to a profile version, and prints the answer
beside the level. One side must be a cluster, and the other a profile version
that Palette holds, in either order:

```text
{{#include ../output/repave-kubeadm-lines.txt}}
```

The `palette` line gives Palette's reason: the code, the pack, and the config
keys that differ. Strata does not compare the values: Palette does. The key
`kubeadmconfig.postKubeadmCommands` is on its list, and a change of a key that
is not on it repaves nothing. A change of the CSI values does not:

```text
{{#include ../output/repave-csi-lines.txt}}
```

`--json` carries the answer as `paletteRepave`, with `required`, each reason,
and `agrees`:

```text
{{#include ../output/repave-kubeadm-json.txt}}
```

`agrees` is `false` when the level says `repave` and Palette says no, or when the
level says `in-place`, `sync`, or `none` and Palette says repave. A level of
`may-repave` or `cluster-wide` says that a repave is possible, so it agrees with
both answers. When the two disagree, the report adds a `note`. Here the
Kubernetes pack moves to another version, and Palette answers as it does for an
Edge cluster, where that change repaves nothing:

```text
{{#include ../output/repave-disagree-lines.txt}}
```

That note is the signal that the table is wrong for the environment. No live
run has produced it: AWS and GCP, the two clouds that the runs used, have one
upgrade engine, and give the answer of the table each time. It costs two requests, the cluster document and one `POST` to
`/v1/spectroclusters/<uid>/validate/repave`. The `POST` asks a question and
changes nothing: the cluster keeps its instances.

Two profile versions, or two clusters, are a usage error, because Palette
answers for one cluster. So are an edge host and a cluster group: Palette
answers for a cluster, and not for them:

```text
{{#include ../output/repave-usage.txt}}
```

## Compare two clusters by more than the release

Two cluster records that carry tiers are compared on each tier that both carry,
and on their labels. A `cluster tiers` line names the state of each, and a line
for each field that differs, as `<tier>.<path>`:

```text
{{#include ../output/tiers-differ-lines.txt}}
```

A tier difference does not change `same`, and does not change the exit: the
root says whether the release is the same. `--json` carries each tier as
`clusterTiers`, and a pack whose reconciled version differs from its profile as
`runningBehind`. See
[More of the cluster than its release](snapshot.md#more-of-the-cluster-than-its-release).

## Compare a cluster with the update that waits

`cluster:<cluster>@pending` is the cluster after it takes the update that waits.
Compare it with the cluster to see what the update changes before anybody
accepts it:

```text
{{#include ../output/edited-pending.txt}}
```

See [What a waiting update changes](snapshot.md#what-a-waiting-update-changes).

## Compare a cluster group with a member

When one side is a cluster group and the other is a `cluster:` ref that selects
no profile, the cluster side holds only the profiles that the group pins. The
question is whether the member runs the pin, and not what else it runs:

```text
{{#include ../output/group-compare.txt}}
```

`tenant-a` runs one more profile of its own, and the comparison leaves it out.
A `cluster:<cluster>/<profile>` ref is not narrowed. See
[A cluster group pins a profile](../use-cases/cluster-groups.md).

## Compare two identical refs

```text
{{#include ../output/compare-identical.txt}}
```

Exit `0`.

## A pull request comment

`--format markdown` writes the same comparison as tables, with one
collapsible block for each changed pack:

```text
{{#include ../output/compare-markdown.txt:1:2}}
```

{{#include ../output/compare-markdown.txt:3:}}

`scripts/ci/pr-comment.sh` posts it: see
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
| `the content uses macro <m>: the root covers its name and not its value, so an edit of a macro does not show` | The values or the manifests of either side name a user macro. | [Macros](./snapshot.md#macros) |
| `macro <m> has another value on each side: ...` | Two cluster records with the digests of their macro values, and a macro that differs. `compare` exits `1`, also when the roots are equal. | [Macros](./snapshot.md#macros) |
| `pack <key> has no upstream digest: Palette gives none, so a change of the archive does not show` | A pack from a registry with `upstreamSource: none`, on either side. A line under the pack says it too. | [The content digest](../internals/hashes.md#the-content-digest) |
| `the impact table says <level>, and Palette says <repave or no repave> for this cluster: the table does not describe its upgrade engine` | `--repave`, and the level and Palette's answer disagree. | [Ask Palette if the change repaves](#ask-palette-if-the-change-repaves) |
| `cluster <c> is behind: Palette says an update of <profile> waits for it, since <time>` | A cluster side whose record says that an update waits. The copy is not the profile. | [Whether an update waits](./snapshot.md#whether-an-update-waits) |
| `cluster <c> runs <pack> <version>, and its profile declares <version>` | A cluster side whose `running` tier holds another version of a pack than its profile. | [Two clusters on one release differ](../use-cases/cluster-tiers.md) |
| `uid <uid> is <name>@<a> on one side and <name>@<b> on the other` | One uid under two versions. Strata prints one note for each such uid, also under `IDENTICAL`. `verify` leaves it out when its verdict already names the overwrite. | [A version bump overwrites a version](../use-cases/overwritten.md) |

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
| `0` | The roots are equal. |
| `1` | The roots differ. `SAME CONTENT` also exits `1`. |
| `2` | Strata could not answer. |

## Options

```text
{{#include ../output/help-compare.txt}}
```
