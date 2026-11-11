# A newer version of the profile exists

## The problem

A pipeline has three levels of versions. A pack goes to a registry, a profile
version pins the packs, and the definition of a cluster pins the profile
versions. The pipeline of the team publishes a new version of a profile.
`verify` still passes on each record of the old version, because Palette still
holds what the record holds. A pin that is behind names a pack, and not the
profile. So no check says that the new version of the profile exists.

In this example, the record of `dev` holds `aws-stack@1.0.0`. The pipeline
published 1.1.0 and the pre-release 1.2.0-rc.1 in `dev`. The project `prod` holds
1.3.0, and the tenant holds 2.0.0. The cluster `payments` runs 1.0.0.

**Who runs this.** A job on a schedule, beside the drift report, and a person who
plans the move of a cluster.

<div class="playground-demo">

`just playground behind-profiles` serves this tenant on this machine, with no key
and no cost.

</div>

## 1. Ask for the newer versions

```text
{{#include ../output/behind-profile-lines.txt}}
```

Exit `1`. The line `[profile]` says that `aws-stack@1.0.0` is behind 1.1.0. The
pre-release is no version to move to. The versions of `prod` and of the tenant
are in another scope, so Palette does not list them for this version. See
[Newer versions of a profile](../commands/behind.md#newer-versions-of-a-profile).
The check `behind-profile-names` of [loop.sh](../development/live-runs.md#loopsh)
tests the list of Palette on the live tenant.

## 2. The record of the cluster

```text
{{#include ../output/behind-profile-cluster-lines.txt}}
```

The record of `payments` holds each profile that the cluster runs. So the same
line names the version that the cluster can move to.

## 3. In a script

```text
{{#include ../output/behind-profile-json.txt}}
```

## What to do

- Compare the two versions before a move:
  `strata compare aws-stack@1.0.0 aws-stack@1.1.0`. See
  [strata compare](../commands/compare.md).
- Ask Palette about the move of the cluster:
  `strata compare cluster:payments aws-stack@1.1.0 --repave`. See
  [Ask Palette if the change repaves](../commands/compare/repave.md).
- After the move, record the cluster again: see
  [The everyday loop](../loop.md#the-pipeline-publishes).
- Run `behind` on a schedule. The exit is `1` while a record holds a version that
  is behind.
