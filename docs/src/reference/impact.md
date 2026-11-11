# Change flags and impact

## The change flags

A changed pack shows one or more flags. Each flag comes from one hash of the
pack. See [The hashes and the root](../internals/hashes.md).

| Flag | Changes when | Meaning for the release |
| --- | --- | --- |
| `IDENTITY` | the type, tag, version, pack uid, or registry uid of the pack changes | The release points at another pack build. The name and the layer are the key of the pack, so a change to either gives an `ADDED` line and a `REMOVED` line. |
| `UPSTREAM` | the digest on the pack reference changes | The content of the pack changed under the same version. A push again under that version does this, and a floating tag that the registry moved does it too. See [A pack version pushed again](../internals/palette/packs.md#a-pack-version-pushed-again) and [Tag names](../internals/palette/packs.md#tag-names). A pack whose `upstreamSource` is `none` never shows this flag. |
| `AUTHORED` | the values, the attached manifests, or the params change | The configuration of the release changed. |
| `IMAGES` | the image list that the values declare, `pack.content.images`, changes | An image was added, removed, or moved to another tag. It comes with `AUTHORED`: it says which part of the values moved, and a gate that reads `AUTHORED` still sees it. |
| `CHARTS` | the chart list that the values declare, `pack.content.charts`, changes | A chart was added, removed, or moved to another version. It comes with `AUTHORED`, as `IMAGES` does. |

**The image list holds the text that the pack names, and strata reads no registry.**
Most entries name a tag, such as `registry.k8s.io/pause:3.10.2`. An image
rebuilt and pushed again under the same tag does not change the list. So
`IMAGES` does not show it, and no record shows it. See
[A vendor rebuilds an image under one tag](../use-cases/image-tag.md).

An entry can name a digest, as `<repository>:<tag>@sha256:<digest>`. Strata
keeps the whole entry. A new digest moves the root. `compare` prints the flag
`IMAGES` and the line
`~ image <repository>:<tag>: sha256:<old> -> sha256:<new>`. A `digest` field
beside `image` is not part of the list. A new value of that field shows as
`AUTHORED` and as a changed value, with no `IMAGES`.

The chart list holds a version, and no chart digest. A digest in the values of a
chart, such as `image.digest`, is a value, and `AUTHORED` covers it. The API
lists the images and the charts of a pack version, and no digest, signature, or
bill of materials for them. So the list is what a record can hold.

Other pack lines:

| Line | Meaning |
| --- | --- |
| `unchanged` | The node hash is the same on both sides. |
| `ADDED` | The pack is only on the second side. |
| `REMOVED` | The pack is only on the first side. |
| `tag <a> -> <b>` | Only the tag name changed, for the same name, layer, type, version, pack uid, and registry uid. It replaces `IDENTITY`. |

Under a pack that changed, a detail line names each image, chart, and param
that moved:

| Detail line | Meaning |
| --- | --- |
| `~ image <repository>: <old tag> -> <new tag>`, `+ image ...`, `- image ...` | An image of `pack.content.images` moved, was added, or was removed. |
| `~ chart <name>: <old> -> <new>`, `+ chart ...`, `- chart ...` | A chart of `pack.content.charts` moved, was added, or was removed. |
| `~ param <name>: <old> -> <new>`, `+ param ...`, `- param ...` | A param moved, was added, or was removed. It comes with `AUTHORED`. For an app tier, a param is a property of the tier. |

## A new machine image

A bring-your-own-image OS pack names its machine image in the annotation
`imageId`. The root does not cover the annotations, so a new `imageId` moves no
flag and gives no impact level. The pack line stays `unchanged`.

When both sides name an image, and the two differ, a `note` names the pack and
the two images. `compare` exits `1`, also when the roots are equal, `verify`
reports `DRIFT`, and `same-release` says `DIFFERENT`. No live run shows if
Palette replaces the nodes for a new `imageId`. See
[The OS pack names a new machine image](../use-cases/os-image.md).

## The impact levels

Strata calculates an impact for each changed pack, from its layer and flags. The
impact is a hint. It is not an answer from Palette.

**The table describes one upgrade engine.** It is the engine of the clusters that
the live runs built: a kubeadm cluster on a cloud that Palette builds with the
cluster API. The public pages give another rule for other clusters. On an Edge
cluster, a change to the Kubernetes layer repaves nothing, and on the OS layer
only the `uri` repaves. Palette keeps a list of the fields that repave for the
OS layer and the Kubernetes layer of each engine, and checks no other layer for
a repave. The lists belong to Palette and can change with each release, so
strata does not copy them.

The live runs also asked Palette about a managed engine, EKS. A new Kubernetes
version repaves, as the table says. A change of the values of the
`kubernetes-eks` pack or of the CSI pack repaves nothing: on EKS, `may-repave`
for the Kubernetes layer means no repave for these values. The EKS packs of the
OS and CNI layers have no values.

The live runs also asked Palette about GKE, a managed engine. A new Kubernetes
version repaves, as the table says. The four GKE packs have values of comments
alone, so the runs ask Palette about no change of values on GKE. A change of
the node pool is not a change of a layer, and the table gives it no level. See
[What no run covers](../internals/palette/limits.md#what-no-run-covers).

For a cluster, ask Palette: `compare --repave` prints its answer beside
the level, and says when the two disagree. See
[Ask Palette if the change repaves](../commands/compare/repave.md).

| Level | Condition | Meaning |
| --- | --- | --- |
| `repave` | OS, Kubernetes, or kernel layer with `IDENTITY` and a new version, or OS layer with `AUTHORED` | On a cluster of the engine above, Palette replaces the nodes. It checks every value of the OS pack. |
| `may-repave` | Kubernetes or kernel layer with `AUTHORED` | Node or control plane configuration changed. Palette can replace the nodes: it does for a key on its list, such as `kubeadmconfig.postKubeadmCommands`, and not for another. |
| `cluster-wide` | CNI or CSI layer with `IDENTITY` or `AUTHORED` | Each node gets the change. Palette does not replace the nodes for it: its repave check reads no CNI or CSI layer. |
| `in-place` | any other layer with `IDENTITY` or `AUTHORED` | Palette upgrades or configures the pack in place. |
| `sync` | only `UPSTREAM`, on any layer, or `UPSTREAM` with `IDENTITY` at the same version on the OS, Kubernetes, or kernel layer | Palette applies the change at the next profile sync. |
| `none` | no change, only a tag renamed for the same build, `IDENTITY` alone at the same version on the OS, Kubernetes, or kernel layer, or a renumber that keeps the numbers of the version | No node changes. |

**A new version repaves, and another pack reference does not.** On the OS,
Kubernetes, and kernel layers, `IDENTITY` gives `repave` only for a new
version. When the version stays and the type, the registry uid, or the pack uid
changes, the pack takes the level of its other flags. The impact text then says
`the same version from another pack reference`. A live run moved the OS pack of
a running cluster to another registry at one version. Palette said no repave,
and the cluster kept its nodes. See
[A pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).

A pack key is `<layer>/<name>`. So a release that exchanges one pack for a pack
of another name gives one `ADDED` line and one `REMOVED` line, and no `IDENTITY`
change. Such a pack takes the impact of its layer, as an identity change does. That is
`repave` on the OS, Kubernetes, or kernel layer. It is `cluster-wide` on the
CNI or CSI layer, and `in-place` on any other layer.

The list goes from the most severe level to the least severe. The JSON summary
gives the most severe level of a comparison as `maxImpact`. A renamed tag
together with a changed value on the OS layer is `repave`, as the changed value
alone is.

A change of the pack order alone moves no flag and no level. Every pack stays
`unchanged`, and `maxImpact` is `none`. The report adds the line `note: pack
order changed between the two profiles`.

## A renumber

A renumber moves a pack to another version and keeps its content. A pipeline
makes one when it gives a pre-release its final number. The two sides of the
pack have the same name, layer, type, and registry. They have the same values,
manifests, params, image list, and `imageId`. The tag, the pack uid, and the
digest of the archive follow the version.

A renumber whose version keeps its numbers gets the impact `none` on each
layer, also on the OS and the Kubernetes layers. Only the pre-release or the
build data of such a version changes, as from `1.65.0-rc.1` to `1.65.0`. A
number that a version omits is `0`, as Palette reads it, so `1.2-rc.1` to
`1.2.0` keeps its numbers too. The text is `none (renumber only)`, and `packs[].renumbered` in the JSON is `true`.
The flags still show `IDENTITY`, and `UPSTREAM` for the new digest:

```text
{{#include ../output/renumber-compare.txt}}
```

**A renumber to other numbers keeps the level of its layer.** `compare` and
`behind` read no archive of a pack. The values do not always name the content.
In the Public Repo, `ubuntu-aws` `20.04` and `24.04` have the same values and
two digests, and they are two releases of the OS. So such a move stays
`repave`. `same-release --allow renumber` reads the archives, and fails such a
move when another file differs: see
[The archives of the two builds](../commands/same-release.md#the-archives-of-the-two-builds).

**Run `same-release --allow renumber` to check the archive of each new build.**
The impact `none` covers what the profile holds. `compare` does not read the
archives, so it cannot show that only `pack.json` changed in the new build.
`compare` runs for each pull request, each night, and in the observer. A
download of a version makes Palette read each pack from its registry, and
Palette takes one download of a version in 10 seconds. So the one read of the
archives is the check of the publish.

## Where to see it

- The terminal report: `impact:` below each changed pack.
- The markdown comment: the `impact` column.
- The JSON document: `packs[].impact.level` and `summary.maxImpact`.
- [Pull request comments](../automation/pr-comments.md): the label threshold.
