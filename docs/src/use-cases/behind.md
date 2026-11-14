# The registry has newer versions

## The problem

A release pins each pack at one version. The registry keeps publishing newer
ones. Every record verifies, because Palette still holds what each one pins, and
a record does not say that its pins are behind, or when a newer version arrived.

In this example, `dev` has two release records: `aws-stack@1.0.0`, which pins
older packs, and `aws-stack@1.1.1`.

**Who runs this.** A job on a schedule, beside the drift report, and a person who
plans the next release.

<div class="playground-demo">

`just playground behind-1` serves this tenant as it was last
week, before the registry published `csi-aws-ebs` 1.65.0, and
`just playground behind-2` serves it today. `just playground behind-3` adds a
profile that Palette refuses to move, and `just playground behind-4` adds a
profile that pins Kubernetes two minor versions behind. `just playground behind-pre-release` serves a
registry with a pre-release of `csi-aws-ebs`. `just playground behind-profiles-1`
serves the cluster `payments` with its four profiles.

</div>

## 1. Last week

```text
{{#include ../output/behind-last-week.txt}}
```

The run keeps its JSON in `behind.json`.

## 2. Today

```text
{{#include ../output/behind.txt}}
```

The first line counts the 9 pins: 4 are behind and 5 are current. Four pins of
`aws-stack@1.0.0` are behind, and each line under one says what the bump does. The
Kubernetes bump and the OS bump are `repave`: plan them. The line
`palette accepts` says that Palette takes the packs of the move on the clusters
that run the profile. It covers the packs only: see
[A move that skips a minor version](../commands/behind.md#a-move-that-skips-a-minor-version).

The pins of `aws-stack@1.1.1` are current. The OS pins are floating tags. Each
pins the version that its tag names now, so the tag adds no version to move to.
`LTS__20.4.x` never reaches 22.04 or 24.04.

The newest Kubernetes version is of another minor version, and it drops `arm64`.
The line `next patch` names 1.35.6, the newest patch of the minor version of the
pin, and the command that compares the two:

```text
{{#include ../output/behind-next-patch.txt}}
```

The line `aws-stack@1.1.1 carries` under each pin of 1.0.0 says that a newer
version of the profile already pins the newer pack. So move the clusters to
`aws-stack@1.1.1`, and do not move each pin by hand. See
[The newer profile version that carries a newer pack](../commands/behind.md#the-newer-profile-version-that-carries-a-newer-pack).

The last lines are the profile versions of the records. `aws-stack@1.0.0` is
behind 1.1.1, the other version of `dev`. The move adds the required variable
`platform_owner`, which has no default value. Give it a value when you move a
cluster, or Palette refuses the update. See
[A newer version of the profile exists](./newer-profile.md).

With last week's run, the report says what arrived since:

```text
{{#include ../output/behind-since-lines.txt}}
```

## 3. A move that Palette refuses

A bump can be a move that Palette refuses: see
[The upgrade dependency](../internals/palette/validate-routes.md#the-upgrade-dependency).
`aws-stack@1.1.11` pins `csi-aws-ebs` 1.63.1, the registry has 1.65.0, and the
cluster `payments` runs the profile:

```text
{{#include ../output/behind-refused.txt}}
```

`palette refuses` gives the code of Palette and the cluster. The bump line says
`cluster-wide`, and it does not show the refusal. `--json` also gives the message
of Palette:

```text
{{#include ../output/behind-refused-json.txt}}
```

The record of the cluster `payments` pins the same pack. `behind` gives it the
same bump. It asks the route of the profile version that the cluster runs,
because the route of the cluster checks no sent tag of such a profile:

```text
{{#include ../output/behind-cluster.txt}}
```

`--no-impact` skips the question, as it skips the comparison:

```text
{{#include ../output/behind-refused-no-impact.txt}}
```

The playground answers each request of the route in the same way. A real Palette
answers for the packs of the body, so only the pack that declares the limit gets
the refusal.

## 4. A move of Kubernetes that skips a minor version

A field test ran `behind` on a pin of `kubernetes-eks` 1.31. It printed
`bump to 1.37` and `palette accepts`, and a reader took that to mean that the move
works. The route checks the packs. The update checks the minor version of
Kubernetes, and refuses a move of more than one. `aws-stack@1.1.20` pins
`kubernetes` 1.34.9, and the newest version is 1.36.2:

```text
{{#include ../output/behind-skip.txt}}
```

The line under `palette accepts` says that the move skips a minor version, and
names the next step, 1.35.6. Move the clusters to a version that pins 1.35.6
first. Then move them to 1.36.2. See
[A move that skips a minor version](../commands/behind.md#a-move-that-skips-a-minor-version).

## 5. A pre-release in the registry

A pipeline can publish a pre-release for each build, such as `1.66.0-rc.1` of
`csi-aws-ebs`. Strata orders versions as Palette does, by Semantic Versioning:
see
[The version that Palette calls the latest](../internals/palette/packs.md#the-version-that-palette-calls-the-latest).
A pre-release is below its version. So it is no newer version of a pin that is
not a pre-release. The pins of `aws-stack@1.1.1` are current:

```text
{{#include ../output/behind-pre-release.txt}}
```

`--pre-release` names the pre-release as a newer version:

```text
{{#include ../output/behind-pre-release-lines.txt}}
```

## 6. A cluster with many profiles

A field test ran `behind` on a cluster with 12 profiles: 29 pins, 10 of them
behind. The report gave each of the 19 current pins a row too. The line
`pinned by` gave the title of the record, with all the profiles and their versions.
So the line did not say which profile held the pack.

`--behind-only` omits the rows that are current, and the first line still counts
them. The line `pinned by` names the profile that holds the pack. Here the cluster
`payments` runs `aws-stack@1.0.0` and three add-on profiles:

```text
{{#include ../output/behind-only-cluster.txt}}
```

The pins that are behind are pins of `aws-stack@1.0.0`, so the move of that
profile is the step to plan. The add-on profiles are current, so they have no row.
See [A cluster with many profiles](../commands/behind.md#a-cluster-with-many-profiles).

## What to do

- Run `behind` on a schedule over the release records, and keep its report:
  [Upstream issues](../automation/upstream-issues.md) does both, and keeps one
  issue open for each pack that is behind.
- Read the bump before you move a pin: `repave` needs a window, and `in-place`
  does not.
- When the newest version is too far, run the command of the line `next patch`.
  It says what the move to the newest patch of the minor version of the pin does.
- On many records, or on a cluster with many profiles, add `--behind-only`. Each
  line `pinned by` names the profile to move.
- To report one pack, give its key to `--pin`, such as `--pin k8s/kubernetes`.
  [One pack](../commands/behind.md#one-pack) shows the report.
- When Palette refuses a move, pick a closer version in `newer`. `behind` asks
  about the newest version only. A candidate gets no answer about the move.
  Palette checks the limit of a move against the clusters that run a version.
  No cluster runs a candidate.
- When a move of Kubernetes skips a minor version, move to the next step that
  the line names first. A move of a profile version can also add a required
  variable with no default value: give it a value in the update.
- Pin a final version. A pin of a pre-release is behind its final version and each
  later version. Use `--pre-release` only to see what the pipeline builds.
- Move the pin in a new release version, and not in place. Then `verify` of the
  old records still holds, and `compare` of the two versions says what changed:
  see [What changed between two pack versions](./pack-versions.md).

## Palette behavior

The live run of `just live-upstream` pushes a new version into the example's own
registry between two runs. The second run names the version as arrived. Palette
lists it with the time it first listed it, and the issue of the pack follows. See
[What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack).

The refusal of a move that skips a minor version comes from the code of Palette,
and no live run shows it yet. Stage 7 of `validate-packs.sh` is the live check:
see [validate-packs.sh](../development/live-runs.md#validate-packssh) and
[What the update checks](../internals/palette/validate-routes.md#what-the-update-checks).
