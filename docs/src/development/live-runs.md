# What the live example verifies

[The live example](./live-example.md) builds a real tenant and asserts what
Palette does with it. This page lists what each script sets up and what each of
its checks establishes, so you can see the coverage without running anything.

`verify.sh` is the read-only script over the cluster, and
[its checks are on the live example page](./live-example.md#the-checks-of-verifysh).
`packs.sh` and `knurl.sh` read too. The other scripts below write to the tenant;
`pack-states.sh` keeps nothing that it writes. `validate-packs.sh` writes only
in stage 4, which deletes the profile that it makes.

Each check names its expected exit code. A run that gets another code fails, and
prints both. The issue numbers name what the check was written for.

## in-place.sh

Edits a published add-on version in place through the API, pushes the update to
a cluster, and then bumps the version. It runs twice, once with the provider's
`immutable-clusterprofiles` flag off and once with it on, and nothing else
differs between the two runs. Cluster A exists before the edit; cluster B is
created from the edited version, and deleted at the end.

This is the source for
[Cluster copies](../internals/palette/clusters.md#cluster-copies) and
[Version bumps](../internals/palette/profiles.md#version-bumps).

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
| `api-edit` | 0 | 0 | #27: the API takes an edit in place, with the flag on or off |
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
| `verify-a3-who`, `a3-who-names-profile` | 1, 0 | 0, 1 | #140: `verify --who` of the same record reads the log of the overwritten profile, and a write names the add-on profile as its object, only without the flag. |
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
[A pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).

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
[Tag names](../internals/palette/packs.md#tag-names).

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
| `compare-images`, `compare-images-says` | 0 | #77: the diff flags `IMAGES` and `CHARTS` beside `AUTHORED`, and names each image that moved: from `csi-aws-ebs` 1.63.1 to 1.65.0, eight images retagged and the chart `aws-ebs-csi-driver` at 2.65.0 |
| `compare-same` | 0 | #55: a pack version against itself is identical |
| `record`, `record-title` | 0 | #55: `snapshot` writes a record of a pack version, which names the pack and not a profile |
| `verify-record` | 0 | #55: the record verifies against the registry it came from |
| `verify-sync`, `verify-sync-json` | 0 | #141, #192: the verdict ends with the last sync of the registry (the start of the last complete sync, a sync in progress, or a failure), and `--json` gives `verify.registrySync` |
| `record-images`, `old-record` | 0 | #77: the record carries the image list; a record without the list gets it from its values, and verifies |
| `latest`, `latest-is-newest` | 0 | #55: `@latest` resolves to the newest version the registry offers |
| `two-registries`, `two-registries-says` | 2, 0 | #55: a bare ref for a pack in two registries is an error that names both, and the ref that fixes it |
| `other-registry`, `registries-differ` | 0, 1 | #55: the version reads from either registry, and the two hold different builds |
| `unknown-version`, `unknown-registry` | 2 | #55: a version or a registry the tenant does not have is an error |
| `packs-one`, `packs-newest` | 0 | #54: `packs` lists a registry's versions newest first, and the newest carries the floating tag |
| `packs-latest`, `packs-two-registries` | 0 | #54: `--latest` gives one line for each registry that has the pack |
| `packs-paged-listing`, `packs-paged`, `packs-paged-count` | 0 | #54, #180: the check reads `kubernetes` in all registries, because the listing `/v1/packs?filters=spec.name=kubernetes` gives an entry for each version in each registry. The listing holds more than 100 entries, and the versions of all registries in `strata packs kubernetes` are more than 100. So strata reads past the first page. One registry alone can hold fewer than 100 versions. |
| `packs-cloud` | 0 | #54: `--cloud` keeps the versions of one cloud |
| `packs-cloud-none`, `packs-cloud-none-says` | 0 | #54: a cloud no version supports is an empty answer, not an error, and it says so |
| `packs-json` | 0 | #54: `--json` writes `strata-packs/1` |
| `packs-palette-latest` | 0 | #124: the listing marks one version of `kubernetes` in the registry as the latest of Palette, and `--latest --json` gives it `paletteLatest`: see [The version that Palette calls the latest](../internals/palette/packs.md#the-version-that-palette-calls-the-latest) |
| `packs-rev-floating-tag` | 0 | #124: the floating tag `1.19.x` of `cert-manager` names `1.19.3`, and not the rebuild `1.19.3-rev1` |
| `packs-unknown-registry`, `packs-unknown-pack` | 2 | #54: an unknown registry lists the visible ones; a pack no registry has is an error |
| `preset-listing` | 0 | #151: the listing gives each option of the `access` parameter of `headlamp` 0.45.0 with no presets: see [The presets of a template option](../internals/palette/packs.md#the-presets-of-a-template-option) |
| `preset-warning`, `preset-warning-says`, `preset-recorded` | 0 | #151: a candidate whose values select the option `public` warns, and records `packs[].presetOptions[]` |
| `os-record` | 0 | #189: a record of the release profile |
| `os-annotations` | 0 | #253: the record keeps no annotation of the OS pack, because `ubuntu-aws` has no `imageId` |
| `os-profile`, `os-annotations-ref` | 0 | #189: the document of the release profile gives the pack reference of the OS pack eight annotations, and no `imageId` |
| `os-pack-registry`, `os-annotations-registry` | 0 | #189: the pack version in the registry, `GET /v1/packs/<uid>`, gives the same annotations as the pack reference: see [The annotations of a pack reference](../internals/palette/packs.md#the-annotations-of-a-pack-reference) |
| `pack-time-listing` | 0 | #191: the listing gives each version of the CSI pack a `lastModifiedTimestamp` that is the same as its `creationTimestamp`: see [What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack) |
| `verify-no-time` | 0 | #191: `verify --json` gives no `verify.lastModified` for a pack record |
| `arch-listing` | 0 | #126: the listing names the architectures of `loki` 2.10.2 as `amd64,arm64`, and of 3.6.7 as `amd64`: see [The architectures of a version](../internals/palette/packs.md#the-architectures-of-a-version) |
| `arch-record`, `arch-behind`, `arch-behind-says`, `arch-behind-json` | 0, 1, 0, 0 | #126: a record of `loki` 2.10.2 is behind 3.6.7, and `behind` says that the move drops `arm64`, in the line and in `newer[].drops` |
| `bitnami-sync` | 0 | #122: the Helm summary listing gives the sync of the Bitnami registry: `InProgress`, with `syncRecovery` `Recovering` and `sync interrupted` |
| `stale-record`, `stale-current` | 0 | #122: a record of the newest `nginx` of Bitnami, and `behind` says that the pin is current |
| `stale-note`, `stale-json` | 0 | #122: the last sync did not complete, so the line of the pin names the sync that runs, the last complete sync, and the recovery. `--json` gives `pins[].registrySync` with the status of Palette: see [A pin that is current against an older copy](../commands/behind.md#a-pin-that-is-current-against-an-older-copy). When the last sync completed, `stale-no-note` checks that the pin has no note in its place. The sync of Bitnami stays `InProgress`, so no run shows `stale-no-note` |

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
that Palette creates at once. It also lists `gloo` 1.2.18 in `Public Repo`,
which carries the mark `disabled` of Spectro Cloud and no state.

| Check | Expected exit | What it shows |
|---|---|---|
| `states-disabled`, `states-disabled-says` | 0 | #68, #120: `packs` lists a disabled registry, and the heading says `disabled`. The `states` query gives the tags: `0.26.1` has `0.26.1 0.26.x` |
| `states-deleted`, `states-deleted-says` | 0 | #68, #120: the same for a deleted registry: the heading says `deleted`, and `1.0.6` has `1.0.6 1.0.x 1.x` |
| `states-deprecated`, `states-deprecated-says` | 0 | #68: a deprecated version keeps its tags, and the heading and the row say `deprecated` |
| `states-mark`, `states-mark-says` | 0 | #125: `packs` lists `gloo` 1.2.18 with its tags `1.2.18 1.2.x 1.x`, and the heading and the row say `disabled`, from the mark with no `system_state` |
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
| `pin-modified`, `profile-mod` | 0 | #81: a second profile pins the same version with values that differ from those of the pack |
| `push-2`, `push-2-differs`, `sync-2` | 0 | the same version is pushed again with other bytes, and the two pushes have two digests |
| `leaf-after`, `listing-follows` | 0 | #66: Palette lists the digest of the new push, under the same version |
| `leaf-verify` | 1 | #66: the record of the pack from before the push reports `DRIFT` |
| `profile-follows` | 1 | #81: the published profile with the values of the pack follows the push: its record from before reports `DRIFT`. The script verifies it again until it does, for up to ten minutes; it took about five |
| `profile-now`, `profile-now-digest` | 0 | #81: its pack reference now carries the digest of the second push |
| `profile-mod-keeps` | 0 | #81: the profile with values of its own keeps the digest it pinned: its record verifies |
| `profile-who`, `profile-who-system` | 1, 0 | #81: the audit log names the move: an `update` by the Palette service `system` |
| `delete-modified` | 0 | the second profile is deleted |
| `push-del`, `sync-del-1`, `del-record` | 0 | #108: 1.0.9 is pushed, and the profile pins it |
| `del-tag`, `sync-del-2` | 0 | #108: the tag 1.0.9 is deleted from the repository, and Palette reads the registry again |
| `del-flag` | ? | #108: Palette marks the pack of the profile invalid in ten minutes (recorded 0) |
| `del-doc`, `del-doc-fields` | 0, ? | #108: `GET /v1/clusterprofiles/<uid>` gives the pack `isInvalid: true` and `inValidReason` `strata live pack 1.0.9 tag is deleted` (recorded 0). The digest of the pack stays the digest of the deleted push |
| `del-record-now`, `del-verify` | 0, ? | #108: a record of the profile now, and `verify` of the record from before: `OK`, with an `INVALID` line (recorded 0) |
| `del-record-flag`, `del-record-root`, `del-verify-says` | 0 | #108: the record holds `invalid` and `invalidReason`, the root does not move, and `verify` prints `INVALID: Palette marks pack addon/strata-live-pack invalid now` |
| `push-del-again`, `sync-del-3`, `del-cleared`, `del-doc-after` | 0, 0, ?, 0 | #108: 1.0.9 is pushed again with other bytes, and Palette reads the registry again. Palette does not clear the flag in ten minutes (recorded 1) |
| `del-tag-end`, `sync-del-4` | 0 | the profile pins 1.0.0 again, and 1.0.9 is deleted, so step 4 finds the registry as step 3 left it |
| `profile-after`, `push-3`, `sync-3` | 0 | 1.0.1 is pushed |
| `packs-two`, `packs-two-order` | 0 | #66: `packs` lists 1.0.1 first, then 1.0.0 |
| `profile-stays` | 0 | #66: the profile pinned to 1.0.0 does not move to 1.0.1 |
| `profile-floating`, `push-4`, `sync-4` | 0 | a profile pins the pack under `1.0.x`, and 1.0.2 is pushed, which the tag now names |
| `pin-floating-modified`, `floating-mod` | 0 | #165: before the push, a second profile pins 1.0.1 under `1.0.x`, with values that differ from those of the pack |
| `floating-moves` | 1 | #66: the profile pinned under the floating tag moves with it. The script verifies its record again until it reports `DRIFT`, for up to ten minutes |
| `floating-who`, `floating-who-system`, `floating-who-versions` | 1, 0, 0 | #69: `verify --who` on the same record reads the audit log of the profile: updates by the Palette service `system`, one of which names the two versions, `1.0.1 > 1.0.2` |
| `floating-mod-keeps` | 0 | #165: the profile with values of its own stays on 1.0.1 under the tag: its record verifies after the sync that moved the first profile |
| `floating-mod-behind`, `floating-mod-behind-names` | 1, 0 | #165: `behind` on its record says that the pin is behind. It names 1.0.2 as newer, as the version that the tag names now, and as the latest of Palette |
| `delete-floating-modified` | 0 | the profile with values of its own is deleted |
| `chart-empty` | 0 | the chart repository starts empty, and two applies remove the Palette registry over it and make it again, so Palette reads the chart for the first time |
| `chart-push-1`, `chart-sync-1`, `chart-packs`, `chart-leaf` | 0 | `strata-live-chart` 1.0.0 is pushed with `helm push` into the example's OCI Helm registry, Palette reads it, and `packs` and `snapshot` read the chart as a pack of type `oci` |
| `chart-digest` | 0 | #66: Palette lists the manifest digest of the push for a chart in an OCI registry, as for a pack, not the digest of the chart archive |
| `chart-profile`, `chart-profile-digest` | 0 | #66: a profile that pins the chart carries that manifest digest in its pack reference |
| `chart-push-2`, `chart-push-2-differs`, `chart-sync-2`, `chart-leaf-after` | 0 | the same chart version is pushed again with other bytes, and Palette reads the registry again |
| `chart-listing-keeps`, `chart-listing-not-new` | 0 | #66: Palette keeps the digest it listed first for the version: the listing does not follow the push, unlike the pack registry |
| `chart-leaf-verify`, `chart-profile-verify` | 0 | #66: the record of the chart from before the push verifies, and so does the profile that pins it: Palette lists what it listed |
| `chart-push-3`, `chart-sync-3`, `chart-packs-two`, `chart-packs-two-versions` | 0 | #66: 1.0.1 is pushed, and the sync reads it: `packs` lists 1.0.1 first, then 1.0.0. A sync reads new versions, and keeps the digest of a known one |
| `dep-push`, `dep-push-requiring`, `dep-sync`, `dep-packs` | 0 | `strata-live-dep` 1.0.0 is pushed, then `strata-live-pack` 1.1.0, whose `pack.json` requires `strata-live-dep` 1.0.0 or newer; Palette reads both |
| `dep-packs-cloud`, `dep-packs-cloud-version` | 0 | #118: `packs strata-live-dep --registry strata-live-registry --cloud aws` lists `1.0.0`. Its `pack.json` gives the cloud type `all` alone, and the listing gives it `all` and each cloud of Palette: see [The cloud type `all`](../internals/palette/packs.md#the-cloud-type-all) |
| `dep-profile-alone`, `dep-profile-alone-says` | 1, 0 | #66: a profile with the requiring pack and not the required one is refused at creation, and the refusal names the required pack |
| `dep-profile-both` | 0 | #66: a profile with both packs is accepted |
| `dep-record`, `dep-record-packs` | 0 | a record of that profile lists the two packs, the requiring one first, as authored |

In step 3b, the document of the profile also gets the annotations `isInValid`
`"true"` and `inValidReason` `The cluster profile is invalid as one or more
pack tags are deleted from the registry`. Ten minutes after the push again, the
pack still has the flag, and the digest of the deleted push. This is the source
for [A tag that a registry deletes](../internals/palette/packs.md#a-tag-that-a-registry-deletes).

Palette refuses a sync while a sync of the registry runs. The sync route of an
OCI registry answers HTTP 500 with the code `OciSyncInProgress` and the message
"another sync process is already in progress". A run got that answer on
`chart-sync-1`, right after the apply that made the Palette registry over the
chart repository. Each check after it passed. So `api.sh sync` waits and asks
again until Palette takes the request, for up to five minutes, and then waits
until the sync ends. See
[The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry).

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
| `release-record-template`, `release-record-root` | 0 | #260: a record of the linked release version has no `source.clusterTemplates`, and has the root from before the link |
| `release-consumers`, `release-consumers-template` | 1, 0 | #147: `strata consumers` of the linked version names the template with `kind: template`, and exits `1`. |
| `release-list-consumers`, `release-list-consumers-template` | 0 | #176: `strata list --packs` counts one template and no cluster for the linked version, the count of `consumers --list-only`. No result yet |
| `release-delete`, `release-delete-says` | 2, 0 | #147: Palette refuses the delete of the linked version, `DeletionResourceInUseError`. |
| `lock-edit`, `lock-edit-says` | 2, 0 | #66: an edit in place of a linked version, a `PUT` of the whole version, is refused, and the refusal carries the code `ClusterProfileUpdateValidationFailure` |
| `lock-verify` | 0 | #66: the version is unchanged |
| `addon-before`, `lock-manifest-edit` | 0 | #81: the linked add-on version, edited through the route of one manifest and then published, is accepted: the lock does not cover that route |
| `lock-manifest-verify` | 1 | #81: the record of the linked add-on version from before reports `DRIFT` |
| `lock-manifest-back` | 0 | the manifest is put back, and the record verifies |
| `cluster-outside-template`, `refusal-says` | 1, 0 | #66: a cluster outside the template with a linked version is refused, `ClusterCreateWithTemplateProfileForbidden` |
| `cluster-runs-release`, `cluster-doc`, `cluster-three-profiles` | 0 | with the template gone, the cluster runs the release version and two add-on profiles |
| `lock-gone`, `lock-gone-verify` | 0, 1 | #66: the edit in place is accepted again, and the record from before reports `DRIFT`: the lock was the template's |
| `addon-order` | 0 | #66: the packs of the two add-on profiles start within thirty seconds of each other, whatever order the cluster lists the profiles in: the listed order is no install order. |

The script builds no cluster in the template. So no check shows the repave
answer for such a cluster yet: see
[The lock of a cluster template](../internals/palette/profiles.md#the-lock-of-a-cluster-template).

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
| `record-before-delete` | 0 | a record of the cluster's copy of the add-on profile, before the deletion |
| `delete` | 0 | #66: `DELETE /v1/spectroclusters/<uid>` |
| `listing-deleting`, `consumers-deleting`, `consumers-shows-deleting` | 0, 1, 0 | #66: while the cluster goes, the listing keeps it with the state `Deleting`, and `consumers` lists it with that state |
| `doc-after`, `doc-after-deleted` | 0 | #66: the cluster document still answers, with the state `Deleted` |
| `verify-deleted`, `verify-deleted-says` | 1, 0 | #139: `verify` of the record from before the deletion says `DRIFT: cluster <c> no longer exists: deleted <time>`, and exits 1. |
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
| `edit-who-resource` | 0 | #140: each entry of `verify.who` names the profile as its object: `resource.kind` `clusterprofile` and its uid. |
| `edit-last-modified` | 0 | #140: the document of the profile gives the time of the edit: `verify.lastModified` is after the record, and the `DRIFT` verdict ends with it. |
| `audits-none` | 0 | #140: Palette answers the log of an object with no entry with an empty list, and not `404`. |
| `audits-limit` | 0 | #188: Palette serves at most 100 entries on a page of the audit log: a `limit` of 101 answers `500` with the code `InvalidLimit`. |
| `api-undo`, `restored` | 0 | the edit is put back, and the record verifies again |
| `cluster-record`, `cluster-edit`, `cluster-apply` | 0 | record cluster A, edit the add-on version it runs, and accept the update on the cluster through the API |
| `cluster-who`, `cluster-who-says`, `cluster-who-person`, `cluster-who-json` | 1, 0, 0, 0 | #69: the cluster record reports `DRIFT`, and `--who` reads the audit log of the cluster: an `update` by the user who accepted it |
| `cluster-restored` | 0 | the edit is put back and accepted again, and the cluster record verifies |

## draft.sh

Read-write. It needs the profiles and no cluster. It edits the manifest of the
add-on version through the API with no publish, records the draft, publishes
it, and puts the edit back with a publish. Then it makes a copy of the add-on
version with no publish, lists it, and deletes it. This is the source for
[Drafts](../internals/palette/profiles.md#drafts).

| Check | Expected exit | What it shows |
|---|---|---|
| `record` | 0 | record the add-on version |
| `no-draft`, `no-draft-says` | 2, 0 | #155: a version with no draft: `<uid>@draft` exits `2`, and says that the version has no draft |
| `api-draft` | 0 | #155: a `PUT` of the version with no publish |
| `list-published` | 0 | #114: `list --packs` gives `published` true for the version while its draft waits |
| `version-holds` | 0 | #155: the version keeps its content while the draft waits: its record verifies |
| `draft-record`, `draft-template` | 0 | #155: `<uid>@draft` records the draft: `source.template` `draft`, and the edit in the manifest |
| `version-record` | 0 | #155: a record of the version, read while the draft waits, holds the published manifest |
| `compare`, `compare-names` | 1, 0 | #155: `compare <uid> <uid>@draft` exits `1`, and names the manifest pack with `AUTHORED` |
| `route-list` | 0 | #155: `packs/manifests` gives the published content of the manifest while the draft waits |
| `route-entity` | 0 | #155: the route of one manifest gives the draft content beside the published content |
| `no-has-draft` | 0 | #155: the profile document gives no `status.hasDraft` |
| `draft-before-publish` | 1 | #155: the record of the draft reports `DRIFT` against the version before the publish |
| `api-publish` | 0 | #155: the publish of the draft |
| `draft-after-publish` | 0 | #155: after the publish, the record of the draft verifies against the version |
| `version-after-publish` | 1 | #155: the record of the version from before the publish reports `DRIFT` |
| `no-draft-after-publish` | 2 | #155: the publish empties the draft |
| `restored` | 0 | the edit is put back, and the first record verifies again |
| `unpublished-create` | 0 | #114: a create of `strata-live-unpublished` with the packs of the add-on version, and no publish |
| `unpublished-listed`, `unpublished-mark` | 0 | #114: the profile listing names the version. `list --packs` gives `published` false, the packs of its draft, and the mark `draft` at the end of its row |
| `unpublished-delete` | 0 | #114: the delete of `strata-live-unpublished` |

## pending.sh

Read-write. It needs the example at its baseline with cluster A. Each stage
makes one change in place and puts it back, so it ends at the baseline.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-record`, `base-none` | 0 | #70: at the baseline, a record of cluster A says `updatesPending: false` |
| `base-profile-updates` | 0 | #70: `profileUpdates` lists each profile of the cluster also when no update waits: it is no flag |
| `base-at-pending`, `base-pending-same`, `base-pending-flag` | 0 | #78: with no update waiting, a record of `cluster:<uid>@pending` has the root of the cluster, and says `pending: true` |
| `base-consumers`, `base-list-only`, `base-list-only-same` | 1, 1, 0 | #78: `consumers --list-only` gives the clusters that the second reading gives, and says `skipped` |
| `addon-edit`, `addon-pending`, `addon-record`, `addon-names` | 0 | #70: an edit in place of a manifest of the add-on version opens an update: the record says `updatesPending: true`, and `pendingUpdates` names the add-on and `Manifest ... is updated` |
| `addon-root` | 0 | #70: the root of that record is the root at the baseline: the statement is outside the root |
| `addon-release-only` | 0 | #70: a record of the release profile alone says that no update waits for it |
| `addon-list`, `addon-list-says` | 0 | #70: `list clusters --applied` marks the add-on `behind: an update waits` |
| `addon-compare`, `addon-compare-says` | 1, 0 | #70: `compare` of the add-on version with its copy on cluster A says `cluster ... is behind` |
| `addon-at-pending`, `addon-pending-root` | 0 | #78: after the edit, the record of `@pending` has another root than the copy that runs |
| `addon-pending-compare`, `addon-pending-compare-says` | 1, 0 | #78: `compare cluster:<uid> cluster:<uid>@pending` names the pack of the add-on `AUTHORED`, and no pack of the release |
| `addon-pending-verify` | 0 | #78: `verify` of the pending record, while the update still waits |
| `accept`, `accept-none` | 0 | #70: a push of the content without `resolveNotification` closes the update all the same |
| `accept-record`, `accept-as-pending` | 0 | #78: the cluster then runs the root that the record of `@pending` held |
| `accept-undo`, `accept-undo-pending`, `accept-again`, `accept-again-none` | 0 | #70: the undo is an edit too, and opens an update; the accept with `resolveNotification` closes it |
| `values-edit`, `values-pending`, `values-record`, `values-names` | 0 | #70: an edit in place of a value of the release version opens an update, and Palette names the pack: `csi-aws-ebs ... values are updated` |
| `values-pending-compare`, `values-pending-compare-says` | 1, 0 | #78: `compare` with `@pending` names one pack, the CSI pack, and the path of the reclaim policy |
| `values-undo`, `values-undo-none` | 0 | #70: an edit that gives the profile the content of the copy again closes the update, with no update of the cluster |
| `values-baseline-none` | 0 | #70: the apply of the baseline, which writes the same content, opens no update |
| `bump-uid-kept`, `bump-record`, `bump-none` | 0 | #70: a new version number alone, in place with the flag off, keeps the uid and opens no update |

## macros.sh

Read-write. It needs the example at its baseline with cluster A, and `kubectl`.
It makes a project macro, has the manifest of the add-on version read it, and
changes its value. It reads the value inside the cluster with a kubeconfig that
it keeps in a temporary file. It puts each change back, and deletes the macro.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-flag`, `base-record`, `base-no-note` | 0, 0, 1 | #72: at the baseline, Palette says the add-on version has no user macros, and a report names none |
| `base-profile-resolved` | 0 | #111: Palette resolves the macros of the version with no cluster, in one `resolved` map, and the system macro of the cluster name is empty there |
| `macro-set`, `macro-edit`, `macro-flag` | 0 | #72: with the manifest reading a project macro, Palette says `hasUserMacros: true` |
| `macro-record`, `macro-names` | 0 | #72: a report of the record names the macro, and says the root covers its name and not its value |
| `macro-accept`, `macro-settled`, `macro-cluster`, `macro-cluster-plain`, `macro-digest` | 0 | #72: cluster A takes the update; a record with `--macros` holds `sha256:` of the value, and not the value |
| `macro-resolved-1` | 0 | #72: `resolvedValues` gives the value of the macro |
| `macro-version`, `macro-version-digest`, `macro-profile-resolved-1` | 0 | #111: a record of the version with `--macros` holds `sha256:` of the value at the scope of the version in `source.macroValues`; `resolvedValues` of the version gives the value of the project macro |
| `change`, `change-resolved` | 0 | #72: the macro gets another value, and `resolvedValues` gives it at once |
| `change-profile-resolved` | 0 | #111: `resolvedValues` of the version gives the new value at once |
| `change-version-macros`, `change-version-macros-says` | 1, 0 | #111: the record of the version with the digests reports `DRIFT`, and the verdict names the macro |
| `change-version`, `change-plain` | 0 | #72: the record of the version, and a cluster record without the digests, still verify |
| `change-cluster`, `change-cluster-says` | 1, 0 | #72: the cluster record with the digests reports `DRIFT`, and the verdict names the macro |
| `change-compare`, `change-compare-says` | 1, 0 | #72: `compare` of the two cluster records exits `1` with equal roots, and names the macro |
| `change-no-notification`, `change-no-audit` | 0 | #72: no notification opens for the cluster, and its audit log holds no new entry |
| `propagation` | 0 | #72: Palette writes the new value inside the cluster with no update to accept: after 11 minutes in the recorded run, where the check was recorded without an expected code |
| `undo-edit`, `undo-accept`, `undo-settled`, `undo-macro`, `undo-flag`, `undo-baseline` | 0 | the manifest back, the macro deleted, and the baseline |

## macro-repave.sh

Read-write, and it replaces the nodes of cluster A. It needs the example at its
baseline with cluster A, and `AWS_PROFILE` to read the EC2 instances. It puts a
project macro in `kubeadmconfig.postKubeadmCommands` of the Kubernetes pack,
has cluster A take that, then changes only the value of the macro, and watches
for 45 minutes. It ends with a repave that waits for approval, so `live-down`
follows it.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-instances`, `base-notifications`, `base-macro` | 0 | the EC2 instances of cluster A, no open notification, and no macro, at the baseline |
| `macro-set`, `macro-edit`, `edit-notification`, `macro-accept` | 0 | #72: the Kubernetes pack reads the macro; Palette opens a notification for that edit of the profile, and cluster A takes it |
| `accepted-instances`, `accepted-changed` | 0, ? | the edit of the profile replaced the nodes (`accepted-changed` recorded 0) |
| `accepted-resolved`, `accepted-record`, `accepted-record-plain`, `accepted-names` | 0 | the macro has its first value; cluster records with and without its digest |
| `change` | 0 | #72: only the value of the macro changes |
| `replaced` | ? | #72: the change of the value alone replaced no node in 45 minutes (recorded 1) |
| `notifications`, `notifications-none` | 0, ? | #72: no notification opened (recorded 1) |
| `audits`, `audits-macro`, `audits-cluster` | 0, ?, ? | #72: no audit entry names a macro, and no audit entry from a person names cluster A (recorded 1, 1) |
| `verify-plain` | ? | #72: a cluster record without the digest reports `OK` (recorded 0) |
| `verify-macros`, `verify-macros-says` | ?, ? | #72: a cluster record with the digest reports `DRIFT`, and names the macro (recorded 1, 0) |
| `undo-edit` | 0 | the Kubernetes pack back |
| `undo-accept`, `undo-accept-says` | 2, 0 | #72: while the repave waits for approval, Palette refuses the update of cluster A: `Cluster update failure as cluster repave is pending for approval` |
| `undo-macro` | 2 | #72: Palette refuses to delete a macro that cluster A reads |
| `repave-record`, `repave-names` | 0 | #107: a record of cluster A says `repavePending: true`, and `pendingRepave.reasons[]` names `kubernetes` and `PackValuesUpdated` |
| `repave-verify`, `repave-verify-says` | 0 | #107: `verify` of the record without the digest names the repave that waits |

The recorded run reads the repave state of cluster A in a second log, once a
minute: it became `Pending` 14 minutes 58 seconds after the change of the value,
at the next read of the agent. `GET /v1/spectroclusters/<uid>/repave/status`
gives the record: source `hubble`, reason `PackValuesUpdated` with the message
`Pack 'kubernetes' values are updated`, `diffConfigKeys`
`kubeadmconfig.postKubeadmCommands`, and the resolved values on each side, `echo
one` and `echo two`. The record names no macro.

A second run gives the same result for each check of #72. It also shows the
checks of #107. A record of cluster A while the repave waits says
`repavePending: true`. Its `pendingRepave.reasons[]` holds the code
`PackValuesUpdated`, the pack `kubernetes`, and the key
`kubeadmconfig.postKubeadmCommands`. `verify` of a record from before the change
exits `0`, and its note names the repave that waits, with the same reason.

The run expected `undo-accept` to exit `0`, and it got `2`; the script now
expects the refusal. It stopped at `undo-accept`, so
`undo-accept-says` and `undo-macro` come from the same refusal read by hand.

The teardown after this run showed that the repave also holds a delete. `tofu
destroy` asked Palette to delete cluster A at 04:05:45, and the cluster stayed
in `Deleting` with its EC2 instances running for 37 minutes, until
`PATCH /v1/spectroclusters/<uid>/repave/approve` at 04:42:43. The agent deleted
the machines 45 seconds later, and no instance was left at 04:46:42. The audit
log holds no entry for the approval.

## repave.sh

Read-write, and it replaces no node. It needs the example at its baseline with
cluster A, and `AWS_PROFILE` to read the EC2 instances. It edits the release
version and the add-on version in place, asks Palette with `compare --repave`,
and puts each edit back. Cluster A takes no update. Stage 7 asks Palette with
`compare --validate`. Stage 8 changes version 1.0.1, which nothing runs,
through OpenTofu, and puts it back.

| Check | Expected exit | What it shows |
|---|---|---|
| `none`, `none-says`, `none-json` | 0 | #73: at the baseline, Palette says `no repave`; `--json` gives `required` false and `agrees` true |
| `k8s-edit` | 0 | the Kubernetes pack of the release version gets one more kubeadm command |
| `k8s`, `k8s-says` | 1, 0 | #73: Palette says `repave: PackValuesUpdated kubernetes (kubeadmconfig.postKubeadmCommands)` |
| `k8s-agrees`, `k8s-json` | 0 | #73: the impact table says `may repave`, so strata prints no disagreement; `--json` names the key |
| `k8s-reversed`, `k8s-reversed-says` | 1, 0 | #73: the two sides in the other order give the same answer |
| `helper`, `helper-says` | 1, 0 | #73: `tenant.sh repave` asks for the same change and gets the same key |
| `k8s-undo` | 0 | the Kubernetes pack back |
| `addon-edit`, `addon`, `addon-says`, `addon-agrees` | 0, 1, 0, 0 | #73: a manifest of the add-on changes; Palette says `no repave`, and the table says in-place |
| `addon-undo` | 0 | the manifest back |
| `usage`, `usage-says` | 2, 0 | #73: two profile versions and no cluster are a usage error |
| `no-update`, `same-nodes` | 0 | #73: the questions took no update, and cluster A kept each EC2 instance |
| `undo-baseline` | 0 | the example is at its baseline |
| `release-record`, `os-value` | 0, 1 | #81: a new key in the values of the OS pack: Palette says repave. It checks every value of the OS layer, so the table gives `repave` for an OS values change |
| `csi-value` | 0 | #81: a changed value in the CSI pack: Palette says no repave. Its check reads no CNI or CSI layer, so `cluster-wide` means no repave |
| `validate`, `validate-says`, `validate-json` | 0 | #134: at the baseline, `compare --validate` prints `palette    accepts`; `--json` gives `accepts` true and no error. |
| `validate-usage`, `validate-usage-says` | 2, 0 | #134: two profile versions and no cluster are a usage error for `--validate`. |
| `var-record-before`, `var-command` | 0 | #132: the Kubernetes pack of 1.0.1 has the kubeadm command `chmod 600 /var/lib/kubelet/config.yaml` |
| `var-record`, `var-in-values` | 0 | #132: with `next_variable` and `next_variable_in_values`, 1.0.1 has the profile variable `strata_mode`, default `600`, and the command reads it in place of `600` |
| `var-strata`, `var-strata-answer` | ? | #132: `compare --repave` of cluster A with 1.0.1 sends the uid of 1.0.1 alone, with no `packValues` and no variables. Palette says no repave: `required` false (recorded 1, 1) |
| `var-none`, `var-default` | ? | #132: `tenant.sh repave` sends the uid of 1.0.0, `replaceWithProfile` 1.0.1, and the `packValues` of 1.0.1. With no variables, and with `strata_mode` `600`, Palette says repave: `PackValuesUpdated` for `kubeadmconfig.postKubeadmCommands` (recorded 1, 1) |
| `var-control`, `var-control-says` | 1, 0 | #132: with `strata_mode` `644`, the command changes, and Palette says repave for `kubeadmconfig.postKubeadmCommands` |
| `var-undone` | 0 | 1.0.1 is as before the stage: its record from before verifies |

Each answer agrees with the impact table, on AWS and on GCP: the two clouds run
the kubeadm engine alone. The edit in place also opens an update notification for cluster A, and
`compare` names it; the undo closes it.

In stage 8, `var-none`, `var-default`, and `var-control` give the same
answer. So the value that the body gives a variable does not change the answer,
and `compare --repave` sends no `variables[]`. The body of strata and the body
of `tenant.sh` get two different answers for one move. This is the source for
[A variable in a field that repaves](../internals/palette/profiles.md#a-variable-in-a-field-that-repaves).

## registry-move-nodes.sh

Read-write, and it can replace the nodes of cluster A two times. It needs the
example at its baseline with cluster A. `move_os_registry` takes the OS pack of
the release version from `move_registry_name`, at the same version and with the
same values, in place. Cluster A takes the change, and the script watches it
for `WATCH_MINUTES` (default 30). Then the release version goes back. When a
repave waits for approval, Palette refuses the update, so the script stops
before the undo, and `live-down` follows it.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-instances`, `base-record` | 0 | the machines of cluster A, three EC2 instances, and a record of the release version |
| `move-record`, `move-same-uid`, `move-identity` | 0 | #135: the move is in place, and the OS pack has another registry and the type `oci`, with the same version and values. The pack uid changes too |
| `move-compare`, `move-impact`, `move-palette`, `move-reasons` | ? | #135: the impact table gave the OS pack `repave` (recorded 0), and `--json` gave `agrees` false. Palette says no repave, with no reason (recorded 1, 0). The table now takes this result: the OS pack is `sync`, and `move-impact` exits 1 |
| `take` | 0 | #135: cluster A takes the update |
| `replaced`, `waits` | ? | #135: for 33 minutes, cluster A keeps the same three EC2 instances, and no repave waits for approval (recorded 1, 1) |
| `after-record`, `after-runs` | 0 | #135: cluster A runs the OS pack from the other registry |
| `undo-take`, `undo-baseline` | 0 | the release version back, and the example at its baseline |

The watch reads cluster A 30 times in 33 minutes: it stays `Running` with the
same instances each time. So a move of the OS pack to another registry, at one version and
with the same values, replaces no node, as Palette says. The impact table took
this result. This is the source for
[A pack that moves between registries](../internals/palette/packs.md#a-pack-that-moves-between-registries).

## validate-packs.sh

No cluster and no cost. In stages 1, 2, 3, and 5, each request to Palette is a
read or a request to a validate route, which writes nothing. It uses `cert-manager`
1.21.1 from the Public Repo, in an add-on candidate, and `gloo` 1.2.18 for the
mark `disabled`. Stage 3 runs only with a profile version of the project: give
it as `PROFILE=<name>@<version>`. With the add-on version of the live example,
Palette takes the version on the route of an update. It refuses a candidate
with that name and version on the name route. Stage 4 makes the profile
`strata-live-layer` and deletes it. It runs only when you give its number:
`validate-packs.sh 4`. Stage 5 asks the name route and the import validate
route.

| Check | Expected exit | What it shows |
|---|---|---|
| `validate-accepts`, `validate-accepts-says`, `validate-accepts-record` | 0 | #152: Palette takes the packs of the candidate; `snapshot --validate` prints `palette    accepts` and writes the record with `source.paletteValidation` `{"accepts": true}` |
| `validate-not-yaml`, `validate-not-yaml-says`, `validate-not-yaml-no-record` | 2, 0, 1 | #152, #242: strata sends values that are not YAML to Palette. Palette refuses the whole request, `PackValuesYamlInvalid`, with no pack. Strata exits `2` and writes no record |
| `validate-not-yaml-status`, `validate-not-yaml-status-says` | 2, 0 | #152: Palette refuses the same body as the whole request, `PackValuesYamlInvalid` with no pack, and sends the refusal as HTTP 500 |
| `validate-no-pack`, `validate-no-pack-says` | 2, 0 | #152: Palette refuses a body with no pack before it checks the packs: HTTP 400 with `ShouldNotBeEmpty` |
| `validate-disabled-mark`, `validate-disabled-mark-says` | 2, 0 | #125: Palette refuses a candidate with `gloo` 1.2.18, which carries the mark `disabled` and no `system_state`: `DisabledPacksForbidden`, `Disabled packs 'gloo' are forbidden`, of the whole request. Strata exits `2` and writes no record |
| `validate-version`, `validate-version-says` | 0 | #152: `snapshot <ref> --validate` asks the route of an update, and Palette takes the version as it is |
| `validate-exists`, `validate-exists-says` | 2, 0 | #273: a candidate with the name and the version of `PROFILE` gets `ClusterProfileAlreadyExists`, "Cluster Profile with name '<name>' and version '<version>' already exists". Strata prints `palette    refuses`, exits `2`, and writes no record |
| `layer-registry` | 0 | #210: the registry gives `cert-manager` the layer `addon` |
| `layer-refused`, `layer-refused-says` | 2, 0 | #210: strata refuses a candidate that gives `cert-manager` the layer `csi`. The message names the layer, `csi`, `addon`, and the rule of Palette |
| `layer-validate`, `layer-validate-says` | 0 | #254: the validate route takes the pack with the layer `csi`, and gives no result for it |
| `layer-create` | 0 | #254: Palette makes and publishes `strata-live-layer` 1.0.0, with `cert-manager` in the layer `csi`, with no error |
| `layer-profile`, `layer-published` | 0 | #254: the published pack has the layer `addon` of the registry, not `csi` |
| `layer-record`, `layer-record-key` | 0 | #254: a record of the version holds the pack as `addon/cert-manager` |
| `layer-root` | 0 | #254: `compare` of the candidate with no layer and the version finds the same root, `IDENTICAL` |
| `layer-delete` | 0 | the profile `strata-live-layer` is deleted |
| `validate-name-free`, `validate-name-short` | 0 | #273: the name route takes a name that the project does not hold, and the version `1.1`, with `204` |
| `validate-name-version`, `validate-name-version-says` | 2, 0 | #273: the name route refuses the version `2.2.2.develop` with HTTP 500 and `VersionParseFailure` |
| `validate-bad-version`, `-says`, `-no-record` | 2, 0, 1 | #273: `snapshot --validate` prints `VersionParseFailure` once, from the two routes, and writes no record |
| `validate-pack-twice`, `-says`, `-no-record` | 2, 0, 1 | #273: the import validate route refuses one pack name in two layers with other values: `DuplicatePacksForbidden` |
| `validate-same-pack-twice`, `-says`, `-no-record` | 2, 0, 1 | #273: the API schema refuses two equal layers with HTTP 422 and the code `610` |
| `validate-manifest-name`, `-says`, `-no-record` | 2, 0, 1 | #273: the import validate route refuses the manifest name `Wilma_1`: `InvalidName` |
| `validate-manifest-twice`, `-says`, `-no-record` | 2, 0, 1 | #273: the import validate route refuses one manifest name two times in a layer: `DuplicatePackManifestsForbidden` |

No pack of the Public Repo lists a dependency in its tags route, so no stage
asks for a refusal of a dependency.

The validate route does not check the layer of a pack. Palette answers a pack
with another layer than its registry with `{"packs":{"results":[]}}`, the
answer of a body that it takes. So strata refuses the candidate itself: see
[The layer of a pack](../internals/palette/packs.md#the-layer-of-a-pack).

## export.sh

No cluster, no profile of its own, and no cost. Each request is a `GET`. It
reads the export of three versions of the example:

- the release version, with four packs from the Public Repo
- the registries version, with a Helm pack and an OCI pack
- the add-on version, with a manifest pack and a variable

Give other versions as `PROFILES="<name>@<version> ..."`.
[The export of a version](../internals/palette/profiles.md#the-export-of-a-version)
states what it shows.

| Check | Expected exit | What it shows |
|---|---|---|
| `export-<name>` | 0 | #156: `GET /v1/clusterprofiles/{uid}/export?format=yaml` gives the file of the version |
| `export-record-<name>`, `export-same-<name>` | 0 | #156: `snapshot --from-candidate` reads the export, and `same-release` passes it with the record of the version |
| `export-root-<name>` | 0 | #156: the record of the export of the release version and of the registries version has the root of the version |
| `export-minted-<name>` | 0 | #156: the export of a version with a manifest pack differs only by the pack uid that Palette mints for that pack |
| `export-json-<name>`, `export-json-record-<name>`, `export-json-root-<name>` | 0 | #156: the export with `format=json` gives the same root |
| `export-accepts-json`, `export-accepts-json-no-file` | 0 | #156: to a request with `Accept: application/json`, Palette sends `{"Reader":{}}`, and not the file |
| `export-limit` | ? | #156: three exports at once. In two of four runs, Palette refused one or more with HTTP 429, `TooManyRequests`, "only 1 request in 10 seconds is allowed". In the other two, it refused none |

## registry-kinds.sh

Read-write in the registries of its own host alone, and it needs no cluster. It
turns `registry_kinds` on: a host in AWS that serves a Helm repository, a Helm
repository with credentials, an OCI registry with basic auth, and a Spectro
pack registry, and a Palette registry over each. It removes each at its end.
[The digest of each registry kind](../internals/palette/packs.md#the-digest-of-each-registry-kind)
states what it shows.

| Check | Expected exit | What it shows |
|---|---|---|
| `push-*`, `packs-registry` | 0 | version `1.0.0` into each kind, and a Palette registry over the Spectro pack registry |
| `sync-<kind>-first`, `record-<kind>-first` | 0 | Palette reads each registry, and a record of each pack |
| `charts-digest` | 0 | #75: the Helm repository: the sha256 of the chart archive, bare hex |
| `oci-pack-digest`, `oci-helm-digest` | 0 | #75: OCI `pack` and `helm`: the manifest digest of the push |
| `oci-zarf-digest` | 0 | #75: OCI `zarf`: no digest |
| `packs-zarf`, `packs-zarf-says` | 0 | #128: `strata packs` of the Zarf pack says `no digest` on the row of `1.0.0`, and `no digests` in the heading |
| `provider-types` | 0 | #75: each record of an OCI pack names `pack`, `helm`, or `zarf`, and the Helm record and the Spectro pack record name none |
| `packs-digest` | 0 | #75: the Spectro pack registry: the digest of the push |
| `auth-listed` | ? | #75: Palette lists no chart of the Helm repository with credentials while no profile pins one (recorded 2) |
| `auth-candidate`, `auth-candidate-minted` | 0 | #154: a candidate with the chart builds a record before any profile pins it: no pack uid, no digest, the layer `addon`, the type `helm`, and the tag as the version |
| `auth-packs`, `auth-packs-says` | 2, 0 | #123: `packs --registry` of the Helm repository with credentials is `pack-not-found`. The error says that Palette does not sync the registry, and lists a pack of such a registry only while a profile pins the pack |
| `auth-behind`, `auth-behind-says` | 2, 0 | #123: `behind` on the candidate record cannot check the pin, `not checked`. The line under the pin names the registry that Palette does not sync |
| `auth-pin`, `auth-record` | 0 | a profile that pins the chart by name, and a record of it |
| `auth-candidate-same`, `auth-candidate-minted-only` | 0 | #154: `same-release` with `--allow no-digest` passes the candidate and the published version, and `mintedUidOnly` names the chart |
| `auth-candidate-again`, `auth-candidate-listed` | 0, 0 | #154: a record of the candidate after the create has the published root, because the listing now has the chart |
| `auth-no-digest` | 0 | #75 #71: the pack reference carries no digest, and Palette made a pack uid |
| `auth-verify`, `auth-verify-says`, `auth-verify-allowed` | 1, 0, 0 | #71: `verify` reports `UNVERIFIED`, and `--allow-no-digest` accepts it |
| `auth-same-release`, `auth-same-release-allowed` | 1, 0 | #71: `same-release` refuses it, and `--allow no-digest` accepts it |
| `push2-*`, `push2-differs` | 0 | `1.0.0` again into each kind, with other bytes |
| `charts-keeps`, `charts-verify` | 0 | #75: the Helm repository keeps the first digest, and the record from before verifies |
| `oci-pack-follows`, `oci-pack-verify` | 0, 1 | #75: OCI `pack` gives the digest of the new push, and the record from before reports `DRIFT` |
| `oci-helm-keeps`, `oci-helm-verify` | 0 | #75: OCI `helm` keeps the first digest, and the record from before verifies |
| `oci-zarf-none`, `oci-zarf-verify` | 0, 1 | #75: OCI `zarf` still gives no digest, and the record from before reports `UNVERIFIED` |
| `packs-follows`, `packs-verify` | 0, 1 | #75: the Spectro pack registry gives the digest of the new push, and the record from before reports `DRIFT` |

## tiers.sh

Read-write: it builds cluster B, changes its timezone and its labels through the
API, and removes it. It needs the example at its baseline with cluster A. Stage
5 changes the role bindings of cluster A through the API, and puts them back.
It needs no cluster B: `tiers.sh 5` runs it alone.

| Check | Expected exit | What it shows |
|---|---|---|
| `a-record`, `a-tiers` | 0 | #76: a record of cluster A with `--shape` carries `applied`, `policy`, `running`, and `shape`, and the labels |
| `a-applied`, `a-running`, `a-shape` | 0 | #76: Palette's two hashes; one `running` entry for each pack of the record; two machine pools and the region, and no subnet id |
| `a-arch` | 0 | #105: the shape tier names the cloud type, and Palette gives the architecture of each machine pool, `amd64` or `arm64` |
| `a-behind` | 0 | #76: at the baseline, no pack of cluster A runs another version than its profile declares |
| `b-record`, `same-release` | 0 | #76: a record of cluster B, from the same definition; the release is the same |
| `same-applied`, `same-policy`, `same-running`, `same-shape`, `same-labels` | ? | #76: each tier of the two clusters: `policy`, `running`, `shape`, and the labels are the same (recorded 0); `applied` differs at `spcHash` (recorded 1) |
| `b-timezone`, `b-labels` | 0 | cluster B gets the timezone `America/Chicago` and the label `team=search` |
| `differ`, `differ-policy`, `differ-labels`, `differ-says` | 0 | #76: still one release; `policy` differs at `timezone`, and the labels at `team`; the report says `~ policy.timezone: "Etc/UTC" -> "America/Chicago"` |
| `b-verify`, `b-verify-says` | 0 | #76: the record of B from before still verifies, and names `policy CHANGED` |
| `undo-baseline` | 0 | the example is at its baseline |
| `rbac-before`, `rbac-add` | 0 | #106: the role bindings of cluster A, then one more binding of `barney@example.com` to the cluster role `view`, through `PUT /v1/spectroclusters/<uid>/config/rbacs`. Cluster A has no binding before the stage |
| `rbac-record`, `rbac-in-record` | 0 | #106: the `policy` tier of a record of cluster A holds the binding: the type `ClusterRoleBinding`, the role `ClusterRole` `view`, and the subject `barney@example.com` of type `User` |
| `rbac-edit` | 0 | #106: the subject of the binding becomes `fred@example.com` |
| `rbac-compare`, `rbac-compare-policy`, `rbac-compare-says` | 0 | #106: `compare` of the record with cluster A exits `0` with the root `IDENTICAL`, and names `policy.rbacs[0].subjects[name=fred@example.com]` |
| `rbac-verify`, `rbac-verify-says` | 0 | #106: `verify` of the record exits `0`, and names `policy CHANGED` |
| `rbac-undo`, `rbac-after-read`, `rbac-after` | 0 | the role bindings of cluster A are as before the stage |

`spcHash` is its own for each cluster, so `compare` now reads `spcInfraHash`
alone between two clusters. On the two records of the run, `compare` then says
`cluster tiers unchanged`, and after the change names `policy.timezone` and
`labels.team` alone. The `verify` of B also names `applied CHANGED`: the change
of the timezone and the labels changed the `spcHash` of B.

No run makes a cluster run another pack version than its profile declares; the
book shows it on the stand-in.

Stage 5 reads the role bindings of cluster A, and no script reads the
namespaces of a cluster. Read-only requests to the tenant show the other facts
in [Cluster tiers](../internals/palette/clusters.md#cluster-tiers). A deleted
cluster of the live example gives `{"items": []}` for each route. A uid that is
not a cluster of the project gives HTTP 404 `ResourceNotFound`. The edit of the
subjects in stage 5 also changes the `spcHash` of cluster A, so `compare` and
`verify` name `applied CHANGED` beside `policy CHANGED`.

## fleet.sh

Read-write: it gives cluster A the label `env=prod` through the API, makes a
saved tag filter, and puts both back. It needs the example at its baseline with
cluster A.

| Check | Expected exit | What it shows |
|---|---|---|
| `listing`, `listing-labels` | 0 | #80: the cluster listing carries the labels of cluster A: the tags of the example |
| `record`, `record-labels` | 0 | #80: a record of cluster A carries them |
| `tag`, `tag-search` | 0 | #80: `list clusters --tag` finds cluster A through the search of Palette, at tenant scope, with the uid and the name of its project |
| `label`, `prod`, `prod-row` | 0 | #80: with the label `env=prod`, `--tag env=prod` finds cluster A alone |
| `none` | 0 | #80: a query that no cluster meets finds none |
| `filter-create`, `filters`, `by-filter` | 0 | #80: `list filters` names a saved tag filter and what it selects, and `list clusters --filter` finds cluster A |
| `set`, `set-files`, `set-record` | 0 | #80: `snapshot filter:<name> --dir --prune` writes one record, of cluster A, with the label, removes the record of a cluster that left the set, and leaves a file that is not a record |
| `set-tag` | 0 | #80: `snapshot tag:env=prod --dir` does the same |
| `set-verify` | 0 | #80: the record that the set wrote verifies |
| `unlabel`, `set-again`, `set-removed` | 0 | #80: without the label, the next run removes the record of cluster A |
| `filter-delete`, `baseline` | 0 | the filter is deleted, and the example is at its baseline |

## upstream.sh

Read-write: it pushes three versions of the example's own pack into its own OCI
registry, one of them between two runs, and opens and closes issues of this
repository with the label `strata-live-behind`. It needs the profiles and the
registry of the example, and no cluster. It empties the registry at its end.

| Check | Expected exit | What it shows |
|---|---|---|
| `empty`, `no-issues` | 0 | the registry is empty, and no issue of the test is open |
| `push-1.0.0`, `push-1.1.0`, `sync-1` | 0 | two versions of the pack, and Palette reads the registry again |
| `record-own`, `record-csi`, `record-release` | 0 | records of the pack at 1.0.0, of `csi-aws-ebs` 1.63.1 of the Public Repo, and of the release version |
| `behind-1`, `behind-schema` | 1, 0 | #79: `strata behind` exits 1, with `strata-behind/1` |
| `behind-own` | 0 | #79: the pack at 1.0.0 is behind 1.1.0, with the impact of the bump |
| `behind-csi` | 0 | #79: `csi-aws-ebs` 1.63.1 is behind the newer versions of the Public Repo, and the bump names `IDENTITY` |
| `behind-release` | 0 | #79: each pin of the release version is checked |
| `behind-says` | 0 | #79: the report line is `~ [addon] <pack> 1.0.0 <registry> behind: 1.1.0` |
| `report-1`, `report-1-issues`, `report-1-first` | 1, 0, 0 | #79: `upstream-report.sh` opens one issue for each pack behind, and each says that this run is the first to find it behind |
| `push-1.2.0`, `sync-3` | 0 | a third version arrives |
| `behind-since`, `behind-arrived` | 1, 0 | #79: `--since` the first run names 1.2.0 as arrived, and nothing of `csi-aws-ebs` |
| `behind-listed` | 0 | #79: Palette lists 1.2.0 with the time it first listed it, at or after the push |
| `report-2`, `report-2-arrived` | 1, 0 | #79: the second run of the script names 1.2.0 as arrived in the issue, from the state that the issue kept |
| `record-own-new`, `record-csi-new`, `report-3`, `report-3-closed` | 0 | #79: with each pin at the newest version, the script closes the issues |
| `empty-end` | 0 | the registry is emptied again |

## edge-host.sh

Read-write: it registers two edge hosts with no device, binds a profile to each,
edits a profile in place and back, and deletes the hosts. It needs the example
at its baseline. It needs no cluster of its own and costs nothing.

| Check | Expected exit | What it shows |
|---|---|---|
| `register` | 0 | an edge host registered with no device, and the add-on version bound to it |
| `addon-record`, `host-record`, `host-flag` | 0 | #78: a record of `edgehost:<uid>` says `kind: edgeHost`, with the uid and the name of the host |
| `host-root`, `host-compare` | 0 | #78: the record has the root of the add-on version: the same packs and variable definitions |
| `host-by-name` | 0 | #78: `edgehost:<name>` finds the host through the search |
| `host-unknown`, `host-unknown-says` | 2, 0 | #78: an edge host that is not there is `edge-host-not-found`, which lists the edge hosts |
| `consumers`, `consumers-host` | 1, 0 | #78: `consumers` of the add-on version counts the edge host, with its kind, project, state, and version |
| `consumers-list-only`, `consumers-list-only-host` | 1, 0 | #78: `--list-only` counts it too: the consumer list of Palette names no edge host |
| `host-verify` | 0 | #78: `verify` of the record of the edge host |
| `host-repave` | 2 | #78: `compare --repave` refuses an edge host |
| `edit`, `edit-host-verify` | 0, 0 | #81: after an edit in place of the manifest of the add-on version, `verify` of the record of the edge host passes: the host keeps its own copy |
| `edit-host-vs-version` | 1 | #81: the version differs from the copy of the host |
| `edit-host-own-uid`, `edit-host-copy` | 0 | #81: the copy has a uid of its own, and its route gives the content from before the edit |
| `edit-host-raw`, `edit-host-manifest-uid` | 0 | #78: the template of the edge host names the manifest by its uid, with no content |
| `release-register`, `release-host-record`, `release-edit`, `release-host-verify` | 0 | #78: a second host bound to the release version keeps its copy of the values after an edit in place of the reclaim policy: `verify` of its record is OK |
| `release-host-raw`, `release-host-kept` | 0 | #78: the host holds `reclaimPolicy: Delete`, and the profile holds `Retain` |
| `release-undo`, `release-delete` | 0 | the edit is put back, and the second host deleted |
| `delete`, `after`, `after-no-host` | 0, 1, 0 | #78: after the delete, `consumers` no longer counts the edge host |

An edge host with no device is `unpaired`. No run pairs a device.

## cluster-group.sh

Read-write: it sets `cluster_group`, which builds a cluster group on cluster A
that pins the add-on version, and a virtual cluster in it; it edits the add-on
version in place and back, changes the CPU limit of the group through
OpenTofu and back, and removes the group. It needs the example at its baseline
with cluster A.

| Check | Expected exit | What it shows |
|---|---|---|
| `addon-record`, `group-record`, `group-kind` | 0 | #78: a record of `clustergroup:<uid>` says `kind: clusterGroup`, and holds the add-on version alone: the pin |
| `group-members` | 0 | #78: the record names the virtual cluster as a member |
| `group-root` | 0 | #78: the record has the root of the add-on version |
| `group-by-name` | 0 | #78: `clustergroup:<name>` finds the group |
| `member-record`, `member-runs-pin` | 0 | #78: the virtual cluster runs the add-on version that the group pins |
| `group-compare`, `group-compare-record` | 0 | #78: `compare` of the group, or of its record, with the virtual cluster reads the pinned profile of the member: `IDENTICAL` |
| `group-compare-no-values` | 0 | #78: a group gives no variable values, so the compare states no cluster value |
| `group-verify` | 0 | #78: `verify` of the record of the group |
| `group-macros`, `group-macro-values`, `group-macros-verify` | 0 | #111: a record of `clustergroup:<uid>` with `--macros` holds `cluster.macroValues`, and `verify` of it reads the values of the group again |
| `member-consumer`, `member-consumer-listed` | 1, 0 | #78: `consumers` of the add-on version names the virtual cluster, from the consumer list of Palette |
| `member-not-in-listing` | 0 | #78: the cluster listing does not name the virtual cluster |
| `member-listed`, `member-listed-group` | 0 | #116: `list clusters --json` names the virtual cluster, with `virtual.clusterGroup` the group, and `includesVirtual` true (no result yet) |
| `member-by-name` | 0 | #116: `cluster:<name>` of the virtual cluster resolves to its uid (no result yet) |
| `search-profile`, `search-profile-runs` | 0 | #181: the cluster search with `clusterProfiles eq <add-on name>` answers cluster A, which runs the add-on version, and not the virtual cluster |
| `search-nested`, `search-nested-misses-member` | 0 | #184: the same search with `environment eq nested` does not answer the virtual cluster of the group, which runs the add-on version through its group |
| `member-consumer-search` | 0 | #149: `consumers` does its second reading through the search |
| `member-consumer-group`, `group-virtual-clusters` | 0 | #184: `consumers` finds the group that pins the add-on version, and the virtual clusters of the group, which it reads, name the virtual cluster |
| `edit`, `edit-group-verify` | 0, 0 | #81: after an edit in place of the manifest of the add-on version, `verify` of the record of the group passes: the group keeps its own copy |
| `edit-group-compare` | 0 | #81: the member keeps its copy too, so the group and the member still agree |
| `edit-group-vs-version`, `edit-group-vs-version-says` | 1, 0 | #81: the version differs from the copy of the group, and the compare names the manifest |
| `edit-group-own-uid` | 0 | #81: the copy has a uid of its own |
| `edit-group-raw`, `edit-group-manifest-uid` | 0 | #78: the template of the group names the manifest by its uid, with no content. #110: the document of the group holds `spec.clustersConfig`, with each limit in two units, the values as YAML with comments, and the host cluster |
| `edit-undo` | 0 | the edit is put back |
| `limit-record`, `limit-before` | 0 | #110: a record of the group holds the CPU limit `cpuMilliCore` 6000 in its `policy` tier |
| `limit-verify`, `limit-verify-says` | 0 | #110: after `group_cpu_millicore=4000`, `verify` of the record exits `0` with the root `IDENTICAL`, and names `policy CHANGED` |
| `limit-compare`, `limit-compare-says`, `limit-compare-json`, `limit-compare-tier` | 0 | #110: `compare` of the record with the group exits `0`, and names `policy.limitConfig.cpuMilliCore: 6000 -> 4000`. Palette also changes `cpu` from 6 to 4 |
| `limit-undone` | 0 | the limit is back: `compare` finds the same `policy` tier |
| `gone` | 0 | the group and the member are gone |
| `member-audit-kind` | 0 | #188: the audit log of the member, read by its uid alone, has its create and its delete under the kind `virtualCluster` |
| `member-audit-not-cluster` | 0 | #188: a query with the kind `spectrocluster` and the uid of the member does not find them |

The virtual cluster runs with a CPU limit of 3 cores, and a memory limit that
leaves 1 GiB beside the 3.2 GiB that the agents of Palette take in it.

## app-profile.sh

Read-write: it makes app profiles, updates one, and deletes them. It needs no
cluster, and costs nothing.

| Check | Expected exit | What it shows |
|---|---|---|
| `create` | 0 | an app profile with one manifest tier, made through the API |
| `record`, `record-type` | 0 | #78: a record of `appprofile:<name>@<version>` is of type `app`, counts no consumer, and holds the tier as the pack `app/config` with its manifest |
| `record-no-digest` | 0 | #78: a tier has no digest: `upstreamSource` is `none` |
| `record-by-uid` | 0 | #78: `appprofile:<uid>` gives the same root |
| `record-unknown` | 2 | #78: a name and version that no app profile has is `app-profile-not-found` |
| `verify` | 0 | #78: `verify` of the record |
| `edit`, `edit-same-version` | 0 | #78: a plain update of the manifest keeps the uid and the version, and makes no new version |
| `edit-verify`, `edit-verify-says` | 1, 0 | #78: `verify` of the record reports drift, with the tier `AUTHORED` and the manifest named |
| `edit-who`, `edit-who-appprofile` | 1, 0 | #140: `verify --who` reads the log of the kind `appprofile`, and its entries name the app profile as their object. |
| `edit-last-modified` | 0 | #140: the document of the app profile gives the time of the update, after the record. |
| `order-a-create`, `order-b-create` | 0 | #112: two app profiles with the same two tiers, made with the install orders 2 and 5, and 5 and 2 |
| `order-a-tiers` | 0 | #112: the tiers route gives the install orders of the create |
| `order-a-record`, `order-a-params`, `order-b-record`, `order-b-params` | 0 | #112: each record holds the orders in `packs[].params.installOrder`, and lists the tiers by them: `tier1` then `tier2` in the first, `tier2` then `tier1` in the second |
| `order-compare`, `order-compare-says` | 1, 0 | #112: `compare` of the two records exits `1` with two roots, and names `~ param installOrder: 2 -> 5` under `tier1` and `5 -> 2` under `tier2` |
| `order-update`, `order-update-verify` | 1, 0 | #112: an update of the order of `tier1` to 7 through the tier route gets a 2xx answer, and the tier keeps the order 2. So the record still verifies |
| `order-a-delete`, `order-b-delete` | 0 | the two app profiles are deleted |
| `delete` | 0 | the app profile of stage 1 is deleted |

No run updates the properties of a tier: the tier of the run is a manifest tier,
which has none.

An update of a tier cannot change its install order. Palette answers `PUT
/v1/appProfiles/<uid>/tiers/<tier uid>` with `installOrder` with 2xx, and the
tier keeps its order. An app profile that is made with an install order on each
tier keeps the orders, and the tiers route gives them. The document of the app
profile gives no order. So `api.sh app-profile-order` reads the tier again, and
exits `1` when the order did not change.

## loop.sh

The everyday loop, end to end: [The everyday loop](../loop.md) tells it. The
records are in a git repository that the script builds, and every script of CI
runs in it. It changes cluster A and the add-on version through the API, and
undoes each change. It needs the example at its baseline with cluster A.

| Check | Expected exit | What it shows |
|---|---|---|
| `record-release`, `record-addon`, `record-cluster`, `commit-main` | 0 | main records the release version, the add-on version, and cluster A |
| `cluster-runs-release` | 0 | #81: cluster A runs the recorded release |
| `candidate`, `candidate-record` | 0 | #81: the change: the next version as a candidate file, and as a record before Palette has it |
| `comment`, `comment-says` | 0 | #81: `pr-comment.sh` writes the comment of the pull request, with the change and its impact |
| `commit-branch`, `merge-gate` | 0 | #81: the merge gate is clear: the records after the merge verify |
| `record-next`, `same-release`, `commit-publish` | 0 | #81: Palette holds the content that the reviewer read, by the root |
| `nightly` | 0 | #81: `verify-report.sh` finds every record unchanged |
| `fleet`, `fleet-has-a` | 0 | #81: the fleet of a label holds cluster A |
| `upstream` | 1 | #81: `upstream-report.sh` finds pins that the registry has passed |
| `override`, `override-record`, `override-not-behind` | 0 | #81: a value overridden on cluster A opens no notification: Palette says that no update waits |
| `override-drift` | 1 | #81: the record of cluster A reports the drift |
| `override-release` | 0 | #81: the records of the versions verify: the profiles did not change |
| `override-undo`, `override-gone` | 0 | the override is undone, and the record of cluster A verifies again |
| `edit` | 0 | #81: a person edits the add-on version in place, through the API |
| `nightly-drift`, `nightly-names` | 1, 0 | #81: the nightly job opens an issue for the add-on record |
| `who`, `who-says` | 1, 0 | #81: `--who` names a person who wrote the edit |
| `cluster-kept`, `behind-record`, `behind` | 0 | #81: cluster A keeps its copy, and Palette says that an update waits for it |
| `edit-undo`, `edit-gone` | 0 | the edit is undone, and the add-on record verifies again |
| `incident-list`, `incident-list-has` | 0 | #81: `list --pack` finds the release among the versions that carry the CSI pack |
| `incident-consumers`, `incident-compare` | 1, 0 | #81: cluster A runs the release, and exactly its content |
| `retire` | 0 | #81: nothing runs the next version: the retire gate is open |
| `baseline` | 0 | the example is at its baseline |

## version-states.sh

The other answer to a version bump that overwrites a version: each version in a
state of its own, a workspace of `terraform-version/`, with no feature preview
flag. It builds its own profile, attaches it to cluster A for a while, and
removes it. It needs the example with cluster A.

| Check | Expected exit | What it shows |
|---|---|---|
| `init`, `apply-1`, `record-1` | 0 | version 1.0.0 in its own state, and a record of it |
| `apply-2`, `new-uid` | 0 | #81: version 1.0.1 in a second state has a uid of its own |
| `old-verifies`, `both-listed` | 0 | #81: the record of 1.0.0 verifies, and Palette lists both versions: the bump did not touch 1.0.0 |
| `plan-same` | 0 | #81: the state of 1.0.0 plans no change |
| `plan-edit`, `plan-edit-in-place` | 2, 0 | #81: a change of the content of 1.0.0 at the same version plans an update in place of the published version |
| `attach`, `in-use` | 0, 1 | cluster A runs 1.0.0 too |
| `destroy-refused`, `destroy-refused-says` | 1, 0 | #81: a destroy of the state of 1.0.0 fails while a cluster uses the version, and the refusal says that it is in use |
| `still-there` | 0 | #81: the version is still there, and its record verifies |
| `detach`, `free` | 0 | cluster A lets 1.0.0 go |
| `destroy-1`, `old-gone`, `old-gone-says` | 0, 1, 0 | #81: the destroy of the state removes the version, and `verify` of its record reports that the version no longer exists |
| `destroy-2`, `none-left` | 0 | no version of the name is left |
| `apply-short`, `stored-short`, `short-as-given`, `replan-short`, `destroy-short` | 0 | #81: Palette stores the version `1.2` as given, and not as `1.2.0`, and the provider plans no change after it |
| `apply-prefixed`, `stored-prefixed`, `prefixed-as-given`, `destroy-prefixed` | 0 | #81: Palette stores the version `v1.2.3` as given, and not as `1.2.3` |
| `baseline` | 0 | the example is at its baseline |

## eks.sh

The claims that depend on the engine, on a managed engine: EKS (#89). It needs
the example with `cloud = "eks"` and cluster A. See
[An EKS cluster](./live-example.md#an-eks-cluster).

| Check | Expected exit | What it shows |
|---|---|---|
| `release`, `release-cloud`, `release-layers` | 0 | #89: a record of the EKS release version, of cloud type `eks`, with an OS, a Kubernetes, a CNI, and a CSI layer |
| `release-packs`, `release-digests` | 0 | #89: `amazon-linux-eks`, `kubernetes-eks`, `cni-aws-vpc-eks`, and `csi-aws-ebs`, each with a pack-ref digest |
| `cluster`, `cluster-runs`, `verify-both`, `consumers` | 0, 0, 0, 1 | #89: a record of the EKS cluster runs the recorded release, both records verify, and `consumers` names the cluster |
| `shape`, `shape-pool`, `shape-fields` | 0 | #89: the shape tier holds the managed node group, with its instance type, size, zones, AMI type, and update strategy |
| `instances` | 0 | #89: the live helper lists the nodes of the node group |
| `value-k8s`, `value-csi` | 0 | #89: a new value in the `kubernetes-eks` pack or the CSI pack: Palette says no repave. The OS and CNI packs of EKS have no values |
| `k8s-bump`, `k8s-bump-lines`, `k8s-bump-repave` | 1, 0, 0 | #89: a new Kubernetes version: Palette says repave, `PackUpgraded kubernetes-eks`, and the table agrees |
| `pool-change`, `pool-machines` | 0 | #89: another instance type for the node group, through OpenTofu, is an update in place of the cluster, and nodes of the new type run it. The cluster stays `Running` |
| `nodegroups-change` | 0 | #89: EKS holds two node groups: Palette made one of the new type and kept the old one, with its nodes |
| `nodegroups-back` | 0 | #89: the change back leaves both groups |
| `shape-2`, `shape-differs`, `shape-differs-says` | 0 | #89: compare of the records before and after names the change of the shape tier, `shape.pools[name=worker-pool].instanceType`. A tier does not change the exit |
| `pool-back`, `baseline` | 0 | the node group back, and the example at its baseline |

The old node group is one that Palette no longer tracks. Its nodes keep
running, and a delete of the cluster removes the new group alone: the EKS
control plane cannot go while the old group exists, and the delete waits at
`DeletingInfrastructure`. A delete of the old group in AWS does not end the
wait: its object in Palette keeps its finalizer. So step 4 runs only with
`EKS_POOL_CHANGE=1`, and a cluster after it needs the recovery of
[An EKS cluster](./live-example.md#an-eks-cluster).

Provider 0.30.1 does not read `cloud_config.azs` of an EKS cluster back, and
`azs` forces a new resource. So each plan after the create replaces the
cluster. `cloud-eks.tf` ignores the field.

## gke.sh

`gke.sh` asks the claims that depend on the engine on a GKE cluster (#90).
Google runs the control plane, and one node pool of three `e2-standard-4` nodes
runs the workers. It needs `cloud = "gke"`, cluster A, and `gke_k8s_bump` for
the version bump: see [A GKE cluster](./live-example.md#a-gke-cluster).
`verify.sh`, the read-only suite, passes each of its checks on GKE.

| Check | Expected exit | What it shows |
| --- | --- | --- |
| `release`, `release-cloud`, `release-layers` | 0 | #90: a record of the GKE release version, of cloud type `gke`, with an OS, a Kubernetes, a CNI, and a CSI layer: `cos-gke`, `kubernetes-gke`, `cni-vpc-native-gke`, and `gke-csi-gcp-driver` |
| `release-packs`, `release-digests` | 0 | #90: each GKE pack has an upstream digest, `packRef.digest` |
| `cluster`, `cluster-runs`, `verify-both` | 0 | #90: a record of cluster A verifies, and `compare` of it with the release is `IDENTICAL` |
| `consumers` | 1 | #90: `consumers` of the release names cluster A |
| `shape`, `shape-pool`, `shape-fields` | 0 | #90: the shape tier holds the node pool, with its machine type, size, disk, labels, and update strategy. A GKE pool has no zones |
| `shape-control-plane` | 0 | #90: the shape tier holds the managed cluster settings and the cluster API override, with the release channel |
| `value-os`, `value-k8s`, `value-cni`, `value-csi` | seen | Each of the four GKE packs has values of comments alone, so no change of values has a Palette answer on GKE |
| `k8s-bump`, `k8s-bump-lines`, `k8s-bump-repave` | 1, 0, 0 | #90: for `kubernetes-gke` 1.34 to 1.35, Palette says repave, `PackUpgraded kubernetes-gke`, and the level of the impact table is `repave`: they agree |

With `GKE_POOL_CHANGE=1`, step 4 changes the machine type of the node pool
through OpenTofu, from `e2-standard-4` to `e2-standard-8`:

| Check | Expected exit | What it shows |
| --- | --- | --- |
| `nodepools-before`, `pool-change` | 0 | the GKE node pool before the change, and the apply of the change |
| `nodepools-gone` | 0 | #90: within 5 minutes of the change, GKE has no node pool |
| `shape-2`, `shape-differs`, `shape-differs-says` | 0 | #90: `compare` of the two records names `~ shape.pools[name=worker-pool].instanceType: "e2-standard-4" -> "e2-standard-8"`, and the shape tier is `CHANGED`; the root stays `IDENTICAL` |
| `nodepools-none-made` | 0 | #90: 10 minutes later, GKE still has no node pool: nothing makes the new one |

Palette makes a new machine pool for the new machine type, and gives it the
GKE node pool name of the old one, `worker-pool`. The delete of the old machine
pool deletes the GKE node pool. The controllers that make the new pool ran on
its nodes, so the cluster keeps no node. The delete of the cluster then removes
the GKE cluster, and stays `Deleting`: the machine pool of GKE that has no
machine pool of the cluster API keeps its finalizer, so the delete of the
cluster does not end by itself. The EKS change of [eks.sh](#ekssh) is of the
same kind. A change of the node pool is not a change of a layer, so the impact table
gives it no level, and `compare` shows it in the shape tier alone. The recovery
is in [A GKE cluster](./live-example.md#a-gke-cluster).

## preflight.sh

The checks of the lab itself, before a recipe builds: see
[The preflight checks](./live-example.md#the-preflight-checks). Each check is
one row of the catalog with `engines: [lab]`.

| Check | Expected | What it shows |
|---|---|---|
| `tools-present` | 0 | each tool of the lab is on the PATH |
| `palette-key`, `palette-preflight` | 0 | the key reads the tenant, and `strata preflight` lists the rules of the cloud |
| `cloud-cli-auth` | 0 | `aws sts get-caller-identity`, or `gcloud auth print-access-token` |
| `gcp-apis-off`, `gke-api-off` | 0 | the Compute, Cloud Resource Manager, IAM, and Kubernetes Engine APIs are on (#90) |
| `eks-core-policies` | 0 | the six core policies of Palette are in the AWS account (#89) |
| `ecr-token` | ? | `crane` reads the ECR repository of the example: `0` is a login of the last 12 hours, which `verify.sh` needs and a build does not |
| `leftovers-a`, `leftovers-b` | 0 | the inventory of each cluster name is 0 on every line |

## preflight-plan.sh

The test plan of #91 for one engine, with no cluster: each fault the lab can make
fails when it must, and the clean entity passes. Each run appends to the ledger,
`preflight/evidence/runs.yaml`, and `strata preflight --promotion` reads it. The
checks of each run, and what each one shows, are in `summary.md` of the run;
this table names the faults.

| Engine | The faults made on purpose, by rule id |
|---|---|
| `gke` | `gke-pool-multiple-3`, `palette-validate` (a pool of 4), `gke-no-release-channel`, `gke-no-gpu`, `gke-autopilot`, `gcp-name-length`, `account-missing` |
| `eks` | `eks-one-zone`, `eks-worker-needed`, `update-strategy-eks`, `api-endpoint-reach`, `account-missing` |
| `aws` | `worker-cpu-floor`, `palette-validate` (one worker of four cores), `api-endpoint-reach`, `account-missing` |
| `gcp` | `worker-cpu-floor`, `palette-validate`, `gcp-name-length`, `account-missing` |

Part 3 of each run also runs the custom probes of the lab,
`examples/live/preflight/lab-probes.yaml` (`lab-region`, `lab-name`), beside
the catalog, with `--probes`: a custom probe on a real tenant. With `--build`,
the run adds the build of cluster A through the provider with the watch on it
from its first listing, `strata preflight cluster:UID` on the cluster as
Palette holds it, and a change of the instance type of its worker pool, which
`gke-type-change`, `eks-type-change-orphan`, or `pool-replace-fields` must fail;
`--down` tears the example down and reads the inventory.

The runs of the plan, and what each one showed, are in the section
[What the preflight runs show](#what-the-preflight-runs-show) below.

## watch.sh

`just live-watch --build --down` on aws: cluster B built beside cluster A,
with the watch on it from the moment Palette listed it, on the terminal. 43
readings, `Running` after 1335 seconds, each watch rule passed at the end. At
662 seconds `webhook-call-failed` warned: a `Reconciler error` of the
controller in the new cluster, "failed to set controlPlane type annotation",
the call of `mspectrocluster.kb.io` refused right after the pivot, as on gcp;
the warning held for 20 readings while the event stayed in the 50 newest, and
the build went on to `Running`, so the watch did not stop. The removal of
cluster B did not run: the script was edited during the run, and bash reads a
script as it runs, so the process resumed in the changed file and ended. The
script is one function since, which bash parses before it runs, and the
cluster was removed by hand with `tofu apply -var second_cluster=false`.

## preflight-walkthrough.md

The story of [Preflight: one cluster, end to end](./preflight-walkthrough.md), run
once as written, on aws. The clean entity of the last plan run passed every
tier in 31 seconds, exit 0; the copy with one worker of four cores failed
`worker-cpu-floor` and `palette-validate` in 33 seconds, exit 1 under
`--strict`. `POST /v1/spectroclusters/aws` with the same body answered the uid
in 4 seconds. The watch started at once: 25 readings, `Running` after 764
seconds, each watch rule passed, and no warning this time. On the cluster as
Palette holds it, two notes, `bastion-cost` and `variables-in-use`; on the
change of the worker instance type to `m5.xlarge`, `pool-replace-fields`
warned that the machines roll. The delete was accepted at once; ten minutes
later the events named the security groups and the VPC as deleted, and the
inventory of AWS read 0 on all 12 lines.

## What the preflight runs show

The runs of `preflight-plan.sh` on the tenant of the live example, with the
profiles of each engine and no cluster. Each run is an entry of the ledger.

**aws.** Four faults made on purpose, four caught. `POST /v1/spectroclusters/aws/validate`
refuses one worker of four cores with `MachinePoolWithMinCpuRequired`: "A
minimum allocation of '4.1' CPU cores is required across all worker nodes.
Current allocation : '4' CPU cores", in under 4 seconds; through the provider
the same fault comes after a full apply. A cloud account uid that is not in the
project is a 404 of `GET /v1/cloudaccounts/aws/{uid}`, and an internal control
plane load balancer is a static failure. On the clean entity, 37 checks ran and
36 passed; the exec rules ran against AWS with the profile of the lab
(`aws-quotas`, `aws-ami`, `capa-bootstrap-stack`, `ebs-csi-node-policy`,
`iam-ten-policies`, `aws-region-not-enabled`, `leftovers-before-build`). The one
failure was a wrong result of `role-partial-permissions`:
`aws iam simulate-principal-policy` with `iam:PassRole` and no resource ARN
gives `implicitDeny` for a role whose policy allows it on the CAPA roles alone,
and that role has built clusters. The rule now leaves `iam:PassRole` to a
reproduce line with the three CAPA role ARNs. The run is in the ledger as the
first wrong result of a check.

**eks.** Six faults made on purpose, five caught: one zone, no worker pool, an
`updateStrategy` that EKS does not read, a private endpoint alone, and a cloud
account uid that is not in the project. The sixth, one worker of four cores,
was not caught: `POST /v1/spectroclusters/eks/validate` answers `200` with no
error for it, where the AWS route answers `MachinePoolWithMinCpuRequired` for
the same pool. No run shows whether an EKS create with one worker of four
cores fails later, so the CPU floor on EKS is a reading and not a fact. On the
clean entity, 44 checks ran and 42 passed. The two wrong results were the two
rules that simulate the IAM policy of the role of the cloud account:
`aws iam simulate-principal-policy` with no `--resource-arns` evaluates each
action against the resource `*`, and a statement that names a resource, as the
EKS policy of Palette does for `eks:*`, does not match it. The role built the
EKS clusters of #89. Both rules are warnings now: one simulates the actions
that name a resource, with the cluster, the node group, and the CAPA role ARNs
of the build; the other simulates the actions on `*` alone, and gives the
PassRole line with the CAPA roles as a command to run by hand. A simulation
with no request context cannot prove a build, so neither is a `fail`.

**gke.** Seven faults made on purpose, seven caught. `POST /v1/spectroclusters/gke/validate`
refuses a pool of 4 with `InvalidParam`: "Invalid parameter 'Node pool size';
caused by: Node pool size should be multiple of 3", in under 200 ms; through the
provider, #90 saw the same refusal after a full apply. A cluster with no release
channel, a GPU machine type, Autopilot with a pool, a name over 40 characters,
and a cloud account uid that is not in the project are static failures or a
404, in milliseconds. On the clean entity, 34 checks ran and 32 passed. The two
failures say something about the GCP project of the lab, and not about the
build: the service account holds no `roles/storage.objectViewer` at the project
level, and the Cloud Resource Manager API is off, and the account built the
GKE clusters of #90 and the GCP clusters of #74 all the same. So a documented
role or API that a check does not find is a warning to read, and both rules
are `warn`. The plan does not build, so a rule from the source stays a probe
after these runs: `strata preflight --promotion` promotes a static rule only
when a live run shows the fault (`status: seen`), such as `gke-pool-multiple-3`
and `gke-no-release-channel` from #90.

**gcp.** Four faults made on purpose, four caught. The GCP validate route
applies the CPU floor as the AWS one does: one worker of four cores is
`MachinePoolWithMinCpuRequired` in under a second. A name over 52 characters
gives a derived network name over 63, which the static rule computes as Palette
derives it (`<name>-network`, `<name>-network-<region>`). On the clean entity,
29 checks ran and 27 passed; the two failures are the two findings about the
GCP project of the gke run, which are `warn` now.

Over the four engines, the plan made 21 faults and the checks caught 20; the
one not caught is the CPU floor on EKS, which the EKS validate route does not
report, so `worker-cpu-floor` no longer names `eks` and the plan no longer
makes that fault there. Of the 144 checks that ran on the clean entities, 137
passed; the seven failures were three wrong results of two IAM simulations and
the two GCP project findings, twice. After the fixes, the plan ran again on gcp
and on eks: every fault was caught, every rule of severity `fail` passed on
the clean entities, and `strata preflight --promotion` exits `0` with the
ledger of the six runs. The IAM simulation on eks warns that
`eks:CreateCluster` is `implicitDeny` on each resource of the build, for the
role that built the #89 clusters: the warning to read that the rule now says
it is.

**A custom probe on the tenant.** Each run since the build phase also runs the
two custom probes of the lab, `lab-region` and `lab-name`, from
`examples/live/preflight/lab-probes.yaml`, beside the catalog. Both ran on the
clean entity of the gke run and passed: a file of a company runs the same way,
with no change to strata.

**gke, with a build.** The plan built cluster A through the provider, with
the watch on it from the moment Palette listed it: 39 readings, `Running`
after 1190 seconds, and each watch rule passed (`agent-egress`,
`node-egress-images`, `status-not-cloud`, `pivot-webhook-timeout`). On the
cluster as Palette holds it, 38 checks ran and 34 passed. The three warnings
are the two GCP project findings and `position-zero`: the provider put the
add-on profile at position 0 of the cluster, as the order report says it does.
The one failure was a wrong result of strata: the entity built from the cloud
config carried `skipK8sUpgrade: "disabled"`, which Palette stores on each pool,
and the GKE validate route refuses the field with `InvalidParam`; the entity
now carries it when it is enabled alone. The change of the instance type of the
worker pool, `e2-standard-4` to `e2-standard-8`, was caught by
`gke-type-change` and `pool-replace-fields` before any apply: the fault of
#90, found in a millisecond. The destroy of the cluster ended with
`context deadline exceeded` at the deadline of the provider, and a second
destroy completed, as `provider-delete-timeout` says. The inventory step of the
run could not ask GCP: `gcloud` demanded a reauthentication in the middle of
the campaign, which a non-interactive shell cannot give, the `cloud-cli-auth`
row of the lab. Run by hand after, the inventory was 0 on every line, the GKE
cluster counted.

**eks, with a build.** The watch stopped at 1355 seconds, after 44 readings,
on `pivot-webhook-timeout`, as the rule was then: an event named a failed call
of the webhook `mawscloudconfig.kb.io` while the controller set the owner of
the cloud config, right after the pivot, with the cluster `Provisioning` and no
error in its status. The message of the rule cut the event at 240 characters,
before the cause, and the `world.json` of the watch held no reading, so whether
the call timed out or was refused is not known; both are fixed below. The provider then ended its
apply with exit 0 while the cluster was still `Provisioning`. On the cluster as
Palette holds it, 46 checks ran and 42 passed; `delete-while-provisioning`
failed, because the cluster was not `Running`, and said what a delete would do.
The plan then deleted the cluster, and the delete waited, as the rule said:
the first destroy ended at the deadline of the provider with the cluster
`Deleting` and the EKS cluster, the VPC, and three NAT gateways still in AWS.
The change of the instance type of the worker pool, `t3.xlarge` to
`m5.xlarge`, was caught by `eks-type-change-orphan` and `pool-replace-fields`
before any apply: the fault of #89, found in a millisecond. Two wrong results of
strata came out of this run and are fixed: the validate route refuses the name
of an existing cluster with `ResourceAlreadyExists`, so `palette-validate` and
`worker-cpu-floor` now check an existing cluster under a suffixed name; and the
plan now waits for `Running` before it deletes. The delete did not end by itself: after
75 minutes the EKS control plane, its node group, the VPC, and three NAT
gateways were still in AWS, with the cluster `Deleting` and
`CloudInfrastructureCleanedUp: DeletingInfrastructure`. The recovery was the
one of the book: `eks-nodegroups.sh --delete`, `aws-cleanup.sh fred us-east-2 --yes`,
the inventory at 0, then a second plain delete, which the findings report on
the pivot webhook gives as the step that ends the wait. It did
not end it here: from then on each reconcile of the delete failed with "failed
to delete additional resources: failed to get existing aws managed control
plane: AWSManagedControlPlane fred-cp not found", every five minutes, with the
cloud empty. In this run the delete path read a control plane object that was
gone as an error, and not as done, and the delete did not end. A force delete
of the record ended it. The mechanism is the one of that report, on EKS this
time, with this one more step, which a second plain delete did not end. One
hour of the recovery
went to a fault of the operator: `tofu output -raw` printed a warning beside
the project uid, a shell exported both as `PALETTE_PROJECT_UID`, and every
request with that header answered a plain-text `400 Bad Request`, which read
as Palette refusing the cluster. It was not: with the uid alone, the API
answered as it does. `lib.sh` reads the outputs as JSON, with the warnings
dropped, so the scripts of the lab do not meet it; a shell that copies a
value by hand can.

**aws, with a build.** The plan built cluster A through the provider, with the
watch on it from the moment Palette listed it: 41 readings, `Running` after
1278 seconds, and each watch rule passed (`agent-egress`,
`csi-crash-after-running`, `node-egress-images`, `infrastructure-stalled`,
`pivot-webhook-timeout`, `stale-worker-condition`). Before the build, the four
faults made on purpose were caught: one worker of four cores by
`worker-cpu-floor` and `palette-validate`, an internal control plane load
balancer by `api-endpoint-reach` in a millisecond, and a cloud account uid that
is not in the project by `account-missing`. On the clean entity and on the
cluster as Palette holds it, 40 checks ran and 40 passed, with two notes:
`bastion-cost`, and `variables-in-use` once the profile had a cluster. The
change of the instance type of the worker pool, `t3.xlarge` to `m5.xlarge`, was
caught by `pool-replace-fields` before any apply. The teardown ran after
`Running`: one destroy, 14 resources, and the inventory of AWS at 0 on every
line.

**gcp, with a build.** The four faults were caught before the build, the
derived network name of a long cluster name among them (`gcp-name-length`, in
a millisecond). The watch stopped at 671 seconds, after 22 readings, on
`pivot-webhook-timeout` as the rule was then: right after the pivot, a
`ReconcileError` of the controller named a failed call of
`mspectrocluster.kb.io` while it set the type of the control plane, and a
`Log` event of the agent named a failed call of `vspectrocluster.kb.io` while
it updated the cluster, eight seconds apart. Palette made the calls again: the
wait of the plan saw `Running` 23 minutes after the watch started, with no
other event of note, and the teardown ran after it: one destroy, 14 resources,
and the inventory of GCP at 0 on every line. On the clean entity, 33 checks ran
and 31 passed; on the cluster as Palette holds it, 34 ran and 32 passed. The
two warnings are the two GCP project findings of the gke run. The change of the
instance type of the worker pool, `e2-standard-4` to `e2-standard-8`, was
caught by `pool-replace-fields` before any apply.

**Three wrong results of strata came out of the gcp run, and are fixed.**
`pivot-webhook-timeout` failed on one minute of failed webhook calls, which the
build survived. The rule now fails on the failed move of the cluster, on the
cloud config the controller cannot read, or on 10 minutes or more of failed
calls while the cluster is not `Running`; the new warning `webhook-call-failed`
names each failed call. Both show the start and the end of each event, because
the end names the cause, a timeout or a connection refused, and the old message
cut it. The `world.json` of a watch held the world before the first reading,
with no conditions and no events, so the full events of the eks and the gcp
watches are lost; it now holds the last reading. And the watch ended at the
first failure of any rule; it now ends at the first counted failure alone, as
the exit code counts them, and a failed probe or a warning is in the report
while the watch reads on. The ledger gives the gcp run as `passed` for the
narrowed rule, with its events in the fixture `watch-webhook-refused` that the
tests of the rule replay, and as `caught` for the warning on gcp and on eks;
the eks entry of the old rule is gone, because the cluster was deleted before
the outcome was known. The findings report on the build runs,
`palette-preflight-build-runs-findings-2026-09-29`, holds the timeline of the
eks delete and the source of each finding.

## A chart pushed again, on a cluster

A run by hand, with the registry host of `registry-kinds.sh` and cluster A: no
script holds it yet. Two charts, one in the Helm repository and one in the OCI
registry with the provider type `helm`, each render one ConfigMap. Two profiles
made with `api.sh pin-profile` pin them, and cluster A runs both through
`extra_addon_profile_uids`. Then each chart is pushed again under `1.0.0`, with a
template that adds `pushed: "again"`, and each registry is synced.

| Step | What cluster A runs | What Palette and strata say |
|---|---|---|
| The profiles attached | the first bytes | the digest of the first push, in the listing, the profiles, and the copy on the cluster |
| After the push again, for 10 minutes | the first bytes | the same digests; a record of each profile from before verifies |
| The profiles taken off, and attached again | the second bytes | the same digests; the records from before still verify |

The log of the controller on the cluster shows a `helm pull` of each chart, at
each install, by `chartVersion` `1.0.0`. See
[What a cluster installs of a Helm pack](../internals/palette/packs.md#what-a-cluster-installs-of-a-helm-pack).

## The sync of each registry

A read by hand, with no write: `status.sync` of each registry of the tenant.
It comes from `GET /v1/registries/pack/summary`,
`GET /v1/registries/helm/summary`, and `GET /v1/registries/oci/summary`. It
also comes from `GET /v1/registries/{kind}/{uid}/sync/status` for each pack and
Helm registry.
`packs.sh` reads the Helm summary listing in its check `bitnami-sync`, and no
script holds the other reads.

- The two routes give the same fields and the same values for each registry.
- Two or three registries give the same `lastSyncedTime` or `lastRunTime` to
  the millisecond: the time is the start of the sync job, not its end.
- The Bitnami registry is `InProgress`, with `syncRecovery` `Recovering` and
  `sync interrupted`. Its `lastSyncedTime` is more than a month older than its
  `lastRunTime`.
- A registry whose sync failed gives `Failed` and the cause in `message`.

See [The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry).

## A second cloud: GCP

The scripts run on GCP with `cloud = "gcp"`, and with no other change. The
profiles hold `ubuntu-gcp` 24.04, Kubernetes 1.35.6, `cni-cilium-oss` 1.20.1,
and `csi-gcp-driver` 1.26.4, and cluster A has three `e2-standard-4` nodes.

| Script | Checks passed | Failed |
|---|---|---|
| `verify.sh` | 34 | 0 |
| `in-place.sh off` | 51 | 0 |
| `in-place.sh on` | 53 | 0 |
| `registry-move.sh` | 16 | 0 |
| `packs.sh` | 27 | 0 |
| `knurl.sh` | 26 | 0 |
| `repave.sh` | 23 | 0 |
| `pending.sh` | 29 | 0 |
| `tag-only.sh` | 32 | 0 |

Each check gives the answer that it gives on AWS: strata reads the same
documents, and Palette answers the same way. `tag-only.sh` moves cluster A to a
version with another tag name and back, and Palette keeps each machine, as on
AWS. `repave.sh` gets `repave` for one more kubeadm command and `no repave` for
a manifest of the add-on, as on AWS: the two clouds use one upgrade engine.

What differs is outside strata:

- **The pack names and versions.** The OS and CSI packs are the packs of the
  cloud. The CSI values write the reclaim line as `reclaimPolicy: "Delete"`,
  and not `reclaimPolicy: Delete`: the edit of each script comes from the output
  `csi_reclaim_delete`.
- **The Kubernetes version.** The Kubernetes pack lists `gcp`, and no GCP image
  is shared for Kubernetes 1.36.2 with Ubuntu 24.04: GCP answers
  `Required 'compute.images.useReadOnly' permission` for the image in the project
  `spectro-image-build`, and no machine starts. The image of 1.35.6 is shared.
- **The service account of the nodes.** The nodes run as the Compute Engine
  default service account of the project. In an organization that gives that
  account no role, the GCE cloud controller exits, the node keeps the taint
  `node.cloudprovider.kubernetes.io/uninitialized`, and the cluster stays
  `Provisioning`. `roles/compute.admin` on the project lets it continue.

## What has no live run

- `in-place.sh`, `registry-move.sh`, `registry.sh`, `template.sh`,
  `cluster.sh`, and `tag-only.sh` have run under OpenTofu, with the same
  provider that Terraform used, and give the same answers. `drift-report.sh` has
  run under Terraform only. `order.sh` reads the OpenTofu outputs and applies
  nothing.
- On GCP, the scripts in [A second cloud: GCP](#a-second-cloud-gcp) have run.
  `drift-report.sh`, `merge-gate.sh`, `pack-states.sh`, `registry.sh`,
  `template.sh`, `cluster.sh`, `order.sh`, `who.sh`, `macros.sh`, and
  `macro-repave.sh` have run on AWS only. No live run uses an Edge cluster, the
  other upgrade engine.
- [What Palette does](../internals/palette/limits.md#what-no-run-covers) lists the
  behaviors that no script here establishes.
- No script creates a profile version that breaks a rule of
  [The gaps that strata fills](../internals/palette/profiles.md#the-gaps-that-strata-fills).
  Each rule states what Palette does, and no live run shows it. The test cases
  of the version check come from the version pattern that Palette uses.
- No script uses a Palette that runs in FIPS mode. The live tenant has
  `fipsPackConfig.mode` `none`, and no listed version has the annotation
  `fips`. A read of the live tenant shows only that `packs` gives the same
  versions, tags, and states as before. The read is of `csi-aws-ebs`,
  `kubernetes`, `nginx`, `ubuntu-aws`, and `cni-calico`: see
  [The FIPS rule](../internals/palette/packs.md#the-fips-rule).
- No script installs a Zarf package on a cluster: Palette gives a Zarf pack the
  values of an app profile, `{{.spectro.system.appdeployment.tiername}}`, so it
  needs an app deployment of Palette Dev Engine.
