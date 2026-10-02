# A pack has no digest

## The problem

Strata knows that the archive of a pack changed from the digest that Palette
gives for it. Some registries give no digest: a Helm registry with
synchronization off, and an OCI registry with the provider type `zarf`. See
[The digest of each registry kind](../internals/palette/packs.md#the-digest-of-each-registry-kind). For such a
pack, the upstream hash is the hash of nothing on each side. A change of its
archive cannot move the root, and an equal root does not say that the archive
is the same.

In this example, `dev` has `aws-stack@1.1.5`, whose CSI pack has no digest, and
`aws-stack@1.1.1`, which has the same packs with each digest.

**Who runs this.** A person who records a release, and the scheduled verify job.
The job is [Drift issues](../automation/drift-issues.md).

**Follow along.** `just playground no-digest` serves this tenant on this
machine, with no key and no cost.

## 1. The record says which pack has no digest

```text
{{#include ../output/nodigest-record.txt}}
{{#include ../output/nodigest-source.txt}}
```

`upstreamSource` is `none` for the CSI pack. The other packs have the digest of
their pack reference.

## 2. verify cannot say that the archive is unchanged

```text
{{#include ../output/nodigest-verify.txt}}
```

The content is the same, and the verdict is `UNVERIFIED`, not `OK`: a check that
cannot see a change must not report no change. `verify` exits `1`, and the
report names the pack beside its line.

A team that accepts the pack says so with `--allow-no-digest`. The verdict is
`OK`, the exit is `0`, and the report still names the pack:

```text
{{#include ../output/nodigest-allow.txt}}
```

## 3. A move to a pack with no digest is not the same release

```text
{{#include ../output/nodigest-same-release.txt}}
```

`1.1.5` has the content of `1.1.1`, and its CSI pack has no digest. The pair is
`DIFFERENT`, and the reasons name the pack: a move from a registry that gives a
digest to one that gives none loses what the root can show.
`--allow no-digest` accepts it.

## 4. Review a chart from a private registry before the create

A team adds a profile with a chart from `charts-auth`, a private Helm registry.
Palette does not sync it, so its pack listing has no chart of it. The team
reviews the profile in a candidate file before it creates the profile:

```text
{{#include ../output/nodigest-chart-file.txt}}
```

Make the record of the file:

```text
{{#include ../output/nodigest-chart-record.txt}}
```

Palette makes the pack of the chart when it creates the profile. So the record
has the pack that Palette makes: no pack uid, no digest, the layer `addon`, and
the tag as the version. The team creates the profile in Palette, and records
the published version. The two packs differ by the pack uid alone:

```text
{{#include ../output/nodigest-chart-published.txt}}
{{#include ../output/nodigest-chart-pack.txt}}
```

The chart has no digest, so `same-release` does not pass the pair:

```text
{{#include ../output/nodigest-chart-same.txt}}
```

With `--allow no-digest`, it passes the pair, and `mintedUidOnly` names the
chart:

```text
{{#include ../output/nodigest-chart-allow.txt}}
```

After the create, the listing has the chart with its pack uid. A record of the
same file then gets that uid, and the root of the published version. The
playground serves the tenant after the create.

## What to do

- Take packs from a registry that Palette syncs, so each pack has a digest.
- Where a pack has none, decide it one time, and pass `--allow-no-digest` to
  `verify` and to `verify-report.sh`, and `--allow no-digest` to `same-release`.
- Give a chart from a registry that Palette does not sync in a candidate file,
  as another pack. Do not give it a `layer` other than `addon`, or a `tag`
  other than its version: Palette gives neither to the pack that it makes. See
  [A layer from a registry that Palette does not sync](../reference/candidate-file.md#a-layer-from-a-registry-that-palette-does-not-sync).
- A manifest pack also has `upstreamSource: none`, and it never makes a record
  `UNVERIFIED`: its content is the manifests of the profile, which the root
  covers.

## Palette behavior

Two registry kinds give a pack with no digest, live: a Helm registry with
synchronization off, and an OCI registry with the provider type `zarf`. On a record of each, `verify` reports `UNVERIFIED`, and on
the Helm one `same-release` refuses it: see
[The digest of each registry kind](../internals/palette/packs.md#the-digest-of-each-registry-kind). The rule for
each hash is at [The content digest](../internals/hashes.md#the-content-digest).
What Palette makes for a chart of a registry that it does not sync is at
[A pack that Palette makes at the create](../internals/palette/packs.md#a-pack-that-palette-makes-at-the-create).
