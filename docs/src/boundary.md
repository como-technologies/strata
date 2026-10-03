# Where each part belongs

Strata adds four things beside Palette, and it copies none. This page states
what Palette does, what strata adds, and which of the ideas around strata
belong in a pipeline, in Palette, or in a tool outside both.

## The rule that decides

A record in Git holds the content that the team approved, and a check against
Palette shows each change after the approval. The record lives outside Palette
for one reason: a copy that stays the same when the content changes is what
shows the change.

So a check that compares the content of today with the content that a reviewer
approved needs the copy from that day, and lives outside Palette. A check that
reports what Palette knows at the moment can live inside it.

## What Palette answers

Each row is a feature of Palette, the question it answers, and the question
that strata adds beside it. Strata does not answer the first question again.

| Palette feature | It answers | What strata adds |
| --- | --- | --- |
| [Update notifications](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/update-cluster-profile/) | Is the cluster behind its profile? "Cluster profile changes will generate an update notification on all the clusters that are created from the profile." | Does the content of the cluster match the release that the team approved? A value that a person sets on a cluster ([Update a cluster](https://docs.spectrocloud.com/clusters/cluster-management/cluster-updates/)) is a change of the cluster and not of the profile, so no update waits for it: see [Behind is not different](./loop.md#behind-is-not-different). |
| [The audit log](https://docs.spectrocloud.com/audit-logs/) and [its API](https://docs.spectrocloud.com/api/v1/v-1-audits-list/) | Who changed a resource, and when. | What the content was before and after. An entry names the change, such as the packs whose values changed, and the record holds the values. `verify --who` reads the log beside the record. |
| [The lock of a cluster template](https://docs.spectrocloud.com/cluster-templates/) | "Once a cluster profile version is linked to a cluster template, that version of the cluster profile becomes immutable." | A version that no template links: "Cluster templates can be used to deploy new clusters only." A cluster that exists runs versions outside a template. The lock covers the route of the whole version, and an edit through the route of one manifest changes a linked version, so `verify` covers both ([The lock of a cluster template](./internals/palette/profiles.md#the-lock-of-a-cluster-template)). |
| [Variables in use](https://docs.spectrocloud.com/profiles/cluster-profiles/create-cluster-profiles/define-profile-variables/modify-cluster-profile-variables/) | A change to the variables of a version that a cluster uses: "you must create a new version of your cluster profile before you can modify the variables". | A change to the packs, the values, or the manifests of that version, which the update request of the API takes ([Update a cluster profile](https://docs.spectrocloud.com/api/v1/v-1-cluster-profiles-update/)). |
| [Scans](https://docs.spectrocloud.com/clusters/cluster-management/compliance-scan/) | Is the running cluster compliant, secure, and conformant, and what software does it hold? | Is the cluster the release that the team approved? That is a different question: a cluster can pass each scan and run content from outside the review of the team. |
| [Export and import](https://docs.spectrocloud.com/profiles/cluster-profiles/export-import-cluster-profile/) | A copy of a profile as a file, by hand. "During profile export, Palette masks fields that are marked as sensitive." | A copy that a job compares with Palette on a schedule. `snapshot --from-candidate` reads the export as a candidate, so an export in the repository has a root. |
| [The Terraform provider](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs) | The publish of a version from code. | Whether Palette still holds what the code published. An edit through the API or the console is outside the plan. |
| [Health alerts](https://docs.spectrocloud.com/clusters/cluster-management/health-alerts/) | Is a cluster unhealthy? The alert resource of the provider takes `ClusterHealth` as its only component ([alert](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs/resources/alert)). | Did a published version change? The alerts cover the health of a cluster. |

Palette has an [Argo CD pack](https://docs.spectrocloud.com/integrations/packs/?pack=argo-cd),
which runs the applications of a cluster from Git. The profile itself is what
strata records in Git.

## What strata adds

Strata adds four things beside Palette:

1. **A check that a published version still holds its content.** A published
   version can be updated after its publish: the API takes an edit of the
   packs, the values, and the manifests, and the uid and the version number
   stay. The public documentation recommends a new version for a change: "We
   do not recommend updating a currently deployed cluster profile version to
   push out changes." A version also follows the registry: a version that
   keeps the values of a pack takes a push again of that pack version, a few
   minutes after Palette reads the registry.
   [Edits in place](./internals/palette/profiles.md#edits-in-place) gives what
   a live run shows. `verify` is that check.
2. **A digest of the whole version.** The API gives each pack a digest
   ([Get a cluster profile](https://docs.spectrocloud.com/api/v1/v-1-cluster-profiles-get/)).
   The root of strata is one digest over the whole version: its packs, its
   values, its manifests, and its variables. See
   [The hashes and the root](./internals/hashes.md).
3. **A report of each difference from the approved release.** Palette tells a
   cluster that its profile changed
   ([notifications](https://docs.spectrocloud.com/api/v1/v-1-notifications-obj-type-uid-list/)).
   Strata adds three answers beside it: whether a published version changed
   since its record, whether a cluster differs from the approved release, and
   whether two clusters on one release differ. `verify` and `compare` give
   them, and the scheduled jobs of [The everyday loop](./loop.md) keep them in
   front of a person.
4. **A copy of the approved content in the repository of the team.** Palette
   exports a profile as a file by hand
   ([Export and import](https://docs.spectrocloud.com/profiles/cluster-profiles/export-import-cluster-profile/)).
   The record is that copy, written by a job, with the root, and compared with
   Palette on a schedule.

## Where each idea belongs

The ideas around strata go to three places. The test is the rule at the top of
this page.

| Place | What goes there | Why there |
| --- | --- | --- |
| The pipeline of the team | The publish. The step that writes the record. The graph of the repositories, and who pins what. The approval rules. The check that a version number is new. The policy of the impact threshold, such as a second approver for a repave. | Each one is a decision of the team, and it changes when the team changes. Strata gives the facts, and the pipeline decides. |
| Palette | A content digest in the API. A setting that locks a published version. A lifecycle of a profile version, as packs have one. A report of the clusters that differ from their release. An event when a published version changes. | Each one is a fact that only Palette has at the moment of the change. A digest calculated when the version is published costs nothing, and every tool can then use it. |
| A tool outside both | The rules that make content canonical. The impact model. Each check that compares the content of today with the content that a reviewer approved. The comments, the issues, and the gates on the code host. | The copy lives outside the thing that it is compared with, and the code host is where the team reviews. This is what strata is. |

A feature in the second row does not remove strata. It makes the question
cheaper. A content digest in the API makes `verify` one request. The record
stays the copy of the approved content, whatever the content in Palette is
today.

`strata preflight` follows the same rule. Palette has validate routes, and the
console calls them in its create wizard. A create through the provider or the
API goes to the create route, and the checks of the cloud account, its quotas,
and its network are outside the routes. Strata calls the routes for you, reads
the entity and the records, and runs the tool of the cloud on your machine. It
calls no cloud API and keeps no credential. A check that belongs in Palette,
such as a cluster status that names the refusal of the cloud, is a row of the
catalog with its owner, and a findings report proposes it to Spectro Cloud.
See [How preflight works](./internals/preflight.md).

## The rules of the scope

Five rules decide what a new item of strata can do. The review of each item,
and of each wave, checks them.

1. **Ask Palette. Do not copy its rules.** When a validate route of Palette
   answers a check, strata and `strata-preflight` call that route. A copy of
   the rule goes stale when Palette changes the rule. Strata keeps a local rule
   only when the root needs it: for a change that Palette makes to the content
   and does not report.
2. **Do not repeat a status that Palette shows.** Palette shows flags and
   states in its console, its notifications, and its audit log. A record holds
   one of them only when `compare`, `verify`, or `same-release` needs it to
   explain a difference.
3. **`packs` serves the records and `behind`.** [`strata packs`](./commands/packs.md)
   reads the versions of a pack for a record and for `behind`. It is not a
   catalog of the registry.
4. **A check of build readiness goes to `strata-preflight`.** Palette has no
   such check, so `strata-preflight` is a product of its own beside strata. It
   follows rule 1: it calls the validate routes of Palette, and adds only the
   checks that the routes do not make.
5. **Each item names its question.** An issue names the question of
   [the table in the introduction](./introduction.md#the-model) that it
   answers. Or it names the part of build readiness that it adds. When an item
   does neither, the maintainers decide before the work starts.

## A pipeline that publishes

Strata publishes nothing. A pipeline that publishes has the other half of the
work: it builds the packs, publishes the profile versions, fans a new version
out to each repository that pins it, and applies the clusters. Strata is the
check at each step of that pipeline:

| Step of the pipeline | The question | Strata |
| --- | --- | --- |
| A merge request changes a profile | What does the change do to the clusters? | `snapshot --from-candidate`, `compare`, `pr-comment.sh` |
| The publish | Does Palette hold what the reviewer approved? | `snapshot`, then `same-release` of the candidate and the published version |
| The reconcile of a cluster | Does the cluster run the approved content? | `compare <record> cluster:<name>` |
| The removal of an old version | Does a cluster still use it? | `consumers` |
| A schedule | Did Palette change under a record? Does the registry have newer versions? | `verify-report.sh`, `upstream-report.sh` |

The reconcile step shows why the two halves differ. A pipeline that compares
the version string that a cluster holds with the version it pins finds a cluster
that is behind. It cannot find a cluster that holds the right version string and
runs other content, which is the case of
[A published version is edited in place](./use-cases/edited-in-place.md).
`compare` compares the content.

A version bump is the other place where the two halves meet. The provider can
keep each version under its own uid in two ways, and
[A version bump overwrites a version](./use-cases/overwritten.md#what-to-do)
shows both.
