# A version bump overwrites a version

## The problem

Without the immutable-clusterprofiles flag of the Spectro Cloud provider, a
version bump with the provider updates the profile in place. The uid stays the
same, and takes the new version number and content. The old version disappears from
the listing. The clusters that ran it keep copies that name the old version.

In this example, `dev` has `aws-stack@1.1.1` with the uid `...c3`, and the
cluster `payments` runs it. The team then bumps the version to 1.2.0 with one
changed value, and the flag is off.

## 1. The old version is gone

```text
{{#include ../output/overwritten-list.txt}}
```

The uid `...c3` names `1.2.0` now.

## 2. The record of the version

The steps below check two records that the team wrote before the bump: the
version, and the cluster that runs it.

```text
{{#include ../output/overwritten-records.txt}}
```

```text
{{#include ../output/overwritten-verify.txt}}
```

The verdict names the overwrite. The report shows what changed under the uid.

## 3. The record of the cluster

```text
{{#include ../output/overwritten-cluster.txt}}
```

The cluster runs exactly what it ran: `IDENTICAL`. But the version that it runs
is gone from Palette, so the record reports `DRIFT`.

## 4. The consumers of the new version

```text
{{#include ../output/overwritten-consumers.txt}}
```

Palette lists `payments` as a consumer of the uid. The line says that the
cluster runs the old version.

## 5. The cluster against the new version

```text
{{#include ../output/overwritten-compare.txt}}
```

The note names the uid, the version that the cluster runs, and the version that
the uid holds now.

## Repair the cluster

The cluster runs a version that Palette no longer has. Its record holds the
content. Write the record as a candidate file, publish it again as a new
version, and move the cluster to that version:

```text
{{#include ../output/overwritten-candidate.txt}}
```

Strata writes the file, and stops there. You publish the version with the
provider or the Palette API, and you move the cluster to it. See
[The candidate file](../reference/candidate-file.md).

## What to do

Set the flag in the provider block:

```hcl
provider "spectrocloud" {
  feature_preview = {
    "immutable-clusterprofiles" = true
  }
}
```

With the flag, a bump makes a new uid, and the old version stays. The same bump
then gives:

```text
{{#include ../output/overwritten-flag-list.txt}}
{{#include ../output/overwritten-flag-verify.txt}}
```

Set `skip_destroy` on each profile before the first bump with the flag. It is an
argument of the profile resource, not of the provider block above:

```hcl
resource "spectrocloud_cluster_profile" "aws_stack" {
  skip_destroy = true
}
```

Without it, the provider deletes the replaced version, and Palette refuses the
delete while a cluster runs it. See
[What Palette does: version bumps](../internals/palette.md#version-bumps).
