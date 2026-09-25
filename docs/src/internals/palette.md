# What Palette does

Strata is built around the behavior of the Palette API. This page states that
behavior, and what strata does about each part of it. Spectro Cloud documents
little of this, so each answer here comes from reading a real tenant: see
[The live example](../development/live-example.md).

Read this page when you want to know why strata reports something the way it
does, or before you change how it reads a profile.

Each uid on this page is an example. The pack names, the versions, and the
digests are real.

## The pack-ref digest

On a published profile, the `digest` that Palette puts on each pack reference is
the digest that Palette lists for that pack version. For a pack in an OCI pack
registry, and for a chart in an OCI Helm registry, that digest is the manifest
digest of the push.

| Pack | Type, registry | Version | Pack-ref digest | What the digest is |
| --- | --- | --- | --- | --- |
| ubuntu-aws | `spectro`, pack registry | 24.04 | `sha256:2df05daa4eb4...` | the digest of the pack registry for the version, as Palette lists it |
| kubernetes | `spectro`, pack registry | 1.36.2 | `sha256:c4f7f20f40c7...` | the same |
| cni-cilium-oss | `spectro`, pack registry | 1.20.1 | `sha256:917d9de9a696...` | the same |
| csi-aws-ebs | `spectro`, pack registry | 1.65.0 | `sha256:bd09e8b4187b...` | the same |
| nginx | `helm`, Helm registry | 25.1.5 | `08816cde6bea...` | the sha256 of the chart archive, as the repository index records it |
| argocd | `oci`, OCI pack registry | 10.4.0.1 | `sha256:99cac22f712b...` | the manifest digest of `<base path>/spectro-packs/archive/argocd:10.4.0.1` |

