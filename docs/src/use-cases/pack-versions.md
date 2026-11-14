# What changed between two pack versions

## The problem

A registry offers a new version of a pack. A pull request then changes the pack
version that one layer of a profile pins. The merge publishes a new profile
version, and the version before it still pins the old pack.

Before the review, you want to know what the pack itself changed: the images it
runs, the values it ships, and whether the change replaces nodes. The Palette
console shows the values of two versions next to each other. A record keeps
that comparison after you close the page.

A pack version is a ref: `pack:<name>@<version>`. Strata reads it from the
registry and hashes it as it hashes that pack inside a profile.

**Who runs this.** A person reviewing a bump, at a terminal. The comment job
posts the same comparison on a pull request: see
[Pull request comments](../automation/pr-comments.md).

<div class="playground-demo">

`just playground packs` serves this tenant on this machine,
with no key and no cost. Nothing on this page changes the tenant, so one seed
serves every step.

</div>

## 1. Compare two versions

```text
{{#include ../output/pack-compare.txt}}
```

`IDENTITY` says the layer points at another pack build, `UPSTREAM` that the
content digest moved, and `AUTHORED` that the values the registry ships
changed. `IMAGES` says that the image list of the values changed: the `image`
lines name each image that was added, removed, or moved to another tag, by its
repository, and the `images` line under `root` counts them. The lines under the
values name each changed key. Exit `1`: the versions differ.

```text
{{#include ../output/pack-compare-images.txt}}
```

The image list holds the text that the pack names. An image pushed again under
the same tag does not show here: see
[A vendor rebuilds an image under one tag](./image-tag.md).

The last line of the block is the one that decides what you do:
`impact: node repave (Kubernetes version change)`. A change of the Kubernetes
version replaces each node of every cluster that takes this bump, so the bump
needs a maintenance window. [Impact](../reference/impact.md) gives each level.

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

To compare the pack of a profile, name the registry that the profile takes it
from. The error names the command that shows it:

```text
{{#include ../output/pack-which-registry.txt}}
```

Each pack row ends with its registry. `aws-stack@1.1.18` takes `csi-aws-ebs`
from `Palette Registry`, and `aws-stack@1.1.1` takes it from `Public Repo`. A
record of the version names the registry too, beside its uid:

```text
{{#include ../output/pack-record-registry.txt}}
```

So the ref of the pack of `aws-stack@1.1.18` is
`pack:Palette Registry/csi-aws-ebs@1.65.0`. See
[The registry of each pack](../commands/list.md#the-registry-of-each-pack).

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

`strata packs <name> --latest` gives the version that `@latest` names in each
registry. This is the question a bump asks: is the pin the current version?
Without `--latest` the command lists every version. See
[packs](../commands/packs.md).

Each row is one build, and its four columns are words the later pages use. The
**pack uid** is what the registry calls that build, and it is the identity a
comparison reads: two rows with one uid are one build. The **pack type** says
how the registry serves it, `spectro` for the Public Repo and `oci` for an OCI
registry. Then the clouds it builds for. Last come the **tags** that point at
the build: one build can carry several, so `1.65.0`, `1.65.x` and `1.x` are
three names for one thing, and a profile stores the one its author picked.

That answers whether you are behind. To see what you get if you catch up, put
`@latest` on one side of a comparison:

```text
{{#include ../output/pack-behind-head.txt}}
```

`@latest` is in the command, and the report names the version it resolved to,
`1.36.2`. So the output stays a record of one comparison, and you can keep it.

Ask this at a terminal, and not in a job. `@latest` answers with whatever the
registry holds that day, so the same command gives another answer after the
registry takes a new version. A job pins both sides.

## 4. Record a version, and check it later

```text
{{#include ../output/pack-record.txt}}
```

`pack:<name>@latest` takes the highest version that the registry serves and that is
not a pre-release. The record is a snapshot like any other: keep it, and `verify` reads
the registry again.

```text
{{#include ../output/pack-verify.txt}}
```

Exit `0`: the registry still serves that content under that version. `DRIFT`
and exit `1` say the pack version changed after you recorded it, which a
published build must never do.

The verdict ends with the time of the last sync of the registry. Palette keeps
a copy of the registry, and `verify` reads that copy. When the verdict ends
with `the last sync of the registry failed: <message>`, the copy can be old:
fix the registry or its credentials in Palette, and verify again after the next
sync. See
[The sync of the registry of a pack record](../commands/verify.md#the-sync-of-the-registry-of-a-pack-record).

## What a pack record does not carry

A pack has no profile variables, so the reports say nothing about variables when
both sides are packs. A pack record holds the values the registry ships, and
not the values a profile sets on top of them. It holds no manifests and no
params, because those belong to a profile. To compare what a profile does with a
pack, compare the profile versions: [Review a change before you publish it](./review.md).

## What to do

- Pin the version, not a floating tag, when the build must not move. A tag can
  point at another build later.
- Record the pack, and verify the record on a schedule. `verify` reads the
  registry again, and answers `DRIFT` with exit `1`.
- Name the registry with `--registry` when two registries hold the pack name.
  `strata list registries` names each registry that the key can see.

## Palette behavior

For `@latest`, strata takes the highest version that the registry serves and that is not a
pre-release, in the order of Palette. `strata packs` lists the versions newest
first, with the tags that point at each. Two registries can hold one pack name.
See
[What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack).
