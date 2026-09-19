# What Palette does

The live example ran strata against a real tenant, and recorded what Palette
did. Each section gives the run ids. [Live runs](../development/live-runs.md)
has the check tables. The runs used Terraform 1.16.2, except run
`20260917T183929Z-tofu`, which repeated the read-only checks with OpenTofu
1.12.6 and gave the same answers. The live example uses OpenTofu now, with the
same provider.

Each uid in this book is an example. The runs used the uids of a test tenant,
and this book holds none of them. The pack names, the versions, the digests, and
the run ids are the real ones.

## The pack-ref digest

On a published profile, the `digest` that Palette puts on each pack reference
equals the digest that Palette lists for that pack version. For a Helm pack and
an OCI pack, it also equals the digest that the registry itself serves. Run
`20260916T175442Z`:

| Pack | Type, registry | Version | Pack-ref digest | What the digest is |
| --- | --- | --- | --- | --- |
| ubuntu-aws | `spectro`, pack registry | 24.04 | `sha256:2df05daa4eb4...` | the digest of the pack registry for the version, as Palette lists it |
| kubernetes | `spectro`, pack registry | 1.36.2 | `sha256:c4f7f20f40c7...` | the same |
| cni-cilium-oss | `spectro`, pack registry | 1.20.1 | `sha256:917d9de9a696...` | the same |
| csi-aws-ebs | `spectro`, pack registry | 1.65.0 | `sha256:bd09e8b4187b...` | the same |
| nginx | `helm`, Helm registry | 25.1.5 | `08816cde6bea...` | the sha256 of the chart archive, as the repository index records it |
| argocd | `oci`, OCI pack registry | 10.4.0.1 | `sha256:99cac22f712b...` | the manifest digest of `<base path>/spectro-packs/archive/argocd:10.4.0.1` |

The `spectro` digests are also in `examples/INDEX.md`, fetched from the registry
a day earlier, and in the first run `20260916T153806Z`. So for each registry kind, the pack-ref digest is a digest of the
content of the pack version. Strata needs no registry reads of its own.

- A Helm digest is bare hex, with no `sha256:` prefix.
- The `nginx` 25.1.5 entry in the Bitnami index points at a chart in an OCI
  registry. The digest is still the sha256 of the chart archive: the digest of
  the chart layer, not the manifest digest (`sha256:7504fcfcdf30...`).
- Palette takes the digest from its own copy of the registry, and the copy can
  be late. On 2026-09-16, Palette listed `nginx` up to 25.1.5 from Bitnami,
  while the repository index had 25.1.12.

## Pack schema and presets

Palette does not store the pack schema or the presets on a profile. In the live
run, the pack references and the manifests entries of a published profile had no
schema and no presets. The registry lists 20 presets for `cni-cilium-oss` and 6
for `kubernetes`. They belong to the pack version in the registry. So a change
to them under the same version is a change to the content of the pack, which the
pack-ref digest follows. A preset that a profile applies goes into the values.

## A registry move

`examples/live/registry-move.sh` moved a pack as a team promotes one. Version
1.0.0 took each pack from the pack registry `Public Repo`. A version bump to
1.0.1, with the immutable-clusterprofiles flag on, pinned `csi-aws-ebs` 1.65.0
to the OCI pack registry `Palette Registry`. Run `20260917T150025Z`, provider
0.30.1:

| Field | 1.0.0, pack registry | 1.0.1, OCI registry |
| --- | --- | --- |
| type | `spectro` | `oci` |
| registryUid | `5eecc89d0b150045ae661cef` | `64ec000000000000000000a1` |
| packUid | `6aa410ec144aaec79d547a9e` | `64ec000000000000000000b1` |
| digest | `sha256:bd09e8b4187b...` | `sha256:a746e1468908...` |
| tag, version | `1.65.0` | `1.65.0` |
| values, manifests, params | | unchanged |

The pack type follows the registry: `spectro` for a pack registry, `helm`, or
`oci`. So `--allow registry-move` accepts a type change between two registries.
It refuses a `manifest` pack, which comes from no registry. The pair passes as
`REGISTRY MOVE`. After an edit of the CSI values of 1.0.1 in place, the check
fails and names
`charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`.
The first run of the move, `20260916T233556Z`, found that the policy required an
equal pack type, from a fixture that rewrote only the uids and the digests. It
failed the real pair with `IDENTITY: type spectro -> oci`. After the fix, run
`20260917T003010Z` passed all 15 checks.

## Tag names

A registry can give one pack version several tags. For `ubuntu-aws` 24.04,
`GET /v1/packs/ubuntu-aws/registries/<uid>` lists `LTS__24.04`, `LTS__24.4.x`,
and `LTS__24.x`, each with the version `24.04` and the pack uid
`6917446d9da9b9852faf5f60`. A profile from the provider pins `24.04`. Palette
stores the tag that the author picked.

`examples/live/tag-only.sh` published two versions that differ from 1.0.0 only
in the tag of the OS pack: 1.0.2 under `LTS__24.04`, and 1.0.3 under the
floating tag `LTS__24.x`. It moved cluster A to 1.0.2 and back, and kept each
move `Running` for 15 minutes. Then it renamed the tag of 1.0.2 in place through
the API. Runs `20260917T044135Z` and `20260917T124102Z`, flag off, gave the same
answers:

