# A publish fans out to what pins it

Some teams keep one repository per pack, one per profile, and one per cluster.
A merge in a pack repository publishes a pack version. A job then opens a bump
merge request on each profile that pins that pack. Merging a bump publishes a
profile version, and that publish opens a bump on each cluster that pins the
profile. Nobody types a version number: the chain does.

This is the same problem as [A team pins each environment](./fan-out.md), one
level up. Many merge requests are open at once, most of them opened by a job,
and each one was written against a level below that keeps moving.

## What each level holds, and what it records

| Level | The repository holds | A merge publishes | The record |
| --- | --- | --- | --- |
| pack | the pack content and its version | a pack version in a registry | the registry digest, which `pack:` reads |
| profile | the pack versions that the profile pins | a profile version in Palette | `strata snapshot <name>@<version>` |
| cluster | the profile versions that the cluster pins | the cluster | `strata snapshot cluster:<name>` |

The lifecycle step is build, test, sign, publish, **record**. The record is the
artifact of the publish, and the publish job writes it:

```bash
# in the publish job of a profile repository, after Palette holds the version
strata snapshot identity@1.0.1 -o releases/identity/1.0.1.json
```

Without that file, a later question has nothing to ask. With it, every check
below is one command.

## Why a record, when the registry has a digest

A published version is immutable by rule, and Palette does not enforce the
rule: the API accepts an edit to a published profile version, under the same
uid and the same version. See
[A published version is edited in place](./edited-in-place.md) and the live run
in [What Palette does](../internals/palette.md).

The record is what turns the rule into a check:

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. A scheduled job over every record keeps one issue open per record that
drifted, on either host. See [Drift issues](../automation/drift-issues.md).

## Review a bump that a job opened

A bump merge request says "1.0.0 becomes 1.0.1". That is not a review. The
comparison is:

```text
{{#include ../output/pack-compare.txt:1:11}}
```

Put it in the merge request with
[`pr-comment.sh`](../automation/pr-comments.md), which posts one comment on
GitHub or one note on GitLab, and labels the merge request when the impact
reaches `repave`. The reviewer reads what the bump does to a node, not a
version string.

## Two bumps race, and the second one wins

The fan-out opens a bump against the profile as it was at the publish. While it
waits for review, another bump merges. The second merge request is now behind,
and merging it puts the earlier pack version back.

[The merge gate](../automation/merge-gate.md) is the check for this. It merges
the target branch in first, and reads the result:

```text
{{#include ../output/merge-gate-behind.txt}}
```

A job that lowers a pin on purpose — a pack version is withdrawn, so the bump
pins the one before it — writes the declaration in the commit it makes:

```text
Rollback: releases/identity/1.0.1.json to identity@1.0.0

spire 1.0.1 was withdrawn: the CRD pack and the server disagree on the namespace
```

The gate then reports the rollback and passes. A human doing the same thing
writes the same line, or a reviewer puts the `rollback` label on the merge
request. Nothing else turns a backwards pin green, so no bump slips through
because a job was running from an old base.

## Before you push

Every check above also runs in a terminal, which is where a developer wants
them: before the push, not after the pipeline.

```bash
# what do the environments run right now, and does any record no longer match?
strata verify "releases/**/*.json" --summary

# what would my branch land, against the default branch as it is now?
scripts/ci/merge-gate.sh "releases/**/*.json" --base main
```

The gate writes to no host and needs no token: it fetches the target branch,
merges it into the checkout, and prints the same report the pipeline prints.
A developer sees the stomp before anyone reviews it, and the pipeline is the
backstop rather than the messenger.

## Before you retire a pack version

The chain also answers the question that comes at the end of a version's life:

```text
{{#include ../output/list-pack-consumers.txt:1:6}}
```

`strata packs <name> --latest` says which version each registry offers now,
which is what a bump compares its pin against.
`strata list --pack <name>` finds the profiles that carry a pack, and
`strata consumers <name>@<version>` finds the clusters that run a profile
version. See [Find the profiles that carry a pack](./find-a-pack.md) and
[Retire a version](./retire.md).

## Does the cluster run what the repository says

The apply is not the proof. A cluster keeps its own copy of what a profile held
when it was attached, and a change on the cluster does not reach the profile:

```text
{{#include ../output/cluster-run-record.txt}}
```

Record each cluster after its apply, and the scheduled verify covers the
clusters as well as the profiles. See
[A cluster does not run its release](./cluster-release.md).

## The checks at each level

| Level | On a merge request | On the merge | On a schedule |
| --- | --- | --- | --- |
| pack | `compare pack:<name>@<a> pack:<name>@<b>` as the comment | the registry holds the digest | — |
| profile | the comment, and the merge gate | `snapshot <name>@<version>` as the record | `verify` each record |
| cluster | the comment, and the merge gate | `snapshot cluster:<name>` after the apply | `verify` each record, and `compare` the cluster with it |

Strata takes no view on how the repositories are cut. It reads refs and
records, so one repository per level and one repository for everything ask the
same questions with the same commands.
