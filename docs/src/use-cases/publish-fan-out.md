# A publish fans out to what pins it

## The problem

Some teams keep one repository per pack, one per profile, and one per cluster.
A merge in a pack repository publishes a pack version. A job then opens a bump
pull request on each profile that pins that pack. Merging a bump publishes a
profile version, and that publish opens a bump on each cluster that pins the
profile. Nobody types a version number: the chain does.

This is the same problem as [A team pins each environment](./fan-out.md), one
level up. Many pull requests are open at once, most of them opened by a job,
and each one was written against a level below that keeps moving. GitLab calls
a pull request a merge request; this page says pull request for both.

**Who runs this.** Jobs at every level. A person merges, and a job publishes,
records, and opens the next pull request. The recipes are
[Pull request comments](../automation/pr-comments.md),
[The merge gate](../automation/merge-gate.md),
[Drift issues](../automation/drift-issues.md), and
[The promotion gate and the retire gate](../automation/gates.md). The record
step in the publish job is yours to write: strata ships no recipe for it,
because the publish itself is yours.

**Follow along.** This page takes its blocks from the pages it links, so no one
page serves all of it. Each block below runs on the seed that its own page
names: `just playground packs`, `just playground list`,
`just playground cluster-release`, and `just playground edited-in-place-2` for
the night verify. The merge gate reads a git repository, which the playground
does not build.

## What each level holds, and what it records

| Repository | The repository holds | A merge publishes | The record |
| --- | --- | --- | --- |
| pack | the pack content and its version | a pack version in a registry | the registry digest, which `pack:` reads |
| profile | the pack versions that the profile pins | a profile version in Palette | `strata snapshot <name>@<version>` |
| cluster | the profile versions that the cluster pins | the cluster | `strata snapshot cluster:<name>` |

A publish has these steps: build, test, sign, publish, and **record**. The record is the
artifact of the publish, and the publish job writes it:

```bash
# in the publish job of a profile repository, after Palette holds the version
strata snapshot aws-stack@1.1.1 -o releases/1.1.1/dev.json
```

Without that file, a later question has nothing to ask. With it, every check
below is one command.

## Why a record, when the registry has a digest

A published version is immutable by the rule of the team. The API takes an
update of a published profile version, under the same uid and the same version,
so the gate checks the rule. See
[A published version is edited in place](./edited-in-place.md) and the live run
in [What Palette does](../internals/palette.md).

The record is what turns the rule into a check:

```text
{{#include ../output/edited-verify.txt}}
```

Exit `1`. A scheduled job over every record keeps one issue open per record that
drifted, on either host. See [Drift issues](../automation/drift-issues.md).

## Review a bump that a job opened

A bump pull request says "1.35.3 becomes 1.36.2". That is not a review. The
comparison is:

```text
{{#include ../output/pack-compare.txt}}
```

The two lines a reviewer needs are the first two: the root changed, and the
pack is `IDENTITY UPSTREAM AUTHORED`, so the build, its digest and its values
all moved. The value lines below them say which.

Put it in the pull request with
[`pr-comment.sh`](../automation/pr-comments.md), which posts one comment on
GitHub or one note on GitLab, and labels the pull request when the impact
reaches `repave`. The reviewer reads what the bump does to a node, not a
version string.

## Two bumps race, and the second one wins

The fan-out opens a bump against the profile as it was at the publish. While it
waits for review, another bump merges. The second pull request is now behind,
and merging it puts the earlier pack version back.

[The merge gate](../automation/merge-gate.md) is the check for this. It merges
the target branch in first, and reads the result:

```text
{{#include ../output/merge-gate-behind.txt}}
```

A job that lowers a pin on purpose — a pack version is withdrawn, so the bump
pins the one before it — writes the declaration in the commit it makes:

```text
Rollback: releases/1.1.1/dev.json to aws-stack@1.1.0

csi-aws-ebs 1.65.0 was withdrawn: the storage class and the driver disagree
```

The gate then reports the rollback and passes. A human doing the same thing
writes the same line, or a reviewer puts the `rollback` label on the pull
request. Nothing else turns a backwards pin green, so no bump slips through
because a job was running from an old base.

## Before you push

Every check above also runs in a terminal, which is where a developer wants
them: before the push, not after the pipeline.

The gate is a script of the strata release, not of your repository. Get it once,
beside the binary:

```bash
# from a release of strata, into .strata/ of your checkout
bash install-strata.sh --version v0.22.0 --dir .strata
```

```bash
# what do the environments run right now, and does any record no longer match?
strata verify "releases/**/*.json"

# what would my branch land, against the default branch as it is now?
.strata/merge-gate.sh "releases/**/*.json" --base main
```

The gate writes to no host and needs no token: it fetches the target branch,
merges it into the checkout, and prints the same report the pipeline prints.
A developer sees the `BEHIND` record before a reviewer does, and the pipeline
then confirms it.

## Before you retire a pack version

The chain also answers the question that comes at the end of a version's life.
Strata retires nothing: it names the clusters that still run a version, so a
person or a job can decide. The block gives that answer for three versions of
one profile:

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
when it was attached, so the profile record does not answer for the cluster.
Give the cluster a record of its own, and check that one:

```text
{{#include ../output/cluster-run-record.txt}}
```

`IDENTICAL` and exit `0`: this cluster runs what it was given. The record names
the profile it selected, `payments/aws-stack`, because `payments` runs two
profiles.

Record each cluster after its apply, and the scheduled verify covers the
clusters as well as the profiles. A cluster that drifts from its own record,
and a version that changed under the cluster, are different findings:
see [A cluster does not run its release](./cluster-release.md) and
[A published version is edited in place](./edited-in-place.md).

## The checks at each level

| Repository | On a pull request | On the merge | On a schedule |
| --- | --- | --- | --- |
| pack | `compare pack:<registry>/<name>@<a> pack:<registry>/<name>@<b>` as the comment | the registry holds the digest | — |
| profile | the comment, and the merge gate | `snapshot <name>@<version>` as the record | `verify` each record |
| cluster | the comment, and the merge gate | `snapshot cluster:<name>` after the apply | `verify` each record, and `compare` the cluster with it |

## What to do

- Write the record in the publish job, at each level. A check that has no record
  has nothing to ask.
- Compare the two versions in the bump pull request, and let the gate read the
  merge result.
- Run the same checks in a terminal before the push.
- Strata makes no rule about how you divide the repositories. It reads refs and
  records, so one repository for each level and one repository for everything
  ask the same questions, with the same commands.
