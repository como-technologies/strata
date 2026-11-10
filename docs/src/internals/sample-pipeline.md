# Bring your own pipeline

Strata reads Palette and writes files. It publishes nothing and deletes nothing,
so it fits the pipeline that your team has now. This page says where strata
fits in any pipeline, and how to add its ideas to a pipeline that exists. Then it
maps strata to one sample pipeline, step by step.

## Where strata fits

Each row is an event of a pipeline, and the command that the pipeline runs at
that event. The command gives an exit code: `0` for the same, `1` for a
difference, and `2` for no answer. The pipeline decides what to do next.

| The event | The command | The question | Read |
| --- | --- | --- | --- |
| A publish | `strata snapshot <name>@<version> -o <record>` | What did the team publish? The record holds the content and its root. | [A publish fans out to what pins it](../use-cases/publish-fan-out.md) |
| A merge request | `pr-comment.sh <record> <candidate>` | What does the change do to the clusters? | [Pull request comments](../automation/pr-comments.md) |
| A promotion | `strata same-release <record> <record>` | Do the two projects hold one release? | [Promote a release to another project](../use-cases/promote.md) |
| A renumber | `strata same-release <record> <record>` | Is the content the same when only the version numbers change? | [A pre-release and its final version](#a-pre-release-and-its-final-version) |
| A bump | `strata behind <records>` | Which pins have a newer version in the registry? | [strata behind](../commands/behind.md) |
| A delete | `strata consumers <name>@<version>` | Does a cluster, an edge host, or a template still use the version? | [Retire a version](../use-cases/retire.md) |
| Drift | `strata verify <records>`, or the observer | Does Palette still hold the content of each record? | [Drift issues](../automation/drift-issues.md), [The observer](../automation/observer.md) |

A renumber moves a release to new version numbers and keeps its content. An
example is the move from a pre-release to its final version. Today
`same-release` passes a renumber only when no pack version changes. The option
`same-release --allow renumber` is an open item,
[#730](https://github.com/como-technologies/strata/issues/730). It passes a pair
whose pack versions and archive digests alone change. Until it merges, such a
pair is `DIFFERENT`.

## Add the ideas of strata to your pipeline

Your pipeline keeps its own rules: its graph, its approvals, its tests, and its
names of versions. Add three ideas of strata, one at a time.

1. **A record.** The publish step writes a record of each version that it
   publishes, and commits it to Git. The record is a copy of the content that the
   team approved. A check with no record has nothing to compare: see
   [What each level holds, and what it records](../use-cases/publish-fan-out.md#what-each-level-holds-and-what-it-records).
2. **A root.** Compare two versions by their roots, not by their names or their
   numbers. The root is one digest of the content. So a name that stays and
   content that changes give another root. See
   [The hashes and the root](./hashes.md).
3. **First-seen records, then approved records.** The versions that your
   pipeline published before strata have no record. Record what Palette holds
   now, with `strata snapshot`. Such a record is first-seen: it holds what
   Palette gave at that time, and no person approved it. A pull request of the
   records is the review that approves them. After the merge, the publish step writes the record of each new
   version. The observer does the same: see
   [First-seen records](../automation/observer.md#first-seen-records).

Strata has no command for a step that the table does not name. The pipeline of
the team owns the publish, the graph of its repositories, and its approval rules.
See [Where each idea belongs](../boundary.md#where-each-idea-belongs).

## A sample pipeline and its versions

The rest of this page maps strata to one pipeline, the pipeline of
`palette-pipeline-tools`. It is one opinionated example, and not a rule. Its team
decided that a version gets its final number only when its whole graph passes,
in its decision ADR-0009. Another pipeline names its versions in another way, and
the table of [Where strata fits](#where-strata-fits) still holds for it.

<div class="playground-demo">

`just playground sample-pipeline` serves the tenant of the merge request and the
merge, with no key and no cost. `just playground sample-pipeline-2` serves the
tenant after the final numbers.

</div>

### The repositories

The pipeline has one repository for each pack, each profile, and each cluster. A
merge in a pack repository publishes a pack version. Then a job opens a merge
request that bumps each profile that pins the pack. A publish of a profile does
the same for each cluster that pins the profile. A bump merges with no person,
except a move to a new major version of a pack.
[A publish fans out to what pins it](../use-cases/publish-fan-out.md) gives the
checks of this layout.

### The versions

The source of a pack or a profile holds its final version, three numbers. The
pipeline gives each publish a form of that version:

| When | The version | Where it goes |
| --- | --- | --- |
| Each pipeline of a merge request | the candidate, `<version>-mr<iid>.<pipeline>` | A pack goes to a sandbox registry. A profile goes into the Palette project. |
| A merge to `main` | the pre-release, `<version>-g<sha12>` | A pack goes to the dev registry. A profile goes into the Palette project. `<sha12>` is the first 12 characters of the merge commit. |
| Each member of the graph passes | the final version, `<version>`, and the git tag `v<version>` | The same places as the pre-release. |

Palette and strata order these versions by Semantic Versioning. A pre-release is
below its version. Two pre-releases of one version compare by their text. So
`1.1.2-g0123456789ab` is below `1.1.2-mr12.345`, because `g` is before `m`:

```text
{{#include ../output/pipeline-versions.txt}}
```

The text of a hash does not give the time of a build. So the sample pipeline
gives no order to two pre-releases of one version: the version of the last event
replaces the pin. See
[The order of versions](./palette/profiles.md#the-order-of-versions).

### The steps after the merge

1. **The stamp.** After the smoke tests on each CI cluster, the pipeline writes a
   stamp. A stamp is an annotated git tag with each pin and each result.
2. **The final numbers.** When each member of the graph passes, the pipeline
   gives the final numbers from the bottom up. It builds each pack again from the
   commit of its pre-release, and only `version` in `pack.json` changes. It
   writes a commit that moves each pin to its final version. Then it publishes the
   final profile version, compares it with the pre-release in Palette, and tags
   `v<version>`.
3. **The move of a cluster.** A cluster that moves its pin from a pre-release to
   its final version gets no apply and no repave. It takes the final version at
   its next apply.
4. **The sweep.** The sweep deletes the pre-releases and the old final versions
   that nothing needs. Each publish of a profile makes a profile version. Palette
   refuses a new profile in a tenant that holds the most profiles that its limit
   takes, with `ResourceLimitValidationFailure`: see
   [Errors](../reference/errors.md#refusals-that-strata-does-not-send-again).
   The sweep keeps the newest final versions, each version that a branch of a
   cluster repository pins, and each version that a cluster holds.
5. **The release to the next tier.** A release copies each pack to the registry
   of that tier, by its digest. Then it publishes the profile in the project of
   that tier. The sample pipeline does this step later, because it needs a
   Palette project for each tier.

### What strata answers at each step

| Step of the pipeline | Part of strata | Today |
| --- | --- | --- |
| The candidate merge request | `pr-comment.sh <record> <name>@<version>-mr<iid>.<pipeline>` | Fits: the candidate is a version in Palette, so the side B is a ref. |
| A merge that is a fast-forward | the merge gate | Not needed: the head of the merge request is the result. |
| A pre-release on each merge | `same-release` of the candidate and the pre-release, `behind`, and `<name>@latest` | Fits: the root does not cover the profile version, and a pre-release is below its version. |
| The final number and its check against the pre-release | `same-release`, `compare` | Fits when no pin moves. Does not fit when a pin moves: the roots differ, and `compare` grades the move of a pin as an upgrade. |
| The move of a cluster to the final version | `compare` | Does not fit, for the same reason. |
| The stamps and the graph | none | The pipeline of the team owns them. Strata publishes nothing. |
| The sweep | `consumers`, the retire gate | Fits as the check in Palette before a delete. The sweep also reads the pins in Git, which strata does not read. |
| Drift after the final | `verify-report.sh` | Fits when the pipeline keeps a record of each final version. The sample pipeline keeps none. |
| The release to the next tier | `same-release --allow registry-move`, the promotion gate | Fits: a copy to another registry is the case of `registry-move`. |
| The build of a cluster | `strata-preflight` | Does not fit: preflight reads the body of a create, or a cluster that Palette holds. A cluster repository applies a Terraform root, and strata does not read a Terraform plan. |

#### The candidate merge request

The candidate is a profile version in Palette. So the side B of the comment is a
ref, and the merge request needs no candidate file:

```text
{{#include ../output/pipeline-candidate.txt}}
```

See [Pull request comments](../automation/pr-comments.md).

#### The pre-release of a merge

The merge is a fast-forward, so the pre-release holds the content of the head of
the merge request. The root does not cover the profile version, so the candidate
and the pre-release have one root:

```text
{{#include ../output/pipeline-pre-release.txt}}
```

Exit `0`. A difference here shows that the pre-release holds other content than
the content that the review saw.

A pre-release is below its version. So `<name>@latest` takes the highest final
version, and not a pre-release above it:

```text
{{#include ../output/pipeline-latest.txt}}
```

`behind` names no pre-release as a newer version of a final pin. The record of
`aws-stack@1.1.1` pins `csi-aws-ebs` `1.65.0`, and the registry has the
pre-release `1.66.0-g4f1c2a9b7d30`:

```text
{{#include ../output/pipeline-behind-final.txt}}
```

See [A pre-release](../commands/behind.md#a-pre-release).

#### A pre-release and its final version

When the final version moves no pin, the pre-release and the final version have
one root. `aws-stack@1.1.2` changes a value of the profile and pins only final
pack versions:

```text
{{#include ../output/pipeline-final-same.txt}}
```

When the final version moves a pin, the roots differ. The version of each pack is
in the root. `aws-stack@1.1.3` moves `csi-aws-ebs` from its pre-release to its
final version:

```text
{{#include ../output/pipeline-final-moved.txt}}
```

Exit `1`. In Palette, the archive of the new build has a new digest too, because
its `pack.json` holds the new version. `compare` grades the move of each pin as a
new pack version: for the OS or the Kubernetes pack, `repave`. So strata does not
answer the move of a cluster to its final version either. Until
[#730](https://github.com/como-technologies/strata/issues/730) merges, the
pipeline makes this check itself, as the sample pipeline does.

Before the final commit, `behind` names each pin of a pre-release that has its
final version now. These are the pins that the final commit must move:

```text
{{#include ../output/pipeline-behind-pre-release-lines.txt}}
```

Exit `1`. See [A pre-release](../commands/behind.md#a-pre-release).

#### The sweep

`consumers` asks Palette if a cluster, an edge host, or a template uses the
version. Exit `0` lets the delete run:

```text
{{#include ../output/pipeline-sweep.txt}}
```

A CI cluster still holds the pre-release of `aws-stack@1.1.3`, because the move to
its final version gets no apply. Exit `1` stops the delete:

```text
{{#include ../output/pipeline-sweep-held.txt}}
```

Palette refuses that delete too: see
[What the delete of a version checks](./palette/profiles.md#what-the-delete-of-a-version-checks).
Strata reads Palette, and not the tier files in Git. So the sweep still reads the
pin of each branch. See [Retire a version](../use-cases/retire.md).

#### Drift after the final

The sample pipeline keeps no record of a final version. So this gap is in the
pipeline, and strata needs no change. Write the record when the pipeline publishes
the final version:

```bash
strata snapshot aws-stack@1.1.2 -o releases/1.1.2/dev.json
```

Then the job of each night checks it:

```text
{{#include ../output/pipeline-verify-lines.txt}}
```

See [Drift issues](../automation/drift-issues.md).

#### The release to the next tier

A copy by digest keeps the tag, the version, the values, and the digest of each
pack, and changes its registry. `same-release --allow registry-move` passes it.
This block comes from
[A pack comes from another registry](../use-cases/registry-move.md):

```text
{{#include ../output/move-allow.txt}}
```

#### The build of a cluster

`strata preflight` reads the body of a create, or a cluster that Palette holds:
see [strata preflight](../commands/preflight.md). A cluster repository of the
sample pipeline applies a Terraform root. Strata does not read a Terraform plan,
so the pipeline runs no preflight before a build.

### The gaps

Each gap names the item that closes it. When an item closes a gap, it changes the
row of the gap on this page.

- **The order of versions.** No gap now. The items
  [#489](https://github.com/como-technologies/strata/issues/489) and
  [#616](https://github.com/como-technologies/strata/issues/616) are in strata.
  Strata orders versions as Palette does: a pre-release is below its version, and
  a version that does not parse reads as its first three numbers.
- **A pre-release and its final version.** Open:
  [#730](https://github.com/como-technologies/strata/issues/730) adds
  `same-release --allow renumber`. In `compare`, it also gives the impact `none`
  to the move of a pin that only changes its version.
- **A record of each final version.** A gap of the pipeline: the publish of a
  final version writes no record. No item of strata is necessary.
- **The build of a cluster from a Terraform root.** Strata does not read a
  Terraform plan. No item of strata closes this gap.
