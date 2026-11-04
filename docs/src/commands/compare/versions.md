# Two versions

Two profile versions, two records of them, or two pack versions: what changed,
and the impact on the clusters that run it. [strata compare](../compare.md)
gives the other questions, the notes, what to expect, and the options.

## Compare two versions

```text
{{#include ../../output/compare-values.txt}}
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
   [change flags](../../reference/impact.md): `IDENTITY`, `UPSTREAM`, `AUTHORED`.
   Below it are the changed values and the impact on running clusters.

Here a value of the CSI pack changed. The impact is `cluster-wide`: each node
gets the change.

## See the full diff

`--full` adds a unified diff of the raw values and of each changed manifest:

```text
{{#include ../../output/compare-full.txt}}
```

The text report cuts a long value. `--full` and the JSON document keep each full
value. [Report formats](../../reference/formats.md#the-terminal-report) gives the
length at which the report cuts.

## The images and the charts of a pack

The values of a pack declare the images that it pulls, `pack.content.images`,
and an add-on pack the Helm charts, `pack.content.charts`. When one of those
lists changes, the pack shows `IMAGES` or `CHARTS` beside `AUTHORED`, a line
names each image or chart that moved, and an `images` line under `root` counts
them across the packs:

```text
{{#include ../../output/pack-compare-images.txt}}
```

Between two profile versions, `--json` counts them in `summary`, and flags each
pack:

```text
{{#include ../../output/compare-images-count.txt}}
```

Palette mirrors the images of a pack version under a path that holds the
version, such as `…/packs/csi-aws-ebs/1.65.0/aws-ebs-csi-driver`. Strata reads a
path segment that is the version of the pack as `<version>`, so one image keeps
one repository across versions, and a version bump shows each image as
retagged:
`~ image …/csi-aws-ebs/<version>/aws-ebs-csi-driver: v1.63.1 -> v1.65.0`.

The list holds the text that the pack names. An image pushed again under one
tag does not show: see
[A vendor rebuilds an image under one tag](../../use-cases/image-tag.md).

## A version that replaces nodes

```text
{{#include ../../output/compare-repave.txt:1:25}}
...
```

`IDENTITY` with a new version on the OS or Kubernetes layer means a new pack
build: on a kubeadm cluster, Palette replaces the nodes (`repave`). Schedule a
maintenance window for this release. The level is a hint from a table that
describes one upgrade engine: see
[Ask Palette if the change repaves](repave.md). `IDENTITY` with the same pack
uid and another tag name replaces no node: see
[One pack build under two tag names](../../use-cases/tag-name.md).
