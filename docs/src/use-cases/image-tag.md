# A vendor rebuilds an image under one tag

## The problem

A pack names an image by a short tag, such as `vendor.example/product:2026.2`.
A vendor can rebuild the image and push it under the same tag. The tag then
names a new image with a new digest. The vendor can delete the old digest too,
so no tag names the old image.

Palette gets no notice. The pack text is the same, so the record, the root, and
each command of strata stay the same. A team met this case. Its pipeline copied
each image, by digest, to a private registry with immutable tags. The copy step
found that the source no longer matched, and no new version of the pack could
publish. The clusters kept the tested image from the private registry, so
nothing that ran changed.

In this example, `dev` has `aws-stack@1.1.13`. Its Kubernetes pack names the
image `vendor.example/product:2026.2` in `pack.content.images`. Four later
versions show the fixes.

**Who runs this.** A person who records a release, the pipeline that publishes
a pack, and the scheduled verify job. The job is
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground image-tag` serves this tenant on this
machine, with no key and no cost. The tenant holds the Palette side alone. It
has no registry for the image, so the text describes the rebuild.

## 1. Record the version

```text
{{#include ../output/image-tag-record.txt}}
{{#include ../output/image-tag-images.txt}}
```

The record keeps the image as the pack names it: a tag and no digest.

## 2. The vendor rebuilds the tag

The vendor pushes a new image under `2026.2`. The pack text, the record, and
Palette do not change. Strata reads no registry, so each answer is the same:

```text
{{#include ../output/image-tag-verify.txt}}
```

`verify` exits `0`. `compare` of the record with the same version exits `0`
too:

```text
{{#include ../output/image-tag-compare-same.txt}}
```

`OK` is true of the content that Palette holds. It says nothing of the image
behind the tag. Palette pulls no image, and it holds no digest of one, so it has
nothing to compare. A node pulls what the chart or the manifest names. Two
clusters on this version can run two images. See
[What no run covers](../internals/palette/limits.md).

## 3. Name a full build tag in a new version

The team publishes `aws-stack@1.1.14`. Its pack names the build tag
`2026.2.0-1234.1` in place of `2026.2`.

```text
{{#include ../output/image-tag-compare-build.txt}}
```

Exit `1`. `IMAGES` says that the image list changed, and `AUTHORED` says that
the values changed. The line `~ image` names the repository and the two tags.

The `impact` line comes from the layer of the pack. This pack is the Kubernetes
layer, so a change of its values is `may-repave`. Another layer gives another
level: see [Change flags and impact](../reference/impact.md#the-impact-levels).
The level is a hint. To ask Palette, see
[Ask Palette if the change repaves](../commands/compare/repave.md).

A new tag from another registry reads as a new image. One pair becomes two
lines:

```text
{{#include ../output/image-tag-compare-mirror.txt}}
```

Exit `1`. A repository with another host is another repository, so the report
shows a `+` line and a `-` line, and no `~` line.

## 4. Name a digest

A build tag is a name, and a vendor can still move it. A digest is not a name:
it names one image. `aws-stack@1.1.16` names the image as
`vendor.example/product:2026.2@sha256:<digest>`. A later version names another
digest:

```text
{{#include ../output/image-tag-compare-digest.txt}}
```

Exit `1`. Strata keeps the whole entry, so a new digest moves the root and shows
as a changed image. A vendor that moves the tag cannot change what a record of
`aws-stack@1.1.16` says.

A pull by digest gets that digest, or it fails where a person can see it. A
digest in the values of a chart, such as `image.digest`, works the same way:
`AUTHORED` covers the value.

## What to do

- Name the digest where the chart or the manifest reads it. Palette pulls no
  image, so a digest in `pack.content.images` alone can pin nothing.
- Or copy each image, by digest, to a registry with immutable tags. Name the
  copy in the pack.
- Keep a record of each version under the scheduled verify job. A new tag or
  digest in the pack text shows as a change.
- Read the note `image-tag-not-digest` in `strata preflight`. It names the owner
  `operator`, and it lists these two fixes. It is a note, because a tag names
  nearly each image of nearly each pack.

## Limits

- Strata reads no registry. An image that a vendor rebuilds under a tag that the
  pack text names stays invisible until the text changes. See
  [Where each part belongs](../boundary.md).
- `images` keeps the `image` of each entry in the list. A `digest` field beside
  it is not part of the list, so `IMAGES` does not show a new value of it.
  `AUTHORED` and the changed value show it.
- No run shows which image a node pulls when only `pack.content.images` names a
  digest. The check is in
  [image-digest.sh](../development/live-runs.md#image-digestsh).
