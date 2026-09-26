# The registry has newer versions

## The problem

A release pins each pack at one version. The registry keeps publishing newer
ones. Every record verifies, because Palette still holds what each one pins, and
nothing says that the pins are behind, or when a newer version arrived.

In this example, `dev` has two release records: `aws-stack@1.0.0`, which pins
older packs, and `aws-stack@1.1.1`.

**Who runs this.** A job on a schedule, beside the drift report, and a person who
plans the next release.

**Follow along.** `just playground behind-1` serves this tenant as it was last
week, before the registry published `csi-aws-ebs` 1.65.0, and
`just playground behind-2` serves it today.

## 1. Last week

```text
{{#include ../output/behind-last-week.txt}}
```

The run keeps its JSON in `behind.json`.

## 2. Today

```text
{{#include ../output/behind.txt}}
```

Four pins of `aws-stack@1.0.0` are behind, and each line under one says what the
bump does. The Kubernetes bump and the OS bump are `repave`: plan them. The pins
of `aws-stack@1.1.1` are current. The OS pins are floating tags: `LTS__24.x`
reaches each 24 release on its own, and `LTS__20.4.x` never reaches 22.04 or
24.04.

With last week's run, the report says what arrived since:

```text
{{#include ../output/behind-since-lines.txt}}
```

## What to do

- Run `behind` on a schedule over the release records, and keep its report:
  [Upstream issues](../automation/upstream-issues.md) does both, and keeps one
  issue open for each pack that is behind.
- Read the bump before you move a pin: `repave` needs a window, and `in-place`
  does not.
- Move the pin in a new release version, and not in place. Then `verify` of the
  old records still holds, and `compare` of the two versions says what changed:
  see [What changed between two pack versions](./pack-versions.md).

## Palette behavior

The live run of `just live-upstream` pushes a new version into the example's own
registry between two runs. The second run names the version as arrived. Palette
lists it with the time it first listed it, and the issue of the pack follows. See
[What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack).
