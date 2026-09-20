# A version bump overwrites a version

## The problem

Without the `immutable-clusterprofiles` flag of the Spectro Cloud provider, a
version bump with the provider updates the profile in place. The uid stays the
same, and takes the new version number and content. The old version disappears from
the listing. The clusters that ran it keep copies that name the old version.

In this example, `dev` has `aws-stack@1.1.1` under the profile uid
`64e0000000000000000000c3`, and the cluster `payments` runs it. The team then
bumps the version to 1.2.0 with one changed value, and the flag is off. The page
shortens that profile uid to `...c3`. The other uid in each listing,
`64f20000000000000000a0d1`, is the project `dev`.

**Who runs this.** A person at a terminal, after a bump surprised them. The
scheduled verify job is what reports the overwrite first: see
[Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground overwritten-1` serves this tenant on this
machine, with no key and no cost. A bump changes the tenant, so this page has
three seeds. Each section names the one it needs, and each seed serves on its
own port.

## 1. Record the version and the cluster

Before the bump, the team records both the version and the cluster that runs it:

```text
{{#include ../output/overwritten-records.txt}}
```

Both records have the root `088136c929bd`, because the cluster runs exactly that
version. Keep the two files: every later step reads them.

## 2. The bump: the old version is gone

**Follow along.** `just playground overwritten-2` is the same tenant after the
bump. Stop the first seed, and export what this one prints. Keep the two records
from step 1.

```text
{{#include ../output/overwritten-list.txt}}
```

The uid `...c3` names `1.2.0` now, and `1.1.1` is in no listing.

## 3. The record of the version

```text
{{#include ../output/overwritten-verify.txt}}
```

Exit `1`. The verdict names the overwrite, and the pack table shows what changed
under the uid.

## 4. The record of the cluster

```text
{{#include ../output/overwritten-cluster.txt}}
```

The cluster runs exactly what it ran: `IDENTICAL`. But the version that it runs
is gone from Palette, so the record reports `DRIFT`, and `verify` exits `1`.

## 5. The consumers of the new version

```text
{{#include ../output/overwritten-consumers.txt}}
```

Palette lists `payments` as a consumer of the uid. The line says that the
cluster runs the old version. Exit `1`.

## 6. The cluster against the new version

```text
{{#include ../output/overwritten-compare.txt}}
```

The note names the uid, the version that the cluster runs, and the version that
the uid holds now. Exit `1`.

## Repair the cluster

The cluster runs a version that Palette no longer has. Its record holds the
content. Write the record as a candidate file:

```text
{{#include ../output/overwritten-candidate.txt}}
```

Strata writes `aws-stack-1.1.1.yaml`, and a directory `aws-stack-1.1.1/` with
the values and the manifests that the file points at. The file carries
`version: 1.1.1`, the number in the record.

Strata does not publish. Publish that file again under `1.1.1` with the provider
or the Palette API: Palette gives it a new uid, and the content is the content
that `payments` runs. Then move `payments` to the new version, and record it
again. See [The candidate file](../reference/candidate-file.md).

## What to do

Set the flag in the provider block:

```hcl
provider "spectrocloud" {
  feature_preview = {
    "immutable-clusterprofiles" = true
  }
}
```

With the flag, a bump makes a new uid, and the old version stays.

**Follow along.** `just playground overwritten-3` is the same tenant after the
same bump, with the flag on. Stop the second seed, and export what this one
prints. Keep the two records from step 1.

```text
{{#include ../output/overwritten-flag-list.txt}}
{{#include ../output/overwritten-flag-verify.txt}}
```

Both records verify, and exit is `0`. The old uid `...c3` still holds `1.1.1`,
and `1.2.0` has a uid of its own.

The flag needs two more settings on each profile resource, and it is an error to
set one without the others:

```hcl
resource "spectrocloud_cluster_profile" "aws_stack" {
  skip_destroy = true

  lifecycle {
    create_before_destroy = true
  }
}
```

`skip_destroy` stops the provider deleting the replaced version. Without it the
provider deletes it, and Palette refuses the delete while a cluster runs it.
`create_before_destroy` makes the provider publish the new version before it
drops the old one from the state. The provider checks `skip_destroy` at plan
time and fails the plan; it cannot see the `lifecycle` block, so that one is
yours to remember.

See [What Palette does: version bumps](../internals/palette.md#version-bumps).
