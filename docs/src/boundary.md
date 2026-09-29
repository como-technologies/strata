# Where each part belongs

Strata adds four things to Palette, and it copies none. This page states what
Palette does already, what it does not do, and which of the ideas around strata
belong in a pipeline, in Palette, or in a tool outside both.

## The rule that decides

A record in Git, checked against Palette, is evidence that stays true when
Palette changes. A feature inside Palette cannot be the witness against Palette:
when Palette holds the wrong content, it also holds the wrong answer about that
content.

So a check that must work when Palette is the suspect lives outside Palette. A
check that only reports what Palette already knows can live inside it.

## What Palette does already

Each row is a feature of Palette, the question it answers, and the question it
does not answer. Strata answers the second question. It does not answer the
first again.

| Palette feature | It answers | It does not answer |
| --- | --- | --- |
| [Update notifications](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/update-cluster-profile/) | Is the cluster behind its profile? "Cluster profile changes will generate an update notification on all the clusters that are created from the profile." | Does the content of the cluster match a release that somebody approved? A value that a person overrides on a cluster ([Update a cluster](https://docs.spectrocloud.com/clusters/cluster-management/cluster-updates/)) opens no notification, and the cluster is then different with no update to wait for: see [Behind is not different](./loop.md#behind-is-not-different). |
| [The audit log](https://docs.spectrocloud.com/audit-logs/) and [its API](https://docs.spectrocloud.com/api/v1/v-1-audits-list/) | Who changed a resource, and when. | What the content was before and after. An entry holds a message, such as the names of the packs whose values changed, and not the values. It corroborates a drift. It cannot stop a change. |
| [The lock of a cluster template](https://docs.spectrocloud.com/cluster-templates/) | "Once a cluster profile version is linked to a cluster template, that version of the cluster profile becomes immutable." | A version that no template links. "Cluster templates can be used to deploy new clusters only." A cluster that exists runs versions that no template locks. The lock also covers one route of the API: an edit through the route of one manifest changes a linked version ([The lock of a cluster template](./internals/palette/profiles.md#the-lock-of-a-cluster-template)). |
| [Variables in use](https://docs.spectrocloud.com/profiles/cluster-profiles/create-cluster-profiles/define-profile-variables/modify-cluster-profile-variables/) | A change to the variables of a version that a cluster uses: "you must create a new version of your cluster profile before you can modify the variables". | A change to the packs, the values, or the manifests of that version. The update request of the API accepts it ([Update a cluster profile](https://docs.spectrocloud.com/api/v1/v-1-cluster-profiles-update/)). |
| [Scans](https://docs.spectrocloud.com/clusters/cluster-management/compliance-scan/) | Is the running cluster compliant, secure, and conformant, and what software does it hold? | Is the cluster the release that the team approved? That is a different question: a cluster can pass each scan and run content that nobody reviewed. |
| [Export and import](https://docs.spectrocloud.com/profiles/cluster-profiles/export-import-cluster-profile/) | A copy of a profile as a file, by hand. "During profile export, Palette masks fields that are marked as sensitive." | A copy that a job compares with Palette on a schedule. |
| [The Terraform provider](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs) | The publish of a version from code. | Whether Palette still holds what the code published: the API accepts an edit that the plan never sees. |
| [Health alerts](https://docs.spectrocloud.com/clusters/cluster-management/health-alerts/) | Is a cluster unhealthy? The alert resource of the provider takes `ClusterHealth` as its only component ([alert](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs/resources/alert)). | Did a published version change? No alert and no webhook fires for it. |

Palette has an [Argo CD pack](https://docs.spectrocloud.com/integrations/packs/?pack=argo-cd),
which runs the applications of a cluster from Git. It does not make Git the
source of a profile.

## What Palette does not have

Strata fills four absences:

1. **A published version can change.** The API accepts an edit of the packs,
   the values, and the manifests of a published version. The uid and the version
   number stay the same, and the documentation asks you not to do it: "We do not
   recommend updating a currently deployed cluster profile version to push out
   changes." Palette changes a version by itself too: a version that keeps the
   values of a pack follows a push again of that pack version, a few minutes
   after Palette reads the registry.
   [Edits in place](./internals/palette/profiles.md#edits-in-place) gives what
   a live run shows. `verify` exists because of this.
2. **No digest of the content.** The API gives each pack a digest, and no digest
   for the whole version: its packs, its values, its manifests, and its
   variables
   ([Get a cluster profile](https://docs.spectrocloud.com/api/v1/v-1-cluster-profiles-get/)).
   The root of strata is that digest. See
   [The hashes and the root](./internals/hashes.md).
3. **No drift report.** Palette tells a cluster that its profile changed
   ([notifications](https://docs.spectrocloud.com/api/v1/v-1-notifications-obj-type-uid-list/)).
   It does not tell anybody that a published version changed, that a cluster
   differs from an approved release, or that two clusters on one release
   differ.
   `verify` and `compare` give those answers, and the scheduled jobs of
   [The everyday loop](./loop.md) keep them in front of a person.
4. **No Git loop.** No feature of Palette keeps the approved content in the
   repository of the team and compares it with Palette. The nearest is an
   export by hand
   ([Export and import](https://docs.spectrocloud.com/profiles/cluster-profiles/export-import-cluster-profile/)).
   The record is that copy.

## Where each idea belongs

The ideas around strata go to three places. The test is the rule at the top of
this page.

| Place | What goes there | Why there |
| --- | --- | --- |
| The pipeline of the team | The publish. The step that writes the record. The graph of the repositories, and who pins what. The approval rules. The check that a version number is new. The policy of the impact threshold, such as a second approver for a repave. | Each one is a decision of the team, and it changes when the team changes. Strata gives the facts, and the pipeline decides. |
| Palette | A content digest in the API. Immutable published versions, as a setting, with no change by a person or by Palette itself. A lifecycle of a profile version, as packs have one. A report of the clusters that differ from their release. An event when a published version changes. | Each one is a fact that only Palette has at the moment of the change. A digest calculated when the version is published costs nothing, and every tool can then use it. |
| A tool outside both | The rules that make content canonical. The impact model. Each check that must work when Palette is the suspect. The comments, the issues, and the gates on the code host. | The witness must not depend on the thing that it checks, and the code host is where the team reviews. This is what strata is. |

A feature in the second row does not remove strata. It makes the question
cheaper. A content digest in the API makes `verify` one request. The record
stays the evidence, because the record does not change when Palette does.

`strata preflight` follows the same rule. Palette has validate routes, and a
create through the provider or the API calls few of them; no route checks what
the build needs from the cloud. Strata calls the routes for you, reads the
entity and the records, and runs the tool of the cloud on your machine. It
calls no cloud API and keeps no credential. What only Palette can fix, such as a
status that shows no error while the cloud refuses the build, is a row of the
catalog with its fix and its owner, and a findings report asks Palette for it.
See [How preflight works](./internals/preflight.md).

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