| Question | Answer |
| --- | --- |
| What did Palette store? | 1.0.0 `24.04`, 1.0.2 `LTS__24.04`, 1.0.3 `LTS__24.x`: the tag as authored, and one pack uid and version for all three. |
| Does Palette repave for a move to 1.0.2 or 1.0.3? (`POST /v1/spectroclusters/<uid>/validate/repave`, which `tenant.sh repave` calls) | No. The control, 1.0.0 with one more entry in `kubeadmconfig.postKubeadmCommands`, repaves: `PackValuesUpdated`. |
| Cluster A moved to 1.0.2, and back | The same four EC2 instances before, between, and after. `Running` at each poll, and its selection matched the target version from the first poll. |
| What did Palette record for each move? | One audit, "Profile ... is dissociated from the cluster" and "... is associated to the cluster", and no repave: `GET /v1/spectroclusters/<uid>/repave/status` answers 404. |
| The rename of the tag of 1.0.2 from `LTS__24.04` to `24.04` in place | Palette accepted it and kept the pack uid. 1.0.2 then had the root of 1.0.0. |

The repave check of Palette also answered "no repave" for a request that gave
the OS pack the tag `22.04`. So a tag in the request is no control for that
check. A change of the values is.

Not covered: a new build published under a floating tag that a profile already
pins. It is not known if Palette then moves the pack uid of the profile. If it
does, the pack uid or version changes, and strata reports an `IDENTITY` change.

## Edits in place

Palette accepts an edit of a published profile version through the API. The
version keeps its uid and its number, stays published, and no new version
appears. `strata verify` on a record from before the edit exits `1` with `DRIFT`,
and names the changed value (run `20260916T153806Z`).

An edit through the API must keep the uid of each manifest. In run
`20260916T233714Z`, the edit sent the manifest with no uid. Palette replaced the
manifest with a new one. When cluster A then accepted the update, Palette removed
the copy of the old manifest on A, and the add-on had no manifest on the cluster.
`api.sh edit` keeps the uids, and `in-place.sh` checks it.

## Cluster copies

`examples/live/in-place.sh` edits and then bumps the add-on profile version that
two clusters run. It runs once with the immutable-clusterprofiles flag off, and
once with it on. Nothing else differs between the two runs. Session
`20260917T151426Z`, provider 0.30.1, cluster A `fred` and cluster B `barney`.
Session `20260917T031126Z` gave the same answers with provider 0.29.7 and its
workaround, except `plan-no-change`, which that provider refused with the flag
on. The first two sessions, `20260916T213303Z` and `20260917T003054Z`, gave the
same answers again. Cluster A
ran `strata-live-addon@1.0.0` before the edit. Cluster B was created from the
edited 1.0.0.

Each cluster runs its own copy of the manifests of a profile. The copy has its
own uid, and `GET /v1/spectroclusters/<uid>/profiles/packs/manifests` serves it.
`cluster:<uid>` records that copy. So a cluster record shows what the cluster
got, not what its profile holds now.

| Step | Flag off | Flag on |
| --- | --- | --- |
| Edit 1.0.0 in place | The API accepts it. The provider plans the same edit as an update in place. | The API accepts it. The provider refuses the same edit at plan time. |
| Cluster A, which ran before the edit | Keeps its old copy. `verify` passes on its record, and the record of the version compares `IDENTICAL` with it. | The same |
| The record of the version from before the edit | `verify` reports `DRIFT`. | The same |
| Cluster B, created after the edit | Runs the edited content. `compare cluster:A/<profile> cluster:B/<profile>` shows the change, and notes that both sides are the same version. | The same |
| The update pushed to cluster A, with its notification open | The copy of cluster A takes the edit within three minutes. | The same |
| The update accepted on cluster A | Cluster A matches cluster B. The record of A from before the edit reports `DRIFT`. | The same |

So `compare` of two cluster records shows a change exactly when the clusters
run different content. Strata needs no record of the updates that wait for a
cluster.

## Version bumps

The same runs then bumped the add-on version to 1.0.1:

| Result | Flag off | Flag on |
| --- | --- | --- |
| The uid | Keeps its place, and names 1.0.1. | 1.0.1 is a clone under a new uid. 1.0.0 stays under the old uid. |
| `verify` of the 1.0.0 record | Reports it overwritten by `strata-live-addon@1.0.1`. | Reports the edit in place, and nothing overwritten. |
| The clusters | Keep their copies, which name 1.0.0, and stay consumers of the uid. | Keep their copies, which equal the 1.0.0 record, and stay consumers of 1.0.0 only. |

The clone carries the content over exactly. The values, the manifest, and the
variables of the flag-on 1.0.1 equal those of the flag-off 1.0.1 from the same
configuration.

