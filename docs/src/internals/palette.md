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

On a published profile, the `digest` that Palette puts on each pack reference
equals the digest that Palette lists for that pack version. For a Helm pack and
an OCI pack, it also equals the digest that the registry itself serves.

| Pack | Type, registry | Version | Pack-ref digest | What the digest is |
| --- | --- | --- | --- | --- |
| ubuntu-aws | `spectro`, pack registry | 24.04 | `sha256:2df05daa4eb4...` | the digest of the pack registry for the version, as Palette lists it |
| kubernetes | `spectro`, pack registry | 1.36.2 | `sha256:c4f7f20f40c7...` | the same |
| cni-cilium-oss | `spectro`, pack registry | 1.20.1 | `sha256:917d9de9a696...` | the same |
| csi-aws-ebs | `spectro`, pack registry | 1.65.0 | `sha256:bd09e8b4187b...` | the same |
| nginx | `helm`, Helm registry | 25.1.5 | `08816cde6bea...` | the sha256 of the chart archive, as the repository index records it |
| argocd | `oci`, OCI pack registry | 10.4.0.1 | `sha256:99cac22f712b...` | the manifest digest of `<base path>/spectro-packs/archive/argocd:10.4.0.1` |

So for each registry kind, the pack-ref digest is a digest of the content of the
pack version. **Strata reads no registry of its own for a published profile: the
profile document carries what it needs.**

- A Helm digest is bare hex, with no `sha256:` prefix.
- A Helm entry can point at a chart in an OCI registry. The digest is still the
  sha256 of the chart archive, which is the digest of the chart layer, and not
  the manifest digest.