So the pack-ref digest is the digest of the pack version as Palette read it. For
a pack in an OCI pack registry it follows each push. For a chart in an OCI Helm
registry it is the digest of the first push that Palette read: see
[A pack version pushed again](#a-pack-version-pushed-again). **Strata reads no
registry of its own for a published profile: the profile document carries what
it needs.**

- A Helm digest is bare hex, with no `sha256:` prefix.
- A chart in an OCI Helm registry that Palette reads itself, the example's own
  in ECR, is a pack of type `oci`, and its digest is the manifest digest of the
  first `helm push` Palette read, as for a pack in an OCI pack registry. It is
  not the sha256 of the chart archive, and a push again does not move it: see
  [A pack version pushed again](#a-pack-version-pushed-again).
- Palette takes the digest from its own copy of the registry, which a sync
  refreshes. `@latest` is the newest version of that copy.

## A pack that requires another pack

`pack.json` can name a pack that this pack requires: `constraints.dependencies`,
with `type: required` and a least version
([Pack constraints](https://docs.spectrocloud.com/registries-and-packs/pack-constraints/)).
In the example's own OCI pack registry, `strata-live-pack` 1.1.0 requires
`strata-live-dep` 1.0.0 or newer. A profile that holds the requiring pack and
not the required one is refused at creation:
`ClusterProfileCreationValidationFailure`, "Pack 'strata-live-dep' is required
with minimum compatible version '1.0.0'". A profile that holds both is
accepted, and its record lists the two packs in the order authored.

So a required dependency is met inside the profile. It does not reach the other
profiles of a cluster, because a profile that lacks the required pack cannot
exist. A record carries no constraint: the profile Palette accepted is the one
that holds it.

## Pack schema and presets

Palette stores neither the pack schema nor the presets on a profile. They belong
to the pack version in the registry.

So a change to a schema or a preset under one version is a change to the content
of the pack. The digest that Palette lists for the version follows it, and a
published profile keeps the digest it was pinned with. A preset that a profile
applies goes into the values of that profile, where strata reads it. Strata
applies no pack preset of its own. What the values of a profile must carry for a
preset is open: see [What no run covers](#what-no-run-covers).

## A pack that moves between registries

The pack type follows the registry: `spectro` for a pack registry, `helm` for a
Helm registry, and `oci` for an OCI registry. When a team re-pins a pack to
another registry, four fields change and the content does not:

| Field | From a pack registry | From an OCI registry |
| --- | --- | --- |
| type | `spectro` | `oci` |
| registryUid | `5eecc89d0b150045ae661cef` | `64ec000000000000000000a1` |
| packUid | `6aa410ec144aaec79d547a9e` | `64ec000000000000000000b1` |
| digest | `sha256:bd09e8b4187b...` | `sha256:a746e1468908...` |
| tag, version | `1.65.0` | `1.65.0` |
| values, manifests, params | | unchanged |

A pack pushed to an OCI registry gets the digest of that push
([Add a pack to an ECR registry](https://docs.spectrocloud.com/registries-and-packs/registries/oci-registry/add-pack-oci/add-pack-oci-ecr/)).
That is not the digest a pack registry lists for the same version.
So between two kinds of registry the digest differs too, as the table shows.

So `same-release --allow registry-move` accepts a type change between two
registries, and the pair passes as `REGISTRY MOVE`. It refuses a `manifest`
pack, which comes from no registry. It still checks the content: after
an edit of the CSI values in place on the moved version, the check fails and names
`charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`.

## A pack version pushed again

A pack version can be pushed again under the same tag, with other bytes. In an
OCI pack registry of the example's own, after Palette reads the registry again:

| What | After the second push |
| --- | --- |
| The pack version Palette lists | The digest of the new push, under the same version and the same pack uid. |
| A record of the pack from before | `verify` reports `DRIFT`: the digest moved. |
| A published profile that pins the version | Keeps the digest it was pinned with. Its record verifies after the sync, and after the next push and sync too. |

So a profile pins one build. A registry that changes under the version does not
move the profile. A record of the pack and a record of a profile answer two
different questions, and `verify` on the pack record is what finds such a push.

A new version pushed beside the pinned one, `1.0.1` beside `1.0.0`, does not
move a profile pinned to `1.0.0` either. A profile pinned under a floating tag
is the other case: see [Tag names](#tag-names).

An OCI Helm registry of the example's own answers the same push differently.
The first time Palette reads a chart version, it lists the manifest digest of
the push, and a profile that pins the chart carries it. A push again under the
same version, with other bytes, and a sync after it, change nothing: Palette
keeps the digest it listed first, and a record of the chart from before the
push verifies. A sync does read a new version: `1.0.1` pushed
beside `1.0.0` is listed, newest first. So for a chart in an OCI Helm registry,
`verify` on a record of the chart says what Palette lists, and not what the
registry serves now. A pack in an OCI pack registry follows each push.

## Tag names

A registry can give one pack build several tags.
`GET /v1/packs/<name>/registries/<uid>` lists several tags for one build of
`ubuntu-aws`. A profile from the Spectro Cloud provider pins `24.04`, and the
example publishes the same build under `LTS__24.04` and under `LTS__24.x`. All
three hold one pack uid and one version. Palette stores the tag that the author
picked, and the version that the tag named, so two profiles on one build can
hold two tags. A record carries both.

Two versions that differ only in that tag run the same build:

| Question | Answer |
| --- | --- |
| What does Palette store? | The tag as authored, with one pack uid and one version behind all of them. |
| Does Palette repave a cluster that moves between them? (`POST /v1/spectroclusters/<uid>/validate/repave`) | No. The control, one more entry in `kubeadmconfig.postKubeadmCommands`, repaves: `PackValuesUpdated`. |
| What happens to the nodes on such a move, and on the move back? | The same EC2 instances throughout. The cluster is `Running` on the target version, and stays so. |
| Can the tag be renamed on a published version? | Yes, through the API, and the pack uid stays. A record from before the rename reports `DRIFT`, and the pair is `SAME CONTENT`. |

The repave check answers "no repave" for a request whose only change is the tag
name of the OS pack, exact or floating. **A tag in the request is no control for
that check; a change of the values is.**

A floating tag moves a published profile. In an OCI pack registry of the
example's own, Palette lists `1.0.x` and `1.x` beside the version `1.0.1`. A
profile pins the pack under `1.0.x`. Then `1.0.2` is pushed, and the tag names
it. Within ten minutes of the sync, the published profile moves: `verify` on a
record of the profile from before reports `DRIFT` with `IDENTITY UPSTREAM`. No
person acted, and no publish happened. A profile pinned to the exact version
`1.0.0` does not move when `1.0.1` arrives.

## Edits in place

A person can edit a published profile version through the API
([Update a cluster profile](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/update-cluster-profile/)).
The version keeps its uid and its number, stays published, and no new version
appears. A pack can also be pushed again under one version, with
`spectro pack push --force`
([Spectro CLI reference](https://docs.spectrocloud.com/registries-and-packs/spectro-cli-reference/))
or `oras push`. A published profile that pins the version keeps the digest it
was pinned with, in an OCI registry: see
[A pack version pushed again](#a-pack-version-pushed-again). A Spectro pack
registry is not covered: see [What no run covers](#what-no-run-covers).

This is why [`strata verify`](../commands/verify.md) exists. A record of a
version from before a person's edit, or from before a floating tag moved, exits
`1` with `DRIFT`, and names the change. A record of a cluster's copy from before
the cluster accepted an update does the same. `DRIFT` says that the object
changed. The audit log of Palette says who changed it, and when: see
[The audit log](#the-audit-log).

An edit through the API keeps the uid of each manifest. The live example sends
each uid back, and a cluster copy still names it.

A reorder is an edit in place too. A `PUT` of a published version with the same
packs, values, and manifests in the other order is accepted, stored in the new
order, and audited as an update of the profile. Palette accepts it for the four
infrastructure layers of a version a cluster runs as well: the CSI layer first
and the OS layer last. `verify` on a record from before reports `DRIFT` and
`note: pack order changed between the two profiles`.

## The audit log

Palette logs each write to a profile and to a cluster.
`GET /v1/audits?resourceKind=<kind>&resourceUid=<uid>&startTime=<time>` lists
the writes to one resource since a time, newest first, at most 100 on a page.
`listmeta.continue` names the next page, and is empty on the last one. The kind
is `clusterprofile` or `spectrocluster`. Each entry gives the time, the action
(`create`, `update`, `delete`, `activity`, or `action`), and the actor: a user,
with a name and a uid, or a Palette service, with a name. Some entries also
give `contentMsg`, a summary of the change. An entry has a uid, and no request
id. [`verify --who`](../commands/verify.md#who-changed-it) reads this log.

An edit in place through the API, a `PUT` of the version and then its publish,
is one entry: an `update` by the user who owns the key, with a summary that
names the manifest that changed.

The log does not hold the content of the change. The record holds it.

A floating tag that moves is logged too. When a profile pins a pack under
`1.0.x` and the registry gets `1.0.2`, the sync that reads it gives the profile
two entries: an `update` by the Palette service `system` with no summary, and
one whose summary names the pack and both versions, such as `version is updated
1.0.1 > 1.0.2`. No person is in either entry.

A cluster that accepts an update through the API gives the cluster one entry:
an `update` by the user who sent the `PUT`, with a summary that names the uid
of the profile and the manifest that changed. The entry names who accepted the
update. The edit of the profile before it is an entry of the profile.

## Macros

A macro is a name and a value, with no version, no history, and no digest
([Macros](https://docs.spectrocloud.com/clusters/cluster-management/macros/)).
`GET /v1/projects/<uid>/macros` lists the macros of a project; `POST` adds one,
`PATCH` changes one, and `DELETE` removes one.

A profile whose manifest reads a user macro, `{{.spectro.macro.<name>}}`, has
`status.hasUserMacros: true`. A profile that reads none has `false`.

`GET /v1/spectroclusters/<uid>/packs/resolvedValues` gives, for each profile a
cluster runs, a map from each placeholder to the value Palette gives it now.
The map holds every macro Palette can resolve for the profile, the system ones
too, such as the cluster name and the region, and not only the ones the profile
reads. It gives a new macro value at once.

When a macro gets another value:

- The profile and its record do not change: `verify` of the record reports `OK`.
- The copy that a cluster holds does not change: `verify` of a cluster record
  without the digests of its macro values reports `OK`.
- No notification opens for the cluster, and the audit log of the cluster
  holds no entry for it.

The cluster still gets the new value. Palette writes it into the running
cluster on its own, with no update to accept: a live run read it inside the
cluster 11 minutes after the change, and a run before it 13 minutes after. So a
cluster runs other content, and its record, its copy, its notifications, and
its audit log say nothing. The digests of `snapshot --macros` are how strata
sees it: see [Macros](../commands/snapshot.md#macros).

A create, a change, or a delete of a project macro writes no audit entry, at
project or tenant scope. The audit log has no resource kind for a macro.

When a macro is in a field that repaves a cluster, such as
`kubeadmconfig.postKubeadmCommands` in the Kubernetes pack, a change of its
value does not replace the nodes. At the next read of the agent, Palette puts
the cluster in a repave that waits for approval (`status.repave.state:
Pending`), one agent poll after the change. It opens no notification for it.
`GET /v1/spectroclusters/<uid>/repave/status` gives the record: the source is
`hubble`, and not a person. The one reason is `PackValuesUpdated`, `Pack
'kubernetes' values are updated`, with the key that differs and the resolved
values on each side. It names no macro, so nothing in the record says that a
macro caused it. While the repave waits, Palette
refuses an update of the cluster profiles: `Cluster update failure as cluster
repave is pending for approval`. Palette also refuses to delete a macro that a
cluster reads.

A repave that waits for approval also holds a delete of the cluster. The delete
puts the cluster in `Deleting`, and the agent removes nothing while the repave
waits: in a live run, the cluster stayed in `Deleting` for 37 minutes, its EC2
instances kept running, and `tofu destroy` failed with `context deadline
exceeded`. After an approval of the repave, the instances were gone in four
minutes. The approval writes no audit entry.

## Cluster copies

Each cluster runs its own copy of the manifests of a profile. The copy has its
own uid, and `GET /v1/spectroclusters/<uid>/profiles/packs/manifests` serves it.
A `cluster:<uid>` ref records that copy. **So a cluster record shows what the
cluster got, and not what its profile holds now.**

What follows from that, around an edit in place. The
`immutable-clusterprofiles` flag of the Spectro Cloud provider changes none of
it:

| Step | What Palette does |
| --- | --- |
| Edit a published version in place | The API accepts it. With the flag, the provider refuses the same edit at plan time; without it, the provider plans an update in place. |
| A cluster that ran before the edit | Keeps its old copy. `verify` passes on its record, and the record of the version compares `IDENTICAL` with it. |
| The record of the version from before the edit | `verify` reports `DRIFT`. |
| A cluster created after the edit | Runs the edited content. A comparison of the two clusters shows the change, and notes that both sides are the same version. |
| The update pushed to the first cluster, with its notification left open | Its copy takes the edit within three minutes. |
| The update accepted on the first cluster | The two clusters match. The record from before the edit reports `DRIFT`. |

So `compare` of two cluster records shows a change exactly when the clusters run
different content, and strata needs no record of the updates that wait for a
cluster.

A copy has the variables of its profile too. `GET
/v1/spectroclusters/<uid>/variables` is an array with one entry for each applied
profile that declares a variable: the profile uid, and each variable with the
fields that the profile endpoint serves, plus `value`, the value the cluster
gives it. So the copy of a profile with a variable has the variables hash of the
profile, and its root equals the root of the profile. The value that a cluster
gives is in the record, under `source.cluster.variables`, and not in the root.

A change of that value is a change to the cluster, and not to the copy. The
Spectro Cloud provider sends it as `variables` of the profile on the cluster
resource. The audit log gets a new entry for the cluster, and its action says
who "updated the cluster variables on the cluster `<name>`". The variables endpoint
gives the new value. A record of the copy from before verifies, and a comparison
of the records from before and after reports the copy unchanged and one
variable value changed.

Palette says when a copy is behind its profile
([Cluster updates](https://docs.spectrocloud.com/clusters/cluster-management/cluster-updates/)).
`GET /v1/notifications/spectrocluster/<uid>` lists the notifications of a
cluster, newest first, with `items: null` when there are none. An edit in place
of a profile that the cluster runs opens one notification: `action.actionType`
is `NotifyActionClusterProfileUpdate`, `action.isDone` is `false`, and
`action.events` names the profile uid and each change:

| Change | Event `type` | `message` |
| --- | --- | --- |
| A manifest | `PackManifestUpdate` | `Manifest '<m>' is updated in the pack '<p>(<p>)'` |
| Pack values | `PackValuesUpdate` | `<display name> <pack> <version> values are updated` |

The notification is done (`isDone: true`) when:

- The cluster takes the change. A push of the content without
  `resolveNotification` closes it too.
- An edit gives the profile the content of the cluster's copy again. The
  cluster does not update.

A new version number alone, in place, opens no notification: the copy holds
the same content. A notification that is done stays in the list.

`GET /v1/spectroclusters/<uid>/profileUpdates` is no flag: it lists each profile
of the cluster also when no update waits. So strata reads the notifications,
and a cluster record says whether an update waits: see
[Whether an update waits](../commands/snapshot.md#whether-an-update-waits).

## The order of the profiles of a cluster

`spec.clusterProfileTemplates` of a cluster lists its profiles in the order the
request that made the cluster gave them. That order carries no meaning. Since
provider 0.28.1, the Spectro Cloud provider sends `cluster_profile` as a set,
hashed over the whole block with the profile uid in it. A new uid moves a
profile to another position. So two clusters built from one definition can list
the profiles in two orders. So strata orders them itself: a cluster record lists
the cluster or infra profile first and then the others by name, and merges their
packs in that order, so two clusters built from one definition have one merged
root. A check of what a cluster runs compares the set of profiles, and [the
knurl adapter](../automation/knurl.md) gives the copies that a cluster holds by
name.

A `PUT` of a cluster's profiles in the other order, with the uids it has,
changes nothing: the document keeps the stored order. The audit log gets an
update of the cluster. So the order strata gives a cluster record
drops nothing that can change: the stored order is set when the cluster is
made, and stays.

That order is no install order either. No run establishes the install order of
the packs of two add-on profiles: see
[What no run covers](#what-no-run-covers).

The order of the packs inside a profile is no install order either. Each add-on
pack has an install priority: the key `spectrocloud.com/install-priority` under
`pack:` in its values, `0` unless set. The lowest value installs first. Packs
with one value install together, and the next value waits for their readiness
checks
([Install Order](https://docs.spectrocloud.com/profiles/cluster-profiles/create-cluster-profiles/create-addon-profile/#install-order)).
The priority is in the values, so the values hash and the root cover it, and a
change to it is an `AUTHORED` change. The list order is content because Palette
keeps it: two orders are two published versions.

## The lock of a cluster template

A cluster template links profile versions and a maintenance policy, and applies
them to the clusters attached to it
([Cluster templates](https://docs.spectrocloud.com/cluster-templates/)). The
documentation says that a linked version becomes immutable. A live run shows
what that lock is:

| Request | Answer |
| --- | --- |
| An edit in place of a version that a template links, through the API | Refused: `ClusterProfileUpdateValidationFailure`, "cluster profile is associated with 1 cluster templates (`<uid>`) and cannot be modified". The version verifies. |
| A cluster outside the template, built with a linked version | Refused: `ClusterCreateWithTemplateProfileForbidden`, "Cluster creation is not allowed as profile ... is already attached to one or more cluster templates". |
| The same edit, after the template is removed | Accepted. A record from before reports `DRIFT`. |

So the lock belongs to the template, not to the version. It holds while the
template links the version, and it keeps every cluster outside the template
away from that version. A cluster that runs a linked version is a cluster the
template applies.

## Version bumps

A version bump behaves differently with the provider's
`immutable-clusterprofiles` flag:

| Result | Flag off | Flag on |
| --- | --- | --- |
| The uid | Keeps the uid, which names the new version. | The new version is a clone under a new uid. The old version stays under the old uid. |
| `verify` of the old record | Reports it overwritten by the new version. | Reports the edit in place, and nothing overwritten. |
| The clusters | Keep their copies, which name the old version, and stay consumers of the uid. | Keep their copies, which match what the old uid holds, and stay consumers of the old uid. |

The clone carries the content over exactly: the values, the manifests, and the
variables of a flag-on bump equal those of a flag-off bump from the same
configuration.

A version is a semantic version. Palette normalizes a short form and a lowercase
`v` prefix, `1.2` to `1.2.0` and `v1.2.3` to `1.2.3`, and refuses a malformed
string. Palette checks the version when it is created and when it is changed,
and the provider does not
([Version a cluster profile](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/version-cluster-profile/)).

With the flag on, set `skip_destroy` on each profile **before** the first bump.
A flag-on bump replaces the profile resource, and the provider deletes the
replaced version with the `skip_destroy` value already in its state, not the new
value (provider 0.30.1, `resourceClusterProfileDelete`). Without it, the bump asks Palette to delete the version that a cluster runs.

## Manifest pack uids

Palette gives the manifest pack of every profile it creates a new pack uid, a
clone included. A publish keeps the uid, and a clone mints one. A pack from a
registry keeps the uid of the registry.

So two profiles built from one configuration can hold one manifest under two
pack uids, and their roots then differ although the content is the same. This is
why [`same-release`](../commands/same-release.md) passes such a pair and the
equality of roots does not: see
[One manifest in two profiles](../use-cases/manifest-pack.md).

## What a registry answers about a pack

`GET /v1/packs/<name>/registries/<uid>` answers with everything the registry
holds for one pack, in one request:

| Field | Holds |
| --- | --- |
| `layer`, `cloudTypes` | the layer of the pack, and the clouds it supports |
| `tags[]` | one row per tag: `tag`, `version`, `packUid`, and `parentTags` |
| `packValues[]` | one row per build: `packUid`, `values`, `schema`, `presets`, `readme`, `annotations`, `dependencies` |

A version carries its exact tag and the floating tags above it, each as its own
row, with one pack uid for the build behind them. `parentTags` names the tags
above that one
([Palette API reference](https://docs.spectrocloud.com/api/v1/)):

```text
{"packUid":"<uid>","parentTags":null,                 "tag":"1.x",    "version":"1.65.0"}
{"packUid":"<uid>","parentTags":["1.x"],              "tag":"1.65.x", "version":"1.65.0"}
{"packUid":"<uid>","parentTags":["1.65.x","1.x"],     "tag":"1.65.0", "version":"1.65.0"}
```

So the newest version of a line is the version that the floating tag names, and
one request gives every version, every tag, and no repeats. The paged listing
`/v1/packs?filters=spec.name=<name>` holds the digest of each build, which this
answer does not, and it repeats a version across pages.

The listing can name a registry that then answers 404 for the pack. It does so
for a pack that Spectro Cloud disabled or deleted: see
[Pack states](#pack-states).

## Pack states

Spectro Cloud retires a pack version in the registries it owns. The listing
`/v1/packs?filters=spec.name=<name>` keeps each retired version, and gives it a
state in `spec.annotations.system_state`, with the time in
`system_deprecatedTimestamp` or `system_disabledTimestamp`:

| State | `GET /v1/packs/<name>/registries/<uid>` | A new profile that pins it |
| --- | --- | --- |
| none | the tags and the values | Palette creates it |
| `deprecated` | the tags and the values | Palette creates it |
| `disabled` | 404, `ResourceNotFound` | Palette refuses it: HTTP 500, `ClusterProfileInvalidPackState`, "pack `<name>:<version>` is disabled" |
| `deleted` | 404, `ResourceNotFound` | Palette refuses it: HTTP 500, `ClusterProfileInvalidPackState`, "pack `<name>:<version>` is deleted" |

So a 404 for a pack that the listing names is a fact about one registry, and
not about the pack: `nginx` is disabled in `Public Repo` and in `Palette
Registry`, and `Bitnami` serves it. Strata reads the state from the listing:

- `strata packs` gives the state of each version, and lists a registry that
  answers 404 from the listing, with no tags. See
  [strata packs](../commands/packs.md#what-to-expect).
- `strata snapshot pack:` and a candidate stop with `pack-unavailable` on a
  disabled or a deleted version, because Palette refuses a profile that pins
  it.
- A deprecated version records as usual.

[The live example checks each state](../development/live-runs.md#pack-statessh).

Palette applies a pack constraint when a profile is created, and not through this
answer: see [A pack that requires another pack](#a-pack-that-requires-another-pack).

## What the cluster listing answers

`GET /v1/dashboard/spectroclusters/meta` is the listing that `strata list
clusters` reads. **It names no profile.** Each entry holds:

| Part | Fields |
| --- | --- |
| `metadata` | `name`, `uid`, `creationTimestamp`, `lastModifiedTimestamp`, `deletionTimestamp`, the labels, and the annotations |
| `spec` | `cloudType`, `cloudAccountUid`, `cloudRegion`, `clusterType`, `archType`, `location`, and `tags` |
| `status` | `state` |

So the profiles that a cluster applies need the cluster document itself,
`GET /v1/spectroclusters/{uid}`, and its `spec.clusterProfileTemplates`. That is
one request for each cluster, which is why `strata list clusters --applied` asks
for them and a plain `strata list` does not.

The listing names no health either, and neither does the cluster document. The
cluster summary, `GET /v1/dashboard/spectroclusters/{uid}`, gives
`status.health.state`: `Healthy` or `UnHealthy`, with the conditions that made
it so. [The knurl adapter](../automation/knurl.md) reads it there.

The consumer list of a profile version, `status.inUseClusters`, is what Palette
computes when it is asked, in the projects the key can read. So `consumers`
reads each cluster the key can list as well, and names a cluster that applies
the version and is not in the list.

The listing answers a JSON array at the top level, and not an object with
`items` ([Palette API reference](https://docs.spectrocloud.com/api/v1/)). Strata
reads both shapes.

A deleted cluster leaves this listing and keeps its document:
`GET /v1/spectroclusters/<uid>` still answers `200`, with the state `Deleted`.
So `cluster:<name>`, which strata resolves through the listing, does not go
ambiguous when one name is deleted and built again.

A deletion through the API behaves in the same way. `DELETE
/v1/spectroclusters/<uid>` is accepted at once. While the cluster goes, the
listing keeps it with the state `Deleting`, and `consumers` lists it with that
state and exits `1`. After, the listing drops it, the document reports
`Deleted`, and `consumers` lists nothing and exits `0`. An apply of the Spectro
Cloud provider then holds no cluster in the OpenTofu state.

## What no run covers

Strata makes no claim about the behaviors in this section.

A run shows that Palette lists a digest for a pack in an OCI pack registry, and
for a chart in an OCI Helm registry that Palette syncs. For a Spectro pack
registry, and for a Helm registry over HTTP with no credentials, every record of
every run carries a pack-ref digest, and no check asserts it. No run shows a
pack with no digest.

These are open:

- A Helm registry that Palette does not sync, such as a chart repository over
  HTTP with credentials.
- A Helm registry of type `helm` whose endpoint is `oci://`.
- A pack version pushed again under the same tag in a Spectro pack registry, or
  in a Helm registry that is not an OCI registry. The example owns neither. An
  OCI pack registry and an OCI Helm registry are covered:
  [A pack version pushed again](#a-pack-version-pushed-again).
- The keys that the values of a profile must carry for the preset option that
  they select.
- The order in which a cluster installs the packs of two add-on profiles.
- An install priority other than `0` on an add-on pack.
- A version bump with the `immutable-clusterprofiles` flag on and `skip_destroy`
  off.
- The variables of a cluster template while the template applies.
- A published profile, or a cluster, that pins a pack version when Spectro Cloud
  later disables or deletes it. Palette refuses a new profile that pins one, so
  the example cannot make this case.

## Where these answers come from

The scripts under `examples/live/` build a real tenant, make each change, and
assert each answer above. [The live example](../development/live-example.md)
says how to run them and what each one costs.
[Live runs](../development/live-runs.md) lists the checks and what each one
establishes.