With the flag on, set `skip_destroy` on each profile before the first bump. A
flag-on bump replaces the profile resource, and the provider deletes the
replaced version with the `skip_destroy` value in its state, not the new value
(provider 0.30.1, `resourceClusterProfileDelete`). Without it, the bump asks
Palette to delete the version that a cluster runs. Palette refuses:
`DeletionResourceInUseError` (run `20260917T132642Z`).

## Manifest pack uids

The flag-on clone and the flag-off 1.0.1 differ in one field: the `packUid` of
the manifest pack. Palette gives the manifest pack of each profile that it
creates a new uid, the clone too. A registry pack keeps the uid of the registry.
So the root of the add-on profile differed between two builds of the live
example from the same configuration (`a18670a7b8d6`, `7d1079372122`). The root
of the release profile did not (`eada36c2e3fd`).

## Drift reports

`examples/live/drift-report.sh` ran `scripts/ci/verify-report.sh` around an edit in
place and a version bump, with the flag off and on. Session `20260917T150538Z`,
provider 0.30.1. In both runs, the edit opened one issue for the add-on record,
a second report updated it, and the undo closed it. Without the flag, the bump
opened one issue for the add-on record, which says the version was overwritten
in place, and one for the record of cluster A. With the flag, nothing opened.
GitHub lists a new or closed issue about five seconds later, so the checks wait
for the listing.

## What a registry answers about a pack

`GET /v1/packs/<name>/registries/<uid>` answers with everything the registry
holds for one pack, in one request. Run `20260918T020941Z` read `csi-aws-ebs`
from `Public Repo`, which gave 36 versions in an answer of about 1 MB:

| Field | Holds |
| --- | --- |
| `layer`, `cloudTypes` | the layer of the pack, and the clouds it supports |
| `tags[]` | one row per tag: `tag`, `version`, `packUid`, and `parentTags` |
| `packValues[]` | one row per build: `packUid`, `values`, `schema`, `presets`, `readme`, `annotations`, `dependencies` |

The 36 versions carried 71 tag rows and 36 pack uids: one build per version,
under several tags. A version carries its exact tag and the floating tags above
it, each as its own row, and `parentTags` names the tags above that one:

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
is where a pack that requires another pack would say so. Not one pack that this
key can see fills it. The run read every distinct pack of the tenant's
registries, 116 of them, and each answered with an empty `dependencies` on every
version:

```text
packs scanned: 116
with a dependency: 0
```

So the shape is there and the content is not. A check that reads a constraint
cannot be written against an answer that no pack fills, and
[#58](https://github.com/como-technologies/strata/issues/58) stops here for that
reason.

**How far a constraint reaches.** A team that publishes its own packs reported
the behavior that makes this matter: Palette checks a required pack against the
packs of the same profile only. A profile whose pack requires a pack that
*another* profile carries is refused, with an error that names the constraint
and not the reason. That team split one profile into three, which is the normal
way to stop one pack publish from bumping the same layers three times, and had
to publish new pack versions that drop the constraint. Their packs state the
requirement in the README instead.

No run in this repository confirms that, because no pack here declares a
dependency to test it with. It is written down because the next person to split
a profile will meet it, and because it is the reason to finish #58 when a tenant
with such a pack is at hand.

## What the cluster listing answers

`GET /v1/dashboard/spectroclusters/meta` is the listing that `strata list
clusters` reads. Run `20260918T175802Z` recorded its answer for the cluster
`fred`, which applies two profiles. The answer names no profile. Each entry
holds:

| Part | Fields |
| --- | --- |
| `metadata` | `name`, `uid`, `creationTimestamp`, `lastModifiedTimestamp`, `deletionTimestamp`, the labels, and the annotations `projectUid`, `scope`, `scopeVisibility`, `ownerUid`, `tenantUid`, and `permissions` |
| `spec` | `cloudType`, `cloudAccountUid`, `cloudRegion`, `clusterType`, `archType`, `location`, and `tags` |
| `status` | `state` |

So the profiles a cluster applies need the cluster document itself, `GET
/v1/spectroclusters/{uid}`, and its `spec.clusterProfileTemplates`. That is one
request for each cluster, which is why `strata list clusters --applied` asks for
them and a plain `strata list` does not.

The listing answers a JSON array at the top level, not an object with `items`.
Strata reads both shapes.

A deleted cluster leaves this listing, and keeps its record: the listing holds
the clusters that are there, and `GET /v1/spectroclusters/<uid>` still answers
`200` with the state `Deleted`. So `cluster:<name>`, which
`Palette::cluster_uid_named` resolves through the listing, does not go ambiguous
when one name is deleted and built again.

## What no run covers

- A pack version pushed again under the same tag, for any registry kind. It is
  not known if the pack-ref digest of a published profile follows such a push.
- A Helm registry that needs credentials, and an OCI registry that holds Helm
  charts in place of packs. The tenant of the runs had neither.
- A new build under a floating tag that a profile pins.
- Whether a required pack constraint reaches beyond the packs of one profile.
  No pack in the visible registries declares a dependency, so no run here can
  test it; a team reported that it does not, above.
- Deletion. No run records it. That a deleted cluster leaves the cluster listing
  comes from one teardown through the provider, read by hand, and another path
  to a deletion may answer differently.
