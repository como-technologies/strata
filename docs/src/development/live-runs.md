# What the live example verifies

[The live example](./live-example.md) builds a real tenant and asserts what
Palette does with it. This page lists what each script sets up and what each of
its checks establishes, so you can see the coverage without running anything.

`verify.sh` is the read-only script over the cluster, and
[its checks are on the live example page](./live-example.md#the-checks-of-verifysh).
`packs.sh` and `knurl.sh` read too. The other scripts below write to the tenant;
`pack-states.sh` keeps nothing that it writes.

Each check names its expected exit code. A run that gets another code fails, and
prints both. The issue numbers name what the check was written for.

## in-place.sh

Edits a published add-on version in place through the API, pushes the update to
a cluster, and then bumps the version. It runs twice, once with the provider's
`immutable-clusterprofiles` flag off and once with it on, and nothing else
differs between the two runs. Cluster A exists before the edit; cluster B is
created from the edited version, and deleted at the end.

This is the source for
[Cluster copies](../internals/palette.md#cluster-copies) and
[Version bumps](../internals/palette.md#version-bumps).

| Check | Flag off | Flag on | What it shows |
|---|---|---|---|
| `a1`, `a1-addon` | 0 | 0 | record cluster A and its add-on selection |
| `release1`, `addon1` | 0 | 0 | record the release version and the add-on version |
| `addon1-is-1.0.0` | 0 | 0 | the add-on version is the one the run expects |
| `a1-updates` | 0 | 0 | what waits for cluster A |
| `a-owns-manifest-copy` | 0 | 0 | cluster A runs its own copy of the add-on manifest, not the profile's |
| `plan-no-change` | 0 | 0 | the provider plans no change once `skip_destroy` is held for the flag |
| `plan-edit` | 2 | 1 | #27: the provider's plan for an edit in place: an update without the flag, a refusal with it |
| `plan-edit-says` | 0 | 0 | the plan says which |
| `api-edit` | 0 | 0 | #27: the API accepts an edit in place, whatever the flag |
| `a2`, `a2-addon`, `b2-addon`, `a2-updates` | 0 | 0 | record each cluster's add-on selection after the edit, and what waits for cluster A |
| `addon2` | 0 | 0 | record the edited add-on version |
| `edit-kept-manifest-uid` | 0 | 0 | the edit kept the manifest's uid, so cluster copies still name it |
| `b-owns-manifest-copy` | 0 | 0 | cluster B was created with its own copy of the manifest |
| `a-kept-old-content` | 0 | 0 | #27: cluster A keeps its copy from before the edit |
| `a-vs-b` | 1 | 1 | #27: cluster A differs from cluster B, created from the edited version |
| `a-vs-b-edited-in-place` | 0 | 0 | #27: the comparison says the version was edited in place |
| `verify-a1` | 0 | 0 | #27: cluster A's record from before the edit still verifies |
| `verify-release1` | 0 | 0 | #23: the release record is untouched by the add-on edit |
| `verify-addon1` | 1 | 1 | #23: the add-on record reports `DRIFT` after its edit in place |
| `addon1-vs-a2` | 0 | 0 | #27: the record from before the edit matches cluster A's selection |
| `consumers-addon` | 1 | 1 | both clusters use the add-on version |
| `apply-keeping-notification` | 0 | 0 | push the content to cluster A with its notification kept |
| `a-changed-without-resolve` | 1 | 1 | #27: three minutes after that push, cluster A's copy has taken the edit |
| `apply-to-a` | 0 | 0 | #27: accept the waiting update, resolving the notification |
| `a3`, `a3-updates` | 0 | 0 | record cluster A after the apply, and what waits for it |
| `a3-matches-b` | 0 | 0 | #27: cluster A now runs what cluster B runs |
| `verify-a1-after-apply` | 1 | 1 | #27: the record from before the edit reports `DRIFT` once the update is applied |
| `addon-uid-kept` | 0 | 1 | #27: the bump keeps the uid without the flag, and makes a new one with it |
| `addon-old4`, `addon-new4` | 0 | 0 | record what the first uid holds now, and the bumped version |
| `a4-addon`, `b4-addon` | 0 | 0 | record each cluster's add-on selection after the bump |
| `verify-addon1-after-bump` | 1 | 1 | #27: the record from before the edit still reports `DRIFT` |
| `addon1-overwritten` | 0 | 1 | #27: `verify` says the recorded version was overwritten, only without the flag |
| `a4-matches-old-version` | 1 | 0 | #23, #27: the selection equals the first uid only with the flag, which keeps the old version |
| `a4-vs-old-json` | 1 | 0 | the same comparison as JSON |
| `a4-names-overwrite` | 0 | 1 | #29: the comparison names the first uid as overwritten, only without the flag |
| `a4-still-a3` | 0 | 0 | #27: the bump left cluster A as it was |
| `verify-a3-after-bump` | 1 | 0 | #30: `verify` of cluster A's record fails only without the flag: the version it runs was overwritten |
| `a3-names-overwritten-profile` | 0 | 1 | #30: the verify names the add-on profile, only without the flag |
| `consumers-old` | 1 | 1 | #27: both clusters are still consumers of the first uid |
| `consumers-old-overwritten` | 0 | 1 | #28: `consumers` marks both clusters as running a version the uid no longer names, only without the flag |
| `clone-carried-nothing` | - | 0 | #27, #32: the flag's clone is the same release as the legacy bump of the same configuration, whatever pack uid Palette minted |
| `clone-same-content` | - | 0 | #27: the clone has the values, manifests, and variables of the legacy bump |
| `b-gone` | 0 | 0 | cluster B is deleted |
| `a5-matches-addon` | 0 | 0 | cluster A runs the restored add-on version |
| `baseline-after` | 0 | 0 | the live example is back at its baseline |

## registry-move.sh

Publishes a version that takes every pack from the pack registry, then bumps it
with the CSI pack re-pinned to an OCI registry. It runs with the flag on, so the
bump makes a new version and keeps the old one. The source for
[A pack that moves between registries](../internals/palette.md#a-pack-that-moves-between-registries).

| Check | Expected exit | What it shows |
|---|---|---|
| `plan-no-change` | 0 | the provider plans no change once `skip_destroy` is held for the flag |
| `move1` | 0 | record the first version, every pack from the pack registry |
| `move-new-uid` | 1 | the flag made the bump a new profile version |
| `move1-kept` | 0 | the first version is still there, unchanged |
| `move2` | 0 | record the bumped version |
| `move2-csi-is-oci` | 0 | its CSI pack comes from the OCI registry |
| `compare-move` | 1 | every field that differs between the two records |
| `same-release-exact` | 1 | an exact check fails: the registry moved |
| `same-release-registry-move` | 0 | #24: the registry-move policy passes a real move, which changes the pack type |
| `api-edit-move2` | 0 | edit the moved version's CSI values through the API |
| `move2-edited` | 0 | record the edited version |
| `same-release-after-edit` | 1 | #24: the policy fails the pair after an edit in place |
| `edit-is-authored` | 0 | the reason names the `AUTHORED` change |
| `delete-move2`, `delete-move1` | 0 | delete both versions |
| `baseline-after` | 0 | the live example is back at its baseline |

## tag-only.sh

Publishes two versions that differ from the first only in the tag of the OS
pack, one an exact tag and one a floating tag. It moves a cluster to one of them
and back, and holds each state until a repave has time to show. It then renames
a tag on a published version through the API. The source for
[Tag names](../internals/palette.md#tag-names).

| Check | Expected exit | What it shows |
|---|---|---|
| `release1`, `exact1`, `floating1` | 0 | record the three versions |
| `a1`, `instances1` | 0 | record the cluster's selection and its EC2 instances before any move |
| `os-packs` | 0 | the tag, version, and pack uid that Palette stored for each version |
| `one-build` | 0 | Palette stored the authored tag, with one pack uid and version behind all three |
| `compare-exact` | 1 | the exact tag: the roots differ |
| `compare-exact-says` | 0 | #19: the pair is `SAME CONTENT`, names the rename, and gives no node impact |
| `compare-exact-json`, `compare-exact-json-says` | 1, 0 | #19: the same as JSON: `sameContent`, the OS pack `tagRenamed`, max impact none |
| `compare-floating`, `compare-floating-says` | 1, 0 | #19: the floating tag is the same content too |
| `same-release-tags` | 0 | #19: the three versions are one release |
| `palette-repave-exact`, `palette-repave-floating` | 0 | Palette repaves nothing for a move between them |
| `palette-repave-control` | 1 | the control, one more kubeadm command, does repave |
| `a2`, `a2-runs-exact`, `instances2` | 0 | after the move, the selection is `IDENTICAL` to the target |
| `repaved-to-exact` | 1 | #19: the move kept the cluster's instances |
| `palette-repave-back` | 0 | Palette repaves nothing for the move back |
| `a3`, `a3-runs-release`, `instances3` | 0 | after the move back, the selection is `IDENTICAL` to the first version |
| `repaved-back` | 1 | #19: the move back kept the instances |
| `retag` | 0 | #19: rename a published version's OS pack tag through the API, keeping its pack uid |
| `exact2`, `retag-kept-pack-uid` | 0 | record the renamed version; the rename kept the pack uid |
| `verify-exact1` | 1 | #19: `verify` of the record from before the rename reports `DRIFT` |
| `verify-exact1-says` | 0 | #19: the drift names the rename, the same content, and no node impact |
| `baseline-after` | 0 | the live example is back at its baseline |

## drift-report.sh

Runs `scripts/ci/verify-report.sh` around an edit in place and a version bump,
with the flag off and on, against real GitHub issues under a label. It starts
from a baseline with no open issue and restores it, closing each issue it
opened. GitHub lists a new or closed issue about five seconds later, so the
checks wait for the listing.

| Check | Flag off | Flag on | What it shows |
|---|---|---|---|
| `record-release`, `record-addon`, `record-cluster-a` | 0 | 0 | record the two versions and the cluster |
| `report-baseline`, `report-baseline-says` | 0 | 0 | #25: at the baseline every record verifies, and no issue opens |
| `plan-edit`, `plan-edit-says` | 2, 0 | 1, 0 | the provider's plan for the edit: an update without the flag, a refusal with it |
| `api-edit` | 0 | 0 | the API accepts the edit whatever the flag |
| `report-edit` | 1 | 1 | #25: the report after the edit exits 1 |
| `report-edit-opened` | 0 | 0 | #25: it opens one issue, for the add-on record; the cluster's copy is untouched |
| `issues-after-edit`, `issue-has-report` | 0 | 0 | #25: one open issue, holding the report of the drift |
| `report-edit-again`, `report-edit-again-updated` | 1, 0 | 1, 0 | #25: a second report updates that issue and opens none |
| `issues-after-edit-again` | 0 | 0 | #25: still exactly one open issue |
| `api-undo`, `report-undo`, `report-undo-closed` | 0 | 0 | #25: the undo makes the report pass again, and closes the issue |
| `issue-edit-closed`, `issues-after-undo` | 0 | 0 | #25: the issue is closed on the host, and none is open |
| `report-bump` | 1 | 0 | #25, #27: the report after the bump: drift only without the flag |
| `report-bump-opened` | 0 | 0 | #25, #27: without the flag it opens an issue for the add-on record and one for the cluster's; with it, none |
| `issues-after-bump` | 0 | 0 | #27, #30: the open issues match that |
| `bump-issue-names-overwrite` | 0 | - | #27: the add-on record's issue says the version was overwritten in place |
| `report-restored`, `report-restored-closed` | 0 | 0 | #25: the restore makes the report pass, and closes each issue the bump opened |
| `issues-after-restore`, `baseline-after` | 0 | 0 | no issue is open, and the example is back at its baseline |

The same report runs through the reusable workflow
`.github/workflows/verify.yml`, driven by
[act](https://github.com/nektos/act) against a scratch caller repository. The
job checks out the caller and this repository at the release tag, downloads that
release's binary, checks it against `SHA256SUMS`, and runs the script. At the
baseline it succeeds and opens nothing; after an edit in place it opens an issue
with the report and fails with exit 1; after the undo it closes the issue and
succeeds.

## merge-gate.sh

Builds a local repository with a bare origin, so the fetch and the merge are
real git and every record and verify is live. The default branch pins one
version and then a newer one, and a branch pins the older one again. The source
for [The merge gate](../automation/merge-gate.md).

| Check | Expected exit | What it shows |
|---|---|---|
| `record-newer`, `record-older`, `record-addon` | 0 | #57: record the version each side pins |
| `behind` | 1 | #57: the gate is red: the merge result pins the earlier version |
| `behind-says` | 0 | #57: the report names the record and both versions |
| `behind-evidence` | 0 | #57: the evidence says the default branch pinned this release before |
| `behind-declares` | 0 | #57: the report gives the line that declares a rollback |
| `declared`, `declared-says` | 0 | #57: the gate passes once the commit declares the rollback, and names it one |
| `api-edit` | 0 | #57: edit a pinned version in place through the API |
| `drifted` | 1 | #57: the gate is red on a record that Palette no longer matches |
| `drifted-says` | 0 | #57: the report names the record that does not verify |
| `drifted-keeps-rollback` | 0 | #57: the declared rollback still passes; only the record fails |
| `api-undo`, `restored` | 0 | #57: the gate passes again once Palette matches the record |

## packs.sh

Read-only, with no cluster and no cost. It reads pack versions from the
registries the tenant can see, including one pack that two registries hold.

| Check | Expected exit | What it shows |
|---|---|---|
| `list-packs` | 0 | #55: the profiles list the pack versions they pin |
| `compare-versions`, `compare-json` | 1 | #55: two versions of one pack differ, and the document names the pack on both sides |
| `compare-same` | 0 | #55: a pack version against itself is identical |
| `record`, `record-title` | 0 | #55: `snapshot` writes a record of a pack version, which names the pack and not a profile |
| `verify-record` | 0 | #55: the record verifies against the registry it came from |
| `latest`, `latest-is-newest` | 0 | #55: `@latest` resolves to the newest version the registry offers |
| `two-registries`, `two-registries-says` | 2, 0 | #55: a bare ref for a pack in two registries is an error that names both, and the ref that fixes it |
| `other-registry`, `registries-differ` | 0, 1 | #55: the version reads from either registry, and the two hold different builds |
| `unknown-version`, `unknown-registry` | 2 | #55: a version or a registry the tenant does not have is an error |
| `packs-one`, `packs-newest` | 0 | #54: `packs` lists a registry's versions newest first, and the newest carries the floating tag |
| `packs-latest`, `packs-two-registries` | 0 | #54: `--latest` gives one line for each registry that has the pack |
| `packs-paged`, `packs-paged-count` | 0 | #54: a pack with more than 100 versions lists completely, past the 100 of one page |
| `packs-cloud` | 0 | #54: `--cloud` keeps the versions of one cloud |
| `packs-cloud-none`, `packs-cloud-none-says` | 0 | #54: a cloud no version supports is an empty answer, not an error, and it says so |
| `packs-json` | 0 | #54: `--json` writes `strata-packs/1` |
| `packs-unknown-registry`, `packs-unknown-pack` | 2 | #54: an unknown registry lists the visible ones; a pack no registry has is an error |

Palette lists the registries of a tenant in no fixed order, so strata sorts them
and the ambiguous-ref message reads the same every time:

```text
strata: pack 'csi-aws-ebs' at version '1.63.1' is in several registries: Palette Registry, Public Repo; name one, as in pack:<registry>/csi-aws-ebs@1.63.1
```

## pack-states.sh

No cluster, and no cost. It uses three packs that Spectro Cloud retired in the
public registries, one for each state: `nginx` 1.15.1 (disabled) and
`generic-vm-libvirt` 1.0.0 (deleted) in `Public Repo`, and `argo-cd` 3.26.7
(deprecated) in `Palette Community Registry`. None of them requires another
pack. It asks Palette for a profile that pins each one, and deletes the profile
that Palette creates at once.

| Check | Expected exit | What it shows |
|---|---|---|
| `states-disabled`, `states-disabled-says` | 0 | #68: `packs` lists a disabled registry from the listing, and the heading says `disabled, no tags` |
| `states-deleted`, `states-deleted-says` | 0 | #68: the same for a deleted registry: `deleted, no tags` |
| `states-deprecated`, `states-deprecated-says` | 0 | #68: a deprecated version keeps its tags, and the heading and the row say `deprecated` |
| `snapshot-disabled`, `snapshot-disabled-says` | 2, 0 | #68: a disabled version is `pack-unavailable`, and the error names the state and the registry |
| `snapshot-deleted` | 2 | #68: a deleted version is `pack-unavailable` |
| `snapshot-deprecated` | 0 | #68: a deprecated version records as usual |
| `profile-disabled`, `profile-disabled-says` | 1, 0 | #68: Palette refuses a profile that pins a disabled version, with `ClusterProfileInvalidPackState` |
| `profile-deleted`, `profile-deleted-says` | 1, 0 | #68: the same for a deleted version |
| `profile-deprecated` | 0 | #68: Palette creates a profile that pins a deprecated version |

## knurl.sh

Read-only. It runs each method of [the knurl adapter](../automation/knurl.md)
against the tenant. It then runs the strata command for the same object, and
compares the two answers. It needs the profiles. The instance checks need the
cluster, and are `SEEN` without one.

| Check | Expected exit | What it shows |
|---|---|---|
| `unit`, `unit-record` | 0 | `observe_unit` reads the release version, and strata records it |
| `unit-exact-is-root`, `unit-content-is-digest` | 0 | the exact digest of knurl is the root, and its content digest is `contentDigest` |
| `unit-pins-are-packs` | 0 | each pin is a pack of the record from a registry, with its version and its digest, in the order of the record |
| `verify`, `verify-strata` | 0 | `verify` of knurl says `Unchanged`, as `strata verify` says of the record |
| `identity`, `identity-says-same` | 0 | the definition of the version, resolved against the real registries, gives its content digest |
| `pinned-by`, `consumers`, `pinned-by-is-consumers` | 0, ?, 0 | the instances of `pinned_by` are the consumers of strata, by name |
| `instance-<name>`, `instance-<name>-running` | 0 | `observe_instance` reads each cluster that runs the version, and its state is `Running` |
| `instance-<name>-health` | 0 | its health is `Healthy`, read from the cluster summary |
| `instance-<name>-copies` | 0 | each copy the cluster holds has the two digests of its cluster record, `cluster:<uid>/<profile>` |
| `leaf`, `leaf-record` | 0 | a pack in one registry reads as a leaf, and strata records it from that registry |
| `leaf-exact-is-root`, `leaf-content-is-archive`, `leaf-no-pins` | 0 | its exact digest is the root, its content digest is the digest of the pack in its registry, and it pins nothing |
| `two-archives`, `two-archives-says` | 2, 0 | #55: a pack that two registries hold as two archives, with no digest in the address, is a terminal failure that names each registry and its digest |
| `two-archives-by-digest`, `two-archives-by-digest-content` | 0 | the address with the digest of one archive chooses it, and the content of knurl is that archive |
| `not-found`, `not-found-terminal` | 2, 0 | a version the tenant does not have is a terminal failure |

The one-registry leaf is the OCI pack of the registries profile. The release
packs are of no use for it: this tenant has each of them in Public Repo and in
Palette Registry, as two archives, so each is the two-archive case.

## registry.sh

Read-write, in the example's own registries alone: the ECR repositories and
the Palette registries that `terraform/` creates, which no other user reads.
It needs the profiles and no cluster. The script pushes the packs under
`examples/live/pack/` with `oras`, and the chart under `examples/live/chart/`
with `helm push`, as
[Add packs to an OCI registry](https://docs.spectrocloud.com/registries-and-packs/registries/oci-registry/add-oci-packs/)
shows. It then asks Palette to read the registry again after each push.

| Check | Expected exit | What it shows |
|---|---|---|
| `empty`, `sync-0` | 0 | the repository starts empty: the images a run before left are removed, and Palette reads the registry again |
| `push-1`, `sync-1` | 0 | `strata-live-pack` 1.0.0 is pushed, and Palette reads the registry |
| `packs`, `packs-version` | 0 | #66: `packs` lists the one version |
| `leaf`, `leaf-digest-is-push` | 0 | #66: a record of the pack carries the digest of the push |
| `profile`, `profile-digest-is-push` | 0 | #66: a profile that pins the version carries that digest in its pack reference |
| `push-2`, `push-2-differs`, `sync-2` | 0 | the same version is pushed again with other bytes, and the two pushes have two digests |
| `leaf-after`, `listing-follows` | 0 | #66: Palette lists the digest of the new push, under the same version |
| `leaf-verify` | 1 | #66: the record of the pack from before the push reports `DRIFT` |
| `profile-verify`, `profile-keeps-pin` | 0 | #66: the published profile that pins the version does not follow the push: its record verifies, and its pack reference keeps the digest of the first push |
| `profile-after`, `push-3`, `sync-3` | 0 | 1.0.1 is pushed |
| `packs-two`, `packs-two-order` | 0 | #66: `packs` lists 1.0.1 first, then 1.0.0 |
| `profile-stays` | 0 | #66: the profile pinned to 1.0.0 does not move to 1.0.1 |
| `profile-floating`, `push-4`, `sync-4` | 0 | a profile pins the pack under `1.0.x`, and 1.0.2 is pushed, which the tag now names |
| `floating-moves` | 1 | #66: the profile pinned under the floating tag moves with it. The script verifies its record again until it reports `DRIFT`, for up to ten minutes |
| `floating-who`, `floating-who-system`, `floating-who-versions` | 1, 0, 0 | #69: `verify --who` on the same record reads the audit log of the profile: updates by the Palette service `system`, one of which names the two versions, `1.0.1 > 1.0.2` |
| `chart-empty` | 0 | the chart repository starts empty, and two applies remove the Palette registry over it and make it again, so Palette reads the chart for the first time |
| `chart-push-1`, `chart-sync-1`, `chart-packs`, `chart-leaf` | 0 | `strata-live-chart` 1.0.0 is pushed with `helm push` into the example's OCI Helm registry, Palette reads it, and `packs` and `snapshot` read the chart as a pack of type `oci` |
| `chart-digest` | 0 | #66: Palette lists the manifest digest of the push for a chart in an OCI registry, as for a pack, not the digest of the chart archive |
| `chart-profile`, `chart-profile-digest` | 0 | #66: a profile that pins the chart carries that manifest digest in its pack reference |
| `chart-push-2`, `chart-push-2-differs`, `chart-sync-2`, `chart-leaf-after` | 0 | the same chart version is pushed again with other bytes, and Palette reads the registry again |
| `chart-listing-keeps`, `chart-listing-not-new` | 0 | #66: Palette keeps the digest it listed first for the version: the listing does not follow the push, unlike the pack registry |
| `chart-leaf-verify`, `chart-profile-verify` | 0 | #66: the record of the chart from before the push verifies, and so does the profile that pins it: Palette lists what it listed |
| `chart-push-3`, `chart-sync-3`, `chart-packs-two`, `chart-packs-two-versions` | 0 | #66: 1.0.1 is pushed, and the sync reads it: `packs` lists 1.0.1 first, then 1.0.0. A sync reads new versions, and keeps the digest of a known one |
| `dep-push`, `dep-push-requiring`, `dep-sync`, `dep-packs` | 0 | `strata-live-dep` 1.0.0 is pushed, then `strata-live-pack` 1.1.0, whose `pack.json` requires `strata-live-dep` 1.0.0 or newer; Palette reads both |
| `dep-profile-alone`, `dep-profile-alone-says` | 1, 0 | #66: a profile with the requiring pack and not the required one is refused at creation, and the refusal names the required pack |
| `dep-profile-both` | 0 | #66: a profile with both packs is accepted |
| `dep-record`, `dep-record-packs` | 0 | a record of that profile lists the two packs, the requiring one first, as authored |

The script ends with an apply of the baseline, which removes the profiles it
made. The pushed versions stay in the registry until `live-down`.

## template.sh

Read-write. It links the release version and the add-on version to a cluster
template before any cluster runs them. It then tries to change a linked version,
and tries to build a cluster outside the template. Last, it builds the cluster
without the template, with a second add-on profile. It needs the profiles and no
cluster. It ends with the removal of the whole example, because step 4 edits a
version that the cluster runs.

| Check | Expected exit | What it shows |
|---|---|---|
| `release-before` | 0 | a record of the release version before the link |
| `template`, `template-links` | 0 | #66: the template links the release version and the add-on version |
| `release-doc`, `release-names-template` | 0 | #66: the version names the template that uses it, and no cluster |
| `lock-edit`, `lock-edit-says` | 2, 0 | #66: an edit in place of a linked version is refused, and the refusal carries the code `ClusterProfileUpdateValidationFailure` |
| `lock-verify` | 0 | #66: the version is unchanged |
| `cluster-outside-template`, `refusal-says` | 1, 0 | #66: a cluster outside the template with a linked version is refused, `ClusterCreateWithTemplateProfileForbidden` |
| `cluster-runs-release`, `cluster-doc`, `cluster-three-profiles` | 0 | with the template gone, the cluster runs the release version and two add-on profiles |
| `lock-gone`, `lock-gone-verify` | 0, 1 | #66: the edit in place is accepted again, and the record from before reports `DRIFT`: the lock was the template's |
| `addon-order` | 0 | #66: the packs of the two add-on profiles start within thirty seconds of each other, whatever order the cluster lists the profiles in: the listed order is no install order. No run has passed this check yet. |

## cluster.sh

Read-write. It needs the example at its baseline with cluster A, changes the
value the cluster gives the variable of the add-on profile, then deletes the
cluster through the API and not the provider, and brings the OpenTofu state in
line. It ends with the cluster gone and the profiles in place.

| Check | Expected exit | What it shows |
|---|---|---|
| `variables-before`, `copy-before`, `audits-before` | 0 | the cluster variables endpoint, a record of the cluster's copy of the add-on profile, and the audit entries of the cluster, before |
| `variables-after`, `value-changed` | 0 | #66: after an apply that gives the variable the value `changed`, the endpoint gives that value |
| `copy-after`, `copy-verify`, `copy-compare`, `copy-source-value` | 0 | #66: the record of the copy from before verifies, the two records compare `IDENTICAL`, and the record from after carries the value under `source.cluster.variables`, outside the root |
| `audits-after`, `audit-records-change` | 0 | #66: the audit log has a new entry for the cluster |
| `audit-says` | 0 | #66: that entry names who updated the cluster variables on the cluster |
| `listing-before`, `listing-has-cluster`, `consumers-before`, `consumers-has-cluster` | 0, 0, 1, 0 | the listing holds the cluster, and `consumers` lists it under the add-on version, with exit 1: the version has a consumer |
| `delete` | 0 | #66: `DELETE /v1/spectroclusters/<uid>` |
| `listing-deleting`, `consumers-deleting`, `consumers-shows-deleting` | 0, 1, 0 | #66: while the cluster goes, the listing keeps it with the state `Deleting`, and `consumers` lists it with that state |
| `doc-after`, `doc-after-deleted` | 0 | #66: the cluster document still answers, with the state `Deleted` |
| `listing-after`, `listing-drops-cluster` | 0 | #66: the listing no longer holds the cluster |
| `consumers-after`, `consumers-drops-cluster` | 0 | #66: `consumers` no longer lists it, and exits 0: the version has no consumer |
| `state-has-no-cluster` | 0 | an apply with `cluster=false` sees the deletion, and the OpenTofu state holds no cluster |

## order.sh

Read-write. It needs the example at its baseline with cluster A. It sends a
`PUT` of two published versions with their packs in the other order, and a `PUT`
of the cluster's profiles in the other order. It reads what Palette stores, what
it audits, and what `verify` says. It leaves the two versions edited, so
`live-down` follows it. A second run reverses them again, and passes the same
way.

| Check | Expected exit | What it shows |
|---|---|---|
| `release-before`, `registries-before`, `cluster-before`, and the `*-doc-before`, `*-doc-after`, `*-audits-before`, and `*-audits-after` checks | 0 | records of the release version, the registries version, and cluster A, and each document and its audit entries before and after each `PUT` |
| `reorder-addon`, `addon-doc-reversed`, `addon-audited` | 0 | #66: a `PUT` of the registries version with its two packs in the other order is accepted, stored in that order, and audited as an update of the profile |
| `addon-verify`, `addon-verify-says`, `registries-after`, `addon-record-reversed` | 1, 0, 0, 0 | #66: the record from before reports `DRIFT`, the report says `pack order changed`, and a new record lists the packs in the new order |
| `reorder-release`, `release-doc-reversed`, `release-audited` | 0 | #66: Palette accepts the four infrastructure layers of the release version in the other order, on a version cluster A runs: the CSI layer first, the OS layer last, and an audit entry |
| `release-verify`, `release-verify-says` | 1, 0 | #66: the record from before reports `DRIFT`, and the report says `pack order changed` |
| `reorder-cluster`, `cluster-order-kept`, `cluster-audited` | 0 | #66: a `PUT` of the cluster's profiles in the other order is accepted, the document keeps the stored order, and the audit log has an update of the cluster all the same |
| `cluster-verify` | 0 | #66: the cluster record verifies: strata orders the profiles itself, so a stored order is never a change |

## who.sh

Read-write. Stage 1 needs the profiles and no cluster. Stage 2 needs cluster A,
and records nothing without it. Each stage makes one edit in place through the
API, reads what `verify --who` says, and puts the edit back.

| Check | Expected exit | What it shows |
|---|---|---|
| `record`, `unchanged-who`, `unchanged-no-line` | 0, 0, 1 | #69: a record that is unchanged reads no audit log, and the output has no `who:` line |
| `api-edit`, `edit-who`, `edit-who-says` | 0, 1, 0 | #69: after an edit in place, the record reports `DRIFT`, and `--who` lists the writes since the record |
| `edit-who-person` | 0 | #69: the edit is one `update` by the user who owns the key, with a summary that names the manifest |
| `edit-who-json` | 0 | #69: `verify.who` in `--json` ends with that update: `actorType` `user`, an `actorUid`, and a time after the record |
| `api-undo`, `restored` | 0 | the edit is put back, and the record verifies again |
| `cluster-record`, `cluster-edit`, `cluster-apply` | 0 | record cluster A, edit the add-on version it runs, and accept the update on the cluster through the API |
| `cluster-who`, `cluster-who-says`, `cluster-who-person`, `cluster-who-json` | 1, 0, 0, 0 | #69: the cluster record reports `DRIFT`, and `--who` reads the audit log of the cluster: an `update` by the user who accepted it |
| `cluster-restored` | 0 | the edit is put back and accepted again, and the cluster record verifies |

## What has no live run

- `in-place.sh`, `registry-move.sh`, `registry.sh`, `template.sh`, and
  `cluster.sh` have run under OpenTofu, with the same provider that Terraform
  used, and give the same answers. `tag-only.sh` and `drift-report.sh` have run
  under Terraform only. `order.sh` reads the OpenTofu outputs and applies
  nothing.
- [What Palette does](../internals/palette.md#what-no-run-covers) lists the
  behaviors that no script here establishes.
