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

## 2. What the move changes

`behind` also compares 1.0.0 with the newest version, as `compare` of the two
versions does. The line `bump to` names the impact level and the change flags of
the move:

```text
{{#include ../output/behind-profile-move-lines.txt}}
```

The level is `repave`, a hint of the table in
[The impact levels](../reference/impact.md#the-impact-levels). `compare` of the two
versions gives the same level:

```text
{{#include ../output/behind-profile-move-compare.txt}}
```

The JSON names each pack that the move changes, with its two versions, and
`valuesChanged` when its values change:

```text
{{#include ../output/behind-profile-move-json.txt}}
```

See [The move to the newest profile version](../commands/behind.md#the-move-to-the-newest-profile-version).

## 3. The record of the cluster

```text
{{#include ../output/behind-profile-cluster-lines.txt}}
```

The record of `payments` holds each profile that the cluster runs. So the same
line names the version that the cluster can move to.

## 4. How far the change of a pack went

When `behind` says that a pin is behind, the next question is: did the change reach
a profile version yet? Here the registry published `csi-aws-ebs` 1.66.0 and
`cert-manager` 1.22.0 after the publish of 1.1.0.

<div class="playground-demo">

`just playground behind-profiles-2` serves this tenant on this machine, with no key
and no cost.

</div>

```text
{{#include ../output/behind-carrier-lines.txt}}
```

The line under each pin names the newest profile version that carries a newer
version of the pack:

- `aws-stack@1.1.0 carries csi-aws-ebs@1.65.0`: the change of 1.65.0 reached the
  profile, and 1.66.0 did not yet. The definition of the cluster still pins 1.0.0,
  so the move of the cluster is the step that waits.
- `no newer version of aws-stack@1.0.0 carries a newer cert-manager`: the change
  stopped at the registry. The pipeline of the profile is the step that waits.

See
[The newer profile version that carries a newer pack](../commands/behind.md#the-newer-profile-version-that-carries-a-newer-pack).

## 5. In a script

```text
{{#include ../output/behind-profile-json.txt}}
```

## What to do

- Read the full diff of the move before you move a cluster:
  `strata compare aws-stack@1.0.0 aws-stack@1.1.0 --full`. See
  [strata compare](../commands/compare.md). The
  [observer](../automation/observer.md#run-it-once) shows the same move on the page
  of the profile.
- Ask Palette about the move of the cluster:
  `strata compare cluster:payments aws-stack@1.1.0 --repave`. See
  [Ask Palette if the change repaves](../commands/compare/repave.md).
- After the move, record the cluster again: see
  [The everyday loop](../loop.md#the-pipeline-publishes).
- When no newer profile version carries a newer pack, publish a profile version
  that pins it. Then move the definition of the cluster to that version.
- Run `behind` on a schedule. The exit is `1` while a record holds a version that
  is behind.
