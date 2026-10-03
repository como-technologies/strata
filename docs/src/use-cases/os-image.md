# The OS pack names a new machine image

## The problem

A bring-your-own-image OS pack names its machine image, such as an AMI, in the
annotation `imageId` of its pack reference. A person can give the pack another
`imageId` and keep its version, its build, and its values. The version then
names another image, and the root does not move: the root does not cover the
annotations. See
[What the root does not cover](../internals/hashes.md#what-the-root-does-not-cover).

A record keeps the annotations of each pack. `compare` and `verify` name a pack
whose `imageId` differs.

In this example, `dev` has `aws-stack@1.1.8`, the content of 1.1.1 with an OS
pack that names its image.

**Who runs this.** The scheduled verify job finds this. The job is
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground os-image-1` serves this tenant on this
machine, with no key and no cost. `just playground os-image-2` serves it after
the edit of the image.

## 1. Record the version

```text
{{#include ../output/os-image-record.txt}}
{{#include ../output/os-image-annotations.txt}}
```

The record keeps the annotations of the OS pack, with `imageId`.

## 2. A person gives the OS pack another image

**Follow along.** `just playground os-image-2` is the same tenant after a
person gave the OS pack another `imageId`. Stop the first seed, and export what
this one prints. Keep the record from step 1.

```text
{{#include ../output/os-image-verify.txt}}
```

`verify` exits `1`. The roots are equal, and the note names the two images.
`compare` names the change in `imageIdsChanged`, and exits `1` too:

```text
{{#include ../output/os-image-compare-json.txt}}
```

## What to do

- Keep a record of each version with a bring-your-own-image OS pack under the
  scheduled verify job.
- When the new image is correct, record the version again. When it is not, give
  the pack the recorded `imageId` again.
- Publish a new image in a new profile version, so that a record of each
  version names one image.

## Limits

- A side with no `imageId` is not compared. A record written before strata kept
  the annotations has none. The OS pack `ubuntu-aws` of the Public Repo has
  none either: see
  [The annotations of a pack reference](../internals/palette/packs.md#the-annotations-of-a-pack-reference).
- The impact level comes from the hashes, so a new `imageId` gives no level.
  See [A new machine image](../reference/impact.md#a-new-machine-image).
- No live run gives an OS pack another image. See
  [What no run covers](../internals/palette/limits.md#what-no-run-covers).