- A chart in an OCI Helm registry that Palette reads itself, the example's own
  in ECR, is a pack of type `oci`, and its digest is the manifest digest of the
  first `helm push` Palette read, as for a pack in an OCI pack registry. It is
  not the sha256 of the chart archive, and a push again does not move it: see
  [A pack version pushed again](#a-pack-version-pushed-again).
- Palette takes the digest from its own copy of the registry, and that copy can
  be late: Palette listed `nginx` up to 25.1.5 while the repository index had
  25.1.12.

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

Palette stores neither the pack schema nor the presets on a profile. The pack
references and the manifests entries of a published profile carry no schema and
no presets. They belong to the pack version in the registry, which lists 20
presets for `cni-cilium-oss` and 6 for `kubernetes`.

So a change to a schema or a preset under one version is a change to the content
of the pack, and the pack-ref digest follows it. A preset that a profile applies
goes into the values of that profile, where strata reads it.

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

A pack pushed to an OCI registry gets the digest of that push, and that is not
the digest a pack registry lists for the same version
([Add packs to an OCI registry](https://docs.spectrocloud.com/registries-and-packs/registries/oci-registry/add-oci-packs/)).
So between two kinds of registry the digest differs too, as the table shows.

So `same-release --allow registry-move` accepts a type change between two
registries, and the pair passes as `REGISTRY MOVE`. It refuses a `manifest`
pack, which comes from no registry. It also keeps its grip on the content: after
an edit of the CSI values in place on top of the move, the check fails and names
`charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`.

## A pack version pushed again

A pack version can be pushed again under the same tag, with other bytes. In an
OCI pack registry of the example's own, after Palette reads the registry again:

| What | After the second push |
| --- | --- |
| The pack version Palette lists | The digest of the new push, under the same version and the same pack uid. |
| A record of the pack from before | `verify` reports `DRIFT`: the digest moved. |
| A published profile that pins the version | Keeps the digest and the pack uid it was pinned with. Its record verifies, minutes after the sync too. |

So a profile pins one build. A registry that changes under the version does not
move the profile. A record of the pack and a record of a profile answer two
different questions, and `verify` on the pack record is what finds such a push:
it reports `UPSTREAM`, and `AUTHORED` too when the values changed.

A new version pushed beside the pinned one, `1.0.1` beside `1.0.0`, does not
move a profile pinned to `1.0.0` either. A profile pinned under a floating tag
is the other case: see [Tag names](#tag-names).

An OCI Helm registry of the example's own answers the same push differently.
The first time Palette reads a chart version, it lists the manifest digest of
the push, and a profile that pins the chart carries it. A push again under the
same version, with other bytes, and a sync after it, change nothing: Palette
keeps the digest it listed first, and a record of the chart from before the
push verifies. The digest stayed through an emptied repository, two syncs, and
two new pushes, for an hour. A sync does read a new version: `1.0.1` pushed
beside `1.0.0` is listed, newest first. So for a chart in an OCI Helm registry,
`verify` on a record of the chart says what Palette lists, and not what the
registry serves now. A pack in an OCI pack registry follows each push.

## Tag names

A registry can give one pack build several tags.
`GET /v1/packs/<name>/registries/<uid>` lists `LTS__24.04`, `LTS__24.4.x`, and
`LTS__24.x` for `ubuntu-aws`, each with the version `24.04` and the pack uid
`6917446d9da9b9852faf5f60`. A profile from the Spectro Cloud provider pins
`24.04`. Palette stores the tag that the author picked, and the version that
the tag named, so two profiles on one build can hold two tags. A record carries
both.

Two versions that differ only in that tag run the same build:

| Question | Answer |
| --- | --- |
| What does Palette store? | The tag as authored, with one pack uid and one version behind all of them. |
| Does Palette repave a cluster that moves between them? (`POST /v1/spectroclusters/<uid>/validate/repave`) | No. The control, one more entry in `kubeadmconfig.postKubeadmCommands`, repaves: `PackValuesUpdated`. |
| What happens to the nodes on such a move, and on the move back? | The same EC2 instances throughout. The cluster stays `Running`, and its selection matches the target version from the first poll. |
| What does Palette record for the move? | One audit, "Profile ... is dissociated from the cluster" and "... is associated to the cluster". No repave: `GET /v1/spectroclusters/<uid>/repave/status` answers 404. |
| Can the tag be renamed on a published version? | Yes, through the API, and the pack uid stays. The renamed version then has the root of the version it was renamed to match. |

The repave check of Palette also answers "no repave" for a request that gives
the OS pack an altogether different tag. **A tag in the request is no control
for that check; a change of the values is.**

A floating tag moves a published profile. In an OCI pack registry of the
example's own, Palette lists `1.0.x` and `1.x` beside the version `1.0.1`. A
profile pinned to the pack under `1.0.x`, and then a push of `1.0.2`, which the
tag now names: some minutes after the push, and not within the sync the
example asks for, the published profile carries `1.0.2`, with its pack uid and
its digest, and the tag `1.0.x` stays. The move took up to four minutes in
the runs. No person acted, and no publish happened. `verify` on a record of the profile
from before reports `DRIFT` with `IDENTITY UPSTREAM`. A profile pinned to the
exact version `1.0.0` does not move when `1.0.1` arrives.

## Edits in place

A person can edit a published profile version through the API
([Update a cluster profile](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/update-cluster-profile/)).
The version keeps its uid and its number, stays published, and no new version
appears. A pack can also be pushed again under one version, with
`spectro pack push --force`
([Spectro CLI reference](https://docs.spectrocloud.com/registries-and-packs/spectro-cli-reference/))
or `oras push`. A published profile that pins the version keeps the digest it
was pinned with: see [A pack version pushed again](#a-pack-version-pushed-again).

This is why [`strata verify`](../commands/verify.md) exists: a record from
before such a change exits `1` with `DRIFT`, and names the changed value.
`DRIFT` says that the published version changed. It does not say who changed
it: a person, a registry whose floating tag moved, or an update that a cluster
accepted.

An edit through the API must keep the uid of each manifest. An edit that sends a
manifest with no uid makes Palette replace that manifest with a new one. When a
cluster then accepts the update, Palette removes the copy of the old manifest
from the cluster, and the add-on is left with no manifest there.

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
resource, and plans one change. The cluster stays `Running`. The audit log gets
two entries with the user's name: an update of the cluster, and the action
"updated the cluster variables on the cluster <name>". The variables endpoint
gives the new value. A record of the copy from before verifies, and a comparison
of the records from before and after reports the copy unchanged and one
variable value changed.

Palette says when a copy is behind its profile.
`GET /v1/spectroclusters/<uid>/profileUpdates` gives the profile set the cluster
would get, and the notifications of the cluster list the update that waits
([Cluster updates](https://docs.spectrocloud.com/clusters/cluster-management/cluster-updates/)).
A record does not carry that yet.

## The order of the profiles of a cluster

`spec.clusterProfileTemplates` of a cluster lists its profiles in the order the
request that made the cluster gave them, and that order carries no meaning: the
Spectro Cloud provider gives them in the order of a set, hashed over the whole
`cluster_profile` block with the profile uid in it, so each apply sends them in
a new order. Two clusters built from one definition, with the cluster profile
first and the add-on profile second, listed the two in the two orders. So strata
orders them itself: a cluster record lists
the cluster or infra profile first and then the others by name, and merges their
packs in that order, so two clusters built from one definition have one merged
root. A check of what a cluster runs compares the set of profiles, and [the
knurl adapter](../automation/knurl.md) gives the copies that a cluster holds by
name.

That order is no install order either. A cluster that ran two add-on profiles,
each with one manifest pack, listed the second one first, and started the two
packs in the same second, three minutes after the last pack of the cluster
profile. The add-on profiles start together.

## The lock of a cluster template

A cluster template links profile versions and a maintenance policy, and applies
them to the clusters attached to it
([Cluster templates](https://docs.spectrocloud.com/cluster-templates/)). The
documentation says that a linked version becomes immutable. A live run shows
what that lock is:

| Request | Answer |
| --- | --- |
| An edit in place of a version that a template links, through the API | Refused: `ClusterProfileUpdateValidationFailure`, "cluster profile is associated with 1 cluster templates and cannot be modified". The version verifies. |
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
| The uid | Keeps its place, and names the new version. | The new version is a clone under a new uid. The old version stays under the old uid. |
| `verify` of the old record | Reports it overwritten by the new version. | Reports the edit in place, and nothing overwritten. |
| The clusters | Keep their copies, which name the old version, and stay consumers of the uid. | Keep their copies, which equal the old record, and stay consumers of the old version only. |

The clone carries the content over exactly: the values, the manifests, and the
variables of a flag-on bump equal those of a flag-off bump from the same
configuration.

A version is `major.minor.patch`. Palette checks it when the version is
created, and the provider does not
([Version a cluster profile](https://docs.spectrocloud.com/profiles/cluster-profiles/modify-cluster-profiles/version-cluster-profile/)).

With the flag on, set `skip_destroy` on each profile **before** the first bump.
A flag-on bump replaces the profile resource, and the provider deletes the
replaced version with the `skip_destroy` value already in its state, not the new
value (provider 0.30.1, `resourceClusterProfileDelete`). Without it, the bump
asks Palette to delete the version that a cluster runs, and Palette refuses with
`DeletionResourceInUseError`.

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
holds for one pack, in one request. For `csi-aws-ebs` in `Public Repo` that is
36 versions in about 1 MB:

| Field | Holds |
| --- | --- |
| `layer`, `cloudTypes` | the layer of the pack, and the clouds it supports |
| `tags[]` | one row per tag: `tag`, `version`, `packUid`, and `parentTags` |
| `packValues[]` | one row per build: `packUid`, `values`, `schema`, `presets`, `readme`, `annotations`, `dependencies` |

Those 36 versions carry 71 tag rows and 36 pack uids: one build for each
version, under several tags. A version carries its exact tag and the floating
tags above it, each as its own row, and `parentTags` names the tags above that
one:

```text
{"packUid":"6aa410ec…","parentTags":null,          "tag":"1.x",    "version":"1.65.0"}
{"packUid":"6aa410ec…","parentTags":["1.x"],       "tag":"1.65.x", "version":"1.65.0"}
{"packUid":"6aa410ec…","parentTags":["1.65.x","1.x"],"tag":"1.65.0","version":"1.65.0"}
```

So the newest version of a line is the version that the floating tag names, and
one request gives every version, every tag, and no repeats. The paged listing
`/v1/packs?filters=spec.name=<name>` holds the digest of each build, which this
answer does not, and it repeats a version across pages.

**A constraint between packs.** `packValues[]` has a `dependencies` field, which
is where a pack that requires another pack would say so. No pack that this key
can see fills it: a read of every distinct pack of the tenant's registries, 116
of them, found an empty `dependencies` on every version.

```text
packs scanned: 116
with a dependency: 0
```

So the shape is there and the content is not. A check that reads a constraint
cannot be written against a field that no pack fills, and
[#58](https://github.com/como-technologies/strata/issues/58) stops for that
reason.

**How far a constraint reaches.** A team that publishes its own packs reports
that Palette checks a required pack against the packs of the same profile only.
A profile whose pack requires a pack that *another* profile carries is refused,
with an error that names the constraint and not the reason. That team split one
profile into three, which is the normal way to stop one pack publish from
bumping the same layers three times, and had to publish new pack versions that
drop the constraint. Their packs state the requirement in the README instead.

No run in this repository confirms that, because no pack here declares a
dependency to test it with. It is written down because the next person to split
a profile will meet it, and because it is the reason to finish #58 when a tenant
with such a pack is at hand.

## What the cluster listing answers

`GET /v1/dashboard/spectroclusters/meta` is the listing that `strata list
clusters` reads. **It names no profile.** Each entry holds:

| Part | Fields |
| --- | --- |
| `metadata` | `name`, `uid`, `creationTimestamp`, `lastModifiedTimestamp`, `deletionTimestamp`, the labels, and the annotations `projectUid`, `scope`, `scopeVisibility`, `ownerUid`, `tenantUid`, and `permissions` |
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
computes when it is asked, in the projects the key can read. It is empty on an
error too. So `consumers` reads each cluster the key can list as well, and names
a cluster that applies the version and is not in the list.

The listing answers a JSON array at the top level, and not an object with
`items`. Strata reads both shapes.

A deleted cluster leaves this listing and keeps its document:
`GET /v1/spectroclusters/<uid>` still answers `200`, with the state `Deleted`.
So `cluster:<name>`, which strata resolves through the listing, does not go
ambiguous when one name is deleted and built again.

A deletion by a path other than the provider looks the same. `DELETE
/v1/spectroclusters/<uid>` is accepted at once. While the cluster goes,
seventeen to twenty minutes for three nodes, the listing keeps it with the state
`Deleting`, and `consumers` lists it with that state and exits `1`. After,
the listing drops it, the document reports `Deleted` with a
`deletionTimestamp`, and `consumers` lists nothing and exits `0`. The Spectro
Cloud provider sees the deletion on its next refresh and drops the cluster from
the OpenTofu state, with nothing to change.

## What no run covers

These are open. Strata makes no claim about them.

- A pack version pushed again under the same tag in a Spectro pack registry,
  or in a Helm registry that is not an OCI registry. The example owns neither.
  An OCI pack registry and an OCI Helm registry are covered:
  [A pack version pushed again](#a-pack-version-pushed-again).
- A Helm registry that Palette does not sync, such as a chart repository over
  HTTP with credentials. For such a registry Palette can list no digest for a
  chart; a record then says `upstreamSource: none`, and its root covers no
  archive.

## Where these answers come from

The scripts under `examples/live/` build a real tenant, make each change, and
assert each answer above. [The live example](../development/live-example.md)
says how to run them and what each one costs.
[Live runs](../development/live-runs.md) lists the checks and what each one
establishes.
