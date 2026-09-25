# A pack has no digest

## The problem

Strata knows that the archive of a pack changed from the digest that Palette
gives for it. A registry that Palette does not sync gives no digest, such as a
Helm registry that needs credentials: see
[What no run covers](../internals/palette.md#what-no-run-covers). For such a
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

## What to do

- Take packs from a registry that Palette syncs, so each pack has a digest.
- Where a pack has none, decide it one time, and pass `--allow-no-digest` to
  `verify` and to `verify-report.sh`, and `--allow no-digest` to `same-release`.
- A manifest pack also has `upstreamSource: none`, and it never makes a record
  `UNVERIFIED`: its content is the manifests of the profile, which the root
  covers.

## Palette behavior

No live run produces a pack with no digest yet: see
[What no run covers](../internals/palette.md#what-no-run-covers). The rule for
each hash is at [The content digest](../internals/hashes.md#the-content-digest).
