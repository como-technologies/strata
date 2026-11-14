# What the live example verifies

[The live example](./live-example.md) builds a real tenant and asserts what
Palette does with it. This page lists what each script sets up and what each of
its checks establishes, so you can see the coverage without running anything.

`verify.sh` is the read-only script over the cluster, and
[its checks are on the live example page](./live-example.md#the-checks-of-verifysh).
`packs.sh`, `knurl.sh`, `observer-kind.sh`, and `observer-role.sh` read too. The other scripts below write to the tenant;
`pack-states.sh` keeps nothing that it writes. `validate-packs.sh` writes only
in stage 4, which deletes the profile that it makes, and in stage 7, whose updates Palette
refuses. `route-bodies.sh` deletes each
profile that it makes.

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
| `verify-a3-who`, `a3-who-names-profile` | 1, 0 | 0, 1 | #140: `verify --who` of the same record reads the log of the overwritten profile. A write names the add-on profile as its object. Both happen only without the flag. |
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
job makes a checkout of the caller and of this repository at the release tag.
It installs the binary of that release with `install-strata.sh`, which checks
the signature of `SHA256SUMS` and then the binary. Then it runs the script.

At the baseline it succeeds and opens nothing. After an edit in place it opens an
issue with the report and fails with exit 1. After the undo it closes the issue
and succeeds.

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
| `compare-images`, `compare-images-says` | 0 | #77: the diff flags `IMAGES` and `CHARTS` beside `AUTHORED`, and names each image that moved. The move is from `csi-aws-ebs` 1.63.1 to 1.65.0. Eight images are retagged, and the chart `aws-ebs-csi-driver` is at 2.65.0 |
| `compare-same` | 0 | #55: a pack version against itself is identical |
| `record`, `record-title` | 0 | #55: `snapshot` writes a record of a pack version, which names the pack and not a profile |
| `verify-record` | 0 | #55: the record verifies against the registry it came from |
| `verify-sync`, `verify-sync-json` | 0 | #141, #192: the verdict ends with the last sync of the registry. That is the start of the last complete sync, a sync in progress, or a failure. `--json` gives `verify.registrySync` |
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
| `packs-palette-latest` | 0 | #124, #489: the listing marks one version of `kubernetes` and one of `cert-manager` in the registry as the latest of Palette. That version is the one that strata calls the newest, with `latest`. In `Public Repo`, it is `1.37.0` of `kubernetes`, which has 137 versions, and `1.21.2` of `cert-manager`, which has 9. `cert-manager` has the rebuilds `1.19.3-rev1` and `1.19.1-rev1` below their base. See [The version that Palette calls the latest](../internals/palette/packs.md#the-version-that-palette-calls-the-latest) |
| `packs-rev-floating-tag` | 0 | #124: the floating tag `1.19.x` of `cert-manager` names `1.19.3`, and not the rebuild `1.19.3-rev1` |
| `packs-unknown-registry`, `packs-unknown-pack` | 2 | #54: an unknown registry lists the visible ones; a pack no registry has is an error |
| `preset-listing` | 0 | #151: the listing gives each option of the `access` parameter of `headlamp` 0.45.0 with no presets: see [The presets of a template option](../internals/palette/packs.md#the-presets-of-a-template-option) |
| `preset-warning`, `preset-warning-says`, `preset-recorded` | 0 | #151: a candidate whose values select the option `public` warns, and records `packs[].presetOptions[]` |
| `os-record` | 0 | #189: a record of the release profile |
| `os-annotations` | 0 | #253: the record keeps no annotation of the OS pack, because `ubuntu-aws` has no `imageId` |
| `os-profile`, `os-annotations-ref` | 0 | #189: the document of the release profile gives the pack reference of the OS pack eight annotations, and no `imageId` |
| `os-pack-registry`, `os-annotations-registry` | 0 | #189: the pack version in the registry, `GET /v1/packs/<uid>`, gives the same annotations as the pack reference: see [The annotations of a pack reference](../internals/palette/packs.md#the-annotations-of-a-pack-reference) |
| `pack-time-listing` | 0 | #191: the listing gives each version of the CSI pack a `lastModifiedTimestamp` that equals its `creationTimestamp`. See [What a registry answers about a pack](../internals/palette/packs.md#what-a-registry-answers-about-a-pack) |
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
public registries, one for each state. They are `nginx` 1.15.1 (disabled) and
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
| `two-archives`, `two-archives-says` | 2, 0 | #55: a pack that two registries hold as two archives is a terminal failure when the address has no digest. The failure names each registry and its digest |
| `two-archives-by-digest`, `two-archives-by-digest-content` | 0 | the address with the digest of one archive chooses it, and the content of knurl is that archive |
| `not-found`, `not-found-terminal` | 2, 0 | a version the tenant does not have is a terminal failure |

The one-registry leaf is the OCI pack of the registries profile. The release
packs are of no use for it. This tenant has each of them in Public Repo and in
Palette Registry, as two archives. So each is the two-archive case.

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
| `floating-who`, `floating-who-system`, `floating-who-versions` | 1, 0, 0 | #69: `verify --who` on the same record reads the audit log of the profile. It shows updates by the Palette service `system`. One of them names the two versions, `1.0.1 > 1.0.2` |
| `floating-mod-keeps` | 0 | #165: the profile with values of its own stays on 1.0.1 under the tag: its record verifies after the sync that moved the first profile |
| `floating-mod-behind`, `floating-mod-behind-names` | 1, 0 | #165: `behind` on its record says that the pin is behind. It names 1.0.2 as newer, as the version that the tag names now, and as the latest of Palette |
| `delete-floating-modified` | 0 | the profile with values of its own is deleted |
| `chart-empty` | 0 | the chart repository starts empty. Two applies remove the Palette registry over it and make it again, so Palette reads the chart for the first time |
| `chart-push-1`, `chart-sync-1`, `chart-packs`, `chart-leaf` | 0 | `strata-live-chart` 1.0.0 is pushed with `helm push` into the example's OCI Helm registry. Palette reads it. `packs` and `snapshot` read the chart as a pack of type `oci` |
| `chart-digest` | 0 | #66: Palette lists the manifest digest of the push for a chart in an OCI registry, as for a pack. It does not list the digest of the chart archive |
| `chart-profile`, `chart-profile-digest` | 0 | #66: a profile that pins the chart carries that manifest digest in its pack reference |
| `chart-push-2`, `chart-push-2-differs`, `chart-sync-2`, `chart-leaf-after` | 0 | the same chart version is pushed again with other bytes, and Palette reads the registry again |
| `chart-listing-keeps`, `chart-listing-not-new` | 0 | #66: Palette keeps the digest it listed first for the version: the listing does not follow the push, unlike the pack registry |
| `chart-leaf-verify`, `chart-profile-verify` | 0 | #66: the record of the chart from before the push verifies, and so does the profile that pins it: Palette lists what it listed |
| `chart-push-3`, `chart-sync-3`, `chart-packs-two`, `chart-packs-two-versions` | 0 | #66: 1.0.1 is pushed, and the sync reads it: `packs` lists 1.0.1 first, then 1.0.0. A sync reads new versions, and keeps the digest of a known one |
| `dep-push`, `dep-push-requiring`, `dep-sync`, `dep-packs` | 0 | `strata-live-dep` 1.0.0 is pushed, then `strata-live-pack` 1.1.0, whose `pack.json` requires `strata-live-dep` 1.0.0 or newer; Palette reads both |
| `dep-packs-cloud`, `dep-packs-cloud-version` | 0 | #118: `packs strata-live-dep --registry strata-live-registry --cloud aws` lists `1.0.0`. Its `pack.json` gives the cloud type `all` alone, and the listing gives it `all` and each cloud of Palette. See [A pack that supports a cloud](../internals/palette/packs.md#a-pack-that-supports-a-cloud). |
| `dep-profile-alone`, `dep-profile-alone-says` | 1, 0 | #66: Palette refuses the create of a profile with the requiring pack and not the required one, and the refusal names the required pack |
| `dep-profile-both` | 0 | #66: a profile with both packs is accepted |
| `dep-record`, `dep-record-packs` | 0 | a record of that profile lists the two packs, the requiring one first, as authored |

In step 3b, the document of the profile also gets two annotations. They are `isInValid`
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
and tries to build a cluster outside the template. Then it builds a cluster in
the template, asks the two repave routes, and removes the cluster. Last, it
builds the cluster without the template, with a second add-on profile.

It needs
the profiles and no cluster. It ends with the removal of the whole example,
because step 5 edits a version that the cluster runs. It builds two clusters,
one after the other, on AWS or GCP.

| Check | Expected exit | What it shows |
|---|---|---|
| `release-before` | 0 | a record of the release version before the link |
| `template`, `template-links` | 0 | #66: the template links the release version and the add-on version |
| `release-doc`, `release-names-template` | 0 | #66: the version names the template that uses it, and no cluster |
| `release-record-template`, `release-record-root` | 0 | #260: a record of the linked release version has no `source.clusterTemplates`, and has the root from before the link |
| `release-consumers`, `release-consumers-template` | 1, 0 | #147: `strata consumers` of the linked version names the template with `kind: template`, and exits `1`. |
| `release-list-consumers`, `release-list-consumers-template` | 0 | #176: `strata list --packs --json` gives each linked version `consumers` 1, with `consumersByKind` `{"clusters": 0, "templates": 1}`. `consumers` counts the same template |
| `release-delete`, `release-delete-says` | 2, 0 | #147: Palette refuses the delete of the linked version, `DeletionResourceInUseError`. |
| `lock-edit`, `lock-edit-says` | 2, 0 | #66: an edit in place of a linked version, a `PUT` of the whole version, is refused, and the refusal carries the code `ClusterProfileUpdateValidationFailure` |
| `lock-verify` | 0 | #66: the version is unchanged |
| `addon-before`, `lock-manifest-edit` | 0 | #81: the linked add-on version, edited through the route of one manifest and then published, is accepted: the lock does not cover that route |
| `lock-manifest-verify` | 1 | #81: the record of the linked add-on version from before reports `DRIFT` |
| `lock-manifest-back` | 0 | the manifest is put back, and the record verifies |
| `cluster-outside-template`, `refusal-says` | 1, 0 | #66: a cluster outside the template with a linked version is refused, `ClusterCreateWithTemplateProfileForbidden` |
| `in-template-doc`, `in-template-names`, `in-template-runs-release` | 0 | #276: a cluster built in the template has `spec.clusterTemplate.uid` of the template, and runs the release version of the template. |
| `in-template-edit`, `in-template-undo` | 0 | #276: the Kubernetes pack of version 1.0.1, which the template does not link, gets one more kubeadm command, and then loses it. |
| `cluster-route`, `cluster-route-says` | 0 | #276: the route of the cluster answers no repave for the move of the cluster to that version, with no reason. |
| `template-route`, `template-route-names`, `template-route-says`, `template-route-json` | 1, 0 | #276: `compare --repave` of the same move asks the route of the template. The first `palette` line names the template, and Palette says repave: `PackValuesUpdated` for `kubernetes` and `kubeadmconfig.postKubeadmCommands`. |
| `cluster-runs-release`, `cluster-doc`, `cluster-three-profiles` | 0 | with the template gone, the cluster runs the release version and two add-on profiles |
| `lock-gone`, `lock-gone-verify` | 0, 1 | #66: the edit in place is accepted again, and the record from before reports `DRIFT`: the lock was the template's |
| `addon-order` | 0 | #66: the packs of the two add-on profiles start within thirty seconds of each other, whatever order the cluster lists the profiles in. The listed order is no install order. |

Step 4 builds the cluster in the template with the OpenTofu variable
`cluster_in_template`. The cluster then takes the profiles of the template. The
checks of the repave answer for such a cluster show the facts in
[The lock of a cluster template](../internals/palette/profiles.md#the-lock-of-a-cluster-template).

A cluster in a template needs a time zone. The variable `cluster_timezone`
gives it, and the apply of step 4 makes no cluster without it. Without a time
zone, Palette refuses the create with `ShouldNotBeEmpty` for the parameter
`timezone`: see
[A cluster in a cluster template needs a time zone](../internals/palette/profiles.md#a-cluster-in-a-cluster-template-needs-a-time-zone).

## cluster.sh

Read-write. It needs the example at its baseline with cluster A. It changes the
value that the cluster gives the variable of the add-on profile. Then it
deletes the cluster through the API and not the provider. Last, it updates the
OpenTofu state to match. It ends with the cluster gone and the profiles in
place.

| Check | Expected exit | What it shows |
|---|---|---|
| `variables-before`, `copy-before`, `audits-before` | 0 | the cluster variables endpoint, a record of the cluster's copy of the add-on profile, and the audit entries of the cluster, before |
| `variables-after`, `value-changed` | 0 | #66: after an apply that gives the variable the value `changed`, the endpoint gives that value |
| `copy-after`, `copy-verify`, `copy-compare`, `copy-source-value` | 0 | #66: the record of the copy from before verifies, and the two records compare `IDENTICAL`. The record from after carries the value under `source.cluster.variables`, outside the root |
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

Read-write. Each stage needs the example at its baseline with cluster A. Stages
1 to 3 send a `PUT` of two published versions with their packs in the other
order. They also send a `PUT` of the cluster's profiles in the other order. They
read what Palette stores, what it audits, and what `verify` says.

They leave the two versions edited, so
`live-down` follows them. A second run reverses them again, and passes the same
way.

Stage 4 runs only when you give its number: `just live-order-append`. It builds
cluster B from the same definition, as the control. It makes the release
version 1.0.2, with a new uid and the content of 1.0.0. It adds an add-on
profile of one ConfigMap to cluster A, with a `PUT` that gives that profile
first. Then it sends the `PATCH` of an upgrade of cluster A to 1.0.2, in the
form of the provider: one entry with `replaceWithProfile`.

After each step, stage 4 reads `spec.clusterProfileTemplates` of each cluster,
and `spec.clusterProfileTemplate` of the summary of each cluster. Then it
returns cluster A to its baseline profiles, deletes the added profile, and
removes cluster B and 1.0.2. The release profile of cluster A then stays last
in the stored order. When it stops early, run `just live-down`. The checks
state what each read must show, from [The order of the profiles of a
cluster](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).

| Check | Expected exit | What it shows |
|---|---|---|
| `release-before`, `registries-before`, `cluster-before`, and the `*-doc-before`, `*-doc-after`, `*-audits-before`, and `*-audits-after` checks | 0 | records of the release version, the registries version, and cluster A, and each document and its audit entries before and after each `PUT` |
| `reorder-addon`, `addon-doc-reversed`, `addon-audited` | 0 | #66: Palette accepts a `PUT` of the registries version with its two packs in the other order. It stores them in that order, and audits the change as an update of the profile |
| `addon-verify`, `addon-verify-says`, `registries-after`, `addon-record-reversed` | 1, 0, 0, 0 | #66: the record from before reports `DRIFT`, the report says `pack order changed`, and a new record lists the packs in the new order |
| `reorder-release`, `release-doc-reversed`, `release-audited` | 0 | #66: Palette accepts the four infrastructure layers of the release version in the other order, on a version that cluster A runs. The CSI layer is first, the OS layer is last, and there is an audit entry |
| `release-verify`, `release-verify-says` | 1, 0 | #66: the record from before reports `DRIFT`, and the report says `pack order changed` |
| `reorder-cluster`, `cluster-order-kept`, `cluster-audited` | 0 | #66: Palette accepts a `PUT` of the cluster's profiles in the other order. The document keeps the stored order. The audit log still has an update of the cluster |
| `cluster-verify` | 0 | #66: the cluster record verifies: strata orders the profiles itself, so a stored order is never a change |
| `append-release` | 0 | #844: the release version 1.0.0. Version 1.0.2 pins the OS pack under the tag of 1.0.0, so it has the content of 1.0.0 |
| `order-a-<step>`, `summary-a-<step>`, `order-b-<step>`, `summary-b-<step>` | 0 | #844: the document and the summary of cluster A and of cluster B. The step is `before`, `added` after the add, or `upgraded` after the upgrade |
| `append-profile`, `append-add` | 0 | #844: an add-on profile of one ConfigMap, and the `PUT` of the profiles of cluster A that gives it first. Palette takes the `PUT`. No result yet: the check is new, and no live window ran it |
| `added-last` | 0 | #844: the stored order of cluster A is the order from before, with the new profile last. No result yet: the check is new, and no live window ran it |
| `append-upgrade` | 0 | #844: Palette takes the `PATCH` of the upgrade of cluster A from 1.0.0 to 1.0.2. No result yet: the check is new, and no live window ran it |
| `upgraded-last` | 0 | #844: the stored order drops 1.0.0, keeps the other profiles in their order, and puts 1.0.2 last. So an add-on profile is at position 0. No result yet: the check is new, and no live window ran it |
| `summary-first` | 0 | #844: in each read of each cluster, `spec.clusterProfileTemplate` of the summary is the profile at position 0 of the document. No result yet: the check is new, and no live window ran it |
| `summary-addon` | 0 | #844: after the upgrade, `spec.clusterProfileTemplate` of the summary of cluster A has the type `add-on`. No result yet: the check is new, and no live window ran it |
| `control-kept` | 0 | #844: cluster B, with no change, gives the order and the `spec.clusterProfileTemplate` of its first read in each read. No result yet: the check is new, and no live window ran it |
| `append-back`, `append-detach`, `append-delete`, `append-baseline` | 0 | #844: cluster A gets 1.0.0 again and loses the added profile, and the profile is deleted. The example is at its baseline: no cluster B and no 1.0.2 |

## who.sh

Read-write. Stage 1 needs the profiles and no cluster. Stage 2 needs cluster A,
and records nothing without it. Stage 3 needs `STRATA_LIVE_NO_AUDIT_KEY`, the
key of a user with no permission to read the audit log, and records nothing
without it. Each stage makes one edit in place through the API, reads what
`verify --who` says, and puts the edit back.

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
| `noaudit-record`, `noaudit-edit` | 0 | #664: record the add-on version, and edit it in place through the API |
| `noaudit-who`, `noaudit-who-verdict` | 1, 0 | #664: with a key that cannot read the audit log, the record still reports `DRIFT` and the exit is `1`. The output starts with the verdict. The key belongs to a user with Cluster Viewer, which holds no `audit.list` |
| `noaudit-who-says` | 0 | #664: the line `who: not read: HTTP 403: <body>` has the code `AccessDenied`. The message names the permission `audit.list` |
| `noaudit-who-json` | 0 | #664: `--json` gives `verify.whoError` that starts with `HTTP 403`, no `verify.who`, and the status `drifted` |
| `noaudit-undo`, `noaudit-restored` | 0 | the edit is put back, and the record verifies again |

## draft.sh

Read-write. It needs the profiles and no cluster. It edits the manifest of the
add-on version through the API with no publish. It records the draft, publishes
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
| `route-names` | 0 | #767: while the draft waits, the `metadata.name` of the answer of `packs/manifests` is the name of the profile. The `metadata.name` of each pack summary is the name of its pin in the draft. The check only reads. |
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

## scope.sh

Read-write. It needs the profiles and no cluster. It creates a profile of the
tenant with the name, the version, and the packs of the release profile,
through the API with no project. It deletes the profile at its end. This is the
source for
[The scope of a version](../internals/palette/profiles.md#the-scope-of-a-version).

| Check | Expected exit | What it shows |
|---|---|---|
| `tenant-create` | 0 | #113: a `POST /v1/clusterprofiles` with no `ProjectUid`, and its publish, make a profile of the tenant |
| `listing-both` | 0 | #113: the profile listing of the project gives two versions with that name and that version: the tenant version and the project version |
| `search-tenant` | 0 | #113: the profile search of the project, with the filter `scope` `tenant`, names the tenant uid and not the project uid |
| `annotation-tenant`, `annotation-project` | 0 | #113: in the project, the document of the tenant version gives `metadata.annotations.scope` `tenant`, and the document of the release version gives `project` |
| `versions-tenant`, `versions-project` | 0 | #707: `spec.versions[]` of each document names the versions of its own scope. The tenant version lists its own uid and not the uid of the release. The release lists its own uid and not the uid of the tenant version. |
| `list-scope` | 0 | #113: `list --json` gives `scope` `tenant` for the tenant version and `project` for the release version |
| `list-mark`, `list-packs-mark` | 0 | #113: the row of the tenant version ends with `tenant`, with and without `--packs` |
| `ambiguous`, `ambiguous-says` | 2, 0 | #113: `snapshot <name>@<version>` exits `2` with `profile-name-ambiguous`. The error names each uid with its scope, and tells you to give the uid |
| `by-uid` | 0 | #113: `snapshot <uid>` records the tenant version |
| `tenant-list`, `tenant-list-no-mark` | 0 | #113: at tenant scope, `list --json` gives `scope` `tenant` for the version, and its row has no mark |
| `tenant-delete` | 0 | #113: the delete of the tenant profile, with no `ProjectUid` |

## projects.sh

Read-write. It needs the profiles, a second project of the tenant, and no
cluster. The name of the second project is in `STRATA_LIVE_SECOND_PROJECT`. The
script creates and publishes a copy of the release profile in that project, and
it deletes the copy at its end. It deletes no project. This is the source for
[Environments in the projects of one tenant](../use-cases/projects.md).

| Check | Expected exit | What it shows |
|---|---|---|
| `copy-create` | 0 | #486: a `POST /v1/clusterprofiles` with the `ProjectUid` of the second project, and its publish, make a copy of the release in that project. The copy has the name and the version of the release. |
| `record-a`, `record-a-project` | 0 | #486: `snapshot <name>@<version> --project-name <lab project>` records the release, and `source.projectUid` names the lab project. |
| `record-b`, `record-b-project` | 0 | #486: the same ref with `--project-name <second project>` records the copy, and `source.projectUid` names the second project. |
| `verify-both` | 0 | #486: one `verify` of the two records reads each record in the project that it names. |
| `same-release`, `same-release-projects` | 0 | #486: `same-release` of the ref twice, with `--project-name-a` and `--project-name-b`, passes. `records[0].projectUid` names the lab project, and `records[1].projectUid` names the second project. |
| `same-release-line` | 0 | #486: the terminal verdict has the line `project <lab uid> -> <second uid>` under the pair. |
| `compare-projects` | 0 | #486: `compare` with a project for each side gives the project and the uid of each side in `a` and `b`, and `sameContent` is true. |
| `records-same` | 0 | #486: `same-release` of the two record files passes. |
| `other-project-read` | ? | #486: what Palette answers to a `GET` of the copy with the `ProjectUid` of the lab project. `other-project-read.out` holds the status and the error code, and the check records them. Palette refuses the read: HTTP 500, with the code `OperationForbidden`. |
| `copy-delete` | 0 | #486: the delete of the copy, with the `ProjectUid` of the second project. |

## release.sh

Read-only. It needs the key and nothing else: no profile and no cluster. It
calls `GET /v1/mgmt/app/version` twice, once with no key and once with the key.
This is the source for
[The release of Palette](../internals/palette/limits.md#the-release-of-palette).

| Check | Expected exit | What it shows |
|---|---|---|
| `release-no-key`, `release-no-key-says` | 0 | #146: a call with no key answers 200, and the body holds `version.installedVersion`. |
| `release-with-key`, `release-with-key-says` | 2, 0 | #146: a call with the `ApiKey` header answers HTTP 500, so a tenant key cannot read the release. |

## public.sh

Read-only. It needs `gh` and the strata binary of the checkout, and it reads no
tenant. Run it after the repository and its packages are public. Each
request of a reader goes with no token and no login. The check of the advisory
form reads with the login of `gh`, because GitHub shows that setting to a
maintainer only.

| Check | Expected exit | What it shows |
|---|---|---|
| `book-opens` | 0 | #65: the address of the book that `strata --help` prints answers 200 with no login. |
| `install-no-token`, `install-runs` | 0 | #65: the install command of the README, as it is, installs the latest release with no token, and both binaries run. |
| `gitlab-install-no-token` | 0 | #483: the install lines of the template `.strata` of the GitLab drop-in install the release of its `STRATA_VERSION` with no token, and the binary runs. The check omits the lines of `apt-get`, which need the image of the job. |
| `image-no-login`, `chart-no-login` | 0 | #65: the registry gives the image and the chart of the observer of that release for an anonymous token. |
| `repository-public` | 0 | #65: the API of GitHub shows the repository with no token, with a description and topics. |
| `advisory-form` | 0 | #65: private vulnerability reporting is on. |

## contract.sh

Read-only. It needs the key and `just examples`: no profile and no cluster. It sends GET
requests only, to the tenant that `~/.palette.env` names. It has no checks. It is the contract run that
[The weekly contract run](./facts.md#the-weekly-contract-run) describes. It writes the document
`strata-contract/1`, and exits with 0, 1, or 2.

The run also sends the read-only calls of the tool of a cloud, such as `aws` and `gcloud`, in the
lab account. `AWS_PROFILE` and [the lab values](./facts.md#the-lab-values) select that account.
A call needs the objects that the lab values name, such as the project and the role.
`--no-cloud` sends no call of a cloud. `--with-cluster` says that the tenant holds a cluster: the run
then makes an error of each skipped fact of the floor.

A tenant with no profile and no cluster skips most of the facts. Run `just live-profiles` first to
give the run profiles to read, and `just live-up` to give it a cluster.

| Result | What it shows |
|---|---|
| `contract.json` | #390, #391, #530: for each fact of the kind `request`, the status and the type of each field against the stand-in. The run asks with the query that strata sends. It checks the names that strata needs, and lists each value of a closed set that the fact and the stand-in do not name. On the real tenant, no fact of this kind differs, and no value is unknown. |
| `contract.json` | #393: for each fact of the kind `cloud`, the shape of the answer, and the values of its table, against the fixtures of the rules. Each call of AWS that the lab sends is `same`. The six calls of GCP cannot be checked, because `gcloud` needs a login on the machine. The calls for a GKE cluster and for an image are skipped, because the lab values name none. |
| `contract.json` | #539: the fact `tag-filter-listing` is `same`, because the run sends it with no project. The fact `tag-filter` is skipped, because the tenant holds no saved filter. The fact `audit-log` is `same` for a page that lacks a kind that strata acts on and for a kind that strata ignores. |
| `contract.json` | #539: the facts `validate-aws-regions` and `validate-security-groups` are `same`, because the run sends the account, the region, and the VPC of the tenant. A zone id that the lab account lacks gives no difference. The facts `eks-refused-zone-id-us-west-1` and `eks-refused-zone-id-ca-central-1` are skipped. The entry says that the account cannot check the value. |
| `contract.json` | #543: the fact `validate-key-pairs` is `same`, because the run sends the account of the tenant in the query `cloudAccountUid`. |
| `contract.json` | #595: the four facts of a region are `same`: `validate-vpcs`, `validate-security-groups`, `validate-key-pairs`, and `validate-instance-types`. The run takes one region for all of them, the first region that lists a VPC. The lab has no region that AWS refuses, so that region is the first of the list. The whole run finds 120 facts: 54 `same`, none `differs`, 66 skipped, none that the run cannot check, and 0 unknown values. `just contract-report --dry-run runs/contract.json` exits 0 and opens no issue. |
| `contract.json` | #554: with a cluster up, the fact `profile-document` is `same` for the first profile of the listing. The run has 0 unknown values, also for the type of the profile and for the provider of a pack ref. A type or a provider that the fact does not name is an unknown value. The fact `cloud-config` reads the cloud type of a cluster as strata does. A cluster that names its cloud type in the kind of its cloud config only still gives a subject. |
| `contract.json` | #540: the fact `aws-policies-per-role-limit` is `same`: the default quota of managed policies for a role is 20. The fact `aws-policies-per-role-quota` is `same`: the lab account gives the quota in `us-east-1` with the shape that the fixtures of `iam-role-policy-quota` hold. |
| `contract.json` | #390, #393: the whole run on the real tenant, with the cloud calls and the lab account. It finds 120 facts: 47 are `same`, none `differs`, 67 are skipped, and 6 cannot be checked. [The facts page](./facts.md#what-the-run-compares) lists the cases where the run skips a fact. |
| `contract.json` | #567: with a cluster up that had no repave, the fact `cluster-repave-status` is `skipped`. The reason names the code `ResourceNotFound`, which Palette gives for that cluster with HTTP 404. The run finds 120 facts: 61 are `same`, none `differs`, 59 are skipped, none cannot be checked, and 0 values are unknown. `just contract-report --dry-run runs/contract.json` exits 0 and opens no issue. |
| `contract.json` | #576: with a cluster up, `just live-contract --with-cluster` exits 0. The run finds 120 facts: 63 `same`, none `differs`, 57 skipped, none that the run cannot check, and 0 unknown values. No fact of the floor is an `error`. [The floor](./facts.md#the-floor-of-a-window-with-a-cluster) lists the rule. The fact `cluster-repave-status` stays skipped for a cluster with no repave, and its reason names `ResourceNotFound` with HTTP 404. `just contract-report --dry-run runs/contract.json` exits 0 and opens no issue. |
| `contract.json` | #576: the fact `validate-security-groups` is `same`, and it was `same` before #576. Each region of the lab account that a live run read holds a VPC, so no live run shows the search past the first region. The tests show the search. No live run shows a first region with no VPC. The entry of `validate-instance-types` is `same` for the choice `aws`. Its reason says that the choices `gcp` and `azure` have no way to find a value for `{region}`. |

## contract-profiles.sh

Read-write, with no cluster. `up` makes the standing cluster profiles of the contract run, and
`down` deletes them. See [The profiles of the run](./facts.md#the-profiles-of-the-run). It has no
checks. It needs the key, the `terraform.tfvars` of the live example, and AWS credentials.
`AWS_PROFILE` selects the account.

| Result | What it shows |
|---|---|
| `up` | #531: the plan holds cluster profiles only: no `aws_*` resource, no registry, no cluster. It holds four profiles: two add-on profiles, and two versions of one profile of the type `cluster`. `up` makes the four. |
| `contract.json` of the next contract run | #531: the facts that need a cluster profile are `same` or `differs`, and none is `skipped` for want of a profile. The facts `profile-listing`, `profile-document`, `profile-pack-manifests`, `profile-variables`, and `profile-resolved-values` are `same`. |

## report-send.sh

It reads and writes no Palette tenant. It needs the address of the deployed endpoint of the report,
and a `gh` login that can write to the repository. It serves the tenant of the chapter `unknown-value` of the book
on this machine, and runs strata against it with `STRATA_REPORT=1` and `STRATA_REPORT_URL` set. So
the send is real, and the value is the made-up profile type of the book. The run opens or changes the
issue of that value through the endpoint, and leaves it open. [The endpoint of the
report](./report-endpoint.md) says how the lead deploys the endpoint and runs this check.

The run changes `HOME`, so the script keeps the login of `gh`. It sets `GH_CONFIG_DIR` to the folder
of the login before the change, and a `GH_TOKEN` of the environment still works. When `gh` cannot
read the issues, the script stops with exit `2` before the first send.

| Check | Expected exit | What it shows |
|---|---|---|
| `send-1`, `send-1-says`, `send-1-marked` | 0 | #397: a run with `STRATA_REPORT=1` exits as a run with no send, says that it sent the report, and marks the line as sent. Each check passes. |
| `issue`, `issue-new` | 0 | #397, #574: one issue of the label `unknown-value` holds the marker of the value, and its first send gives the count 1. The endpoint answers 202 and then writes the issue, so the script waits for it. The check `issue` passes. The service found the issue of the value under the label `unknown-value`, after [the move of the issues to that label](./report-endpoint.md#move-the-issues-to-the-label-of-the-service). The count continued from the state of the older service. The check `issue-new` runs only when the first send opens the issue, and it did not run, because the issue of the value was open. |
| `send-2`, `issue-2` | 0 | #397: a second run posts the report again. Still one issue holds the value, and its count grew by one. Each check passes. |
| `repeat-1` | 0 | #552: the endpoint takes a report with a key and answers 202 before it writes the issue. The service keeps the CPU after the answer, so the issue changes. The check passes: the endpoint answers 202 before it writes. |
| `repeat-2`, `repeat-count` | 0 | #552: the same report with the same key gets 200 and `duplicate`, and the issue counts the report one time. Each check passes: the same report with the same key counts one time. |

The script reads the repository from the `Cargo.toml` of the root. It reads the start of the row
`Lines` from one variable, `lines_row`. A test of the endpoint makes sure that the service writes
this row and the marker of the issue. The checks `issue`, `issue-new`, `issue-2`, and
`repeat-count` use these reads (#593).

`just live-report-send` passes on the deployed service. The checks `send-1`, `send-1-says`,
`send-1-marked`, `issue`, `send-2`, `issue-2`, `repeat-1`, `repeat-2`, and `repeat-count` pass. The
check `issue-new` did not run, because the issue of the value was open.

## pending.sh

Read-write. It needs the example at its baseline with cluster A. Each stage
makes one change in place and puts it back, so it ends at the baseline.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-record`, `base-none` | 0 | #70: at the baseline, a record of cluster A says `updatesPending: false` |
| `base-profile-updates` | 0 | #70: `profileUpdates` lists each profile of the cluster also when no update waits: it is no flag |
| `base-at-pending`, `base-pending-same`, `base-pending-flag` | 0 | #78: with no update waiting, a record of `cluster:<uid>@pending` has the root of the cluster, and says `pending: true` |
| `base-consumers`, `base-list-only`, `base-list-only-same` | 1, 1, 0 | #78: `consumers --list-only` gives the clusters that the second reading gives, and says `skipped` |
| `addon-edit`, `addon-pending`, `addon-record`, `addon-names` | 0 | #70: an edit in place of a manifest of the add-on version opens an update. The record says `updatesPending: true`, and `pendingUpdates` names the add-on and `Manifest ... is updated` |
| `addon-root` | 0 | #70: the root of that record is the root at the baseline: the statement is outside the root |
| `addon-release-only` | 0 | #70: a record of the release profile alone says that no update waits for it |
| `addon-list`, `addon-list-says` | 0 | #70: `list clusters --applied` marks the add-on `behind: an update waits` |
| `addon-compare`, `addon-compare-says` | 1, 0 | #70: `compare` of the add-on version with its copy on cluster A says `cluster ... is behind` |
| `addon-at-pending`, `addon-pending-root` | 0 | #78: after the edit, the record of `@pending` has another root than the copy that runs |
| `addon-pending-compare`, `addon-pending-compare-says` | 1, 0 | #78: `compare cluster:<uid> cluster:<uid>@pending` names the pack of the add-on `AUTHORED`, and no pack of the release |
| `addon-pending-verify` | 0 | #78: `verify` of the pending record, while the update still waits |
| `addon-repave`, `addon-repave-says`, `addon-repave-json` | 1, 0, 0 | #276: `compare --repave` of the cluster with `cluster:<uid>@pending` exits `1`. The `palette` line holds the answer of Palette for a manifest of the add-on. The answer is no repave. |
| `accept`, `accept-none` | 0 | #70: a push of the content without `resolveNotification` closes the update all the same |
| `accept-record`, `accept-as-pending` | 0 | #78: the cluster then runs the root that the record of `@pending` held |
| `accept-undo`, `accept-undo-pending`, `accept-again`, `accept-again-none` | 0 | #70: the undo is an edit too, and opens an update; the accept with `resolveNotification` closes it |
| `values-edit`, `values-pending`, `values-record`, `values-names` | 0 | #70: an edit in place of a value of the release version opens an update, and Palette names the pack: `csi-aws-ebs ... values are updated` |
| `values-pending-compare`, `values-pending-compare-says` | 1, 0 | #78: `compare` with `@pending` names one pack, the CSI pack, and the path of the reclaim policy |
| `values-undo`, `values-undo-none` | 0 | #70: an edit that gives the profile the content of the copy again closes the update, with no update of the cluster |
| `values-baseline-none` | 0 | #70: the apply of the baseline, which writes the same content, opens no update |
| `bump-uid-kept`, `bump-record`, `bump-none` | 0 | #70: a new version number alone, in place with the flag off, keeps the uid and opens no update |
| `k8s-edit`, `k8s-pending` | 0 | #276: one more kubeadm command in the Kubernetes pack of the release version, in place, opens an update. |
| `k8s-repave`, `k8s-repave-says`, `k8s-repave-json` | 1, 0, 0 | #276: `compare --repave` of the cluster with `cluster:<uid>@pending` exits `1`. The `palette` line and `paletteRepave` hold the answer of Palette: repave, `PackValuesUpdated` for `kubernetes` and `kubeadmconfig.postKubeadmCommands`. |
| `k8s-undo`, `k8s-undo-none`, `k8s-baseline-none` | 0 | #276: the undo closes the update with no update of the cluster, and the apply of the baseline opens none. |

## macros.sh

Read-write. It needs the example at its baseline with cluster A, and `kubectl`.
It makes a project macro, has the manifest of the add-on version read it, and
changes its value. It reads the value inside the cluster with a kubeconfig that
it keeps in a temporary file. It puts each change back, and deletes the macro.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-flag`, `base-record`, `base-no-note` | 0, 0, 1 | #72: at the baseline, Palette says the add-on version has no user macros, and a report names none |
| `base-profile-resolved` | 0 | #111: Palette resolves the macros of the version with no cluster, in one `resolved` map. The system macro of the cluster name is empty there |
| `macro-set`, `macro-edit`, `macro-flag` | 0 | #72: with the manifest reading a project macro, Palette says `hasUserMacros: true` |
| `macro-record`, `macro-names` | 0 | #72: a report of the record names the macro, and says the root covers its name and not its value |
| `macro-accept`, `macro-settled`, `macro-cluster`, `macro-cluster-plain`, `macro-digest` | 0 | #72: cluster A takes the update; a record with `--macros` holds `sha256:` of the value, and not the value |
| `macro-resolved-1` | 0 | #72: `resolvedValues` gives the value of the macro |
| `macro-version`, `macro-version-digest`, `macro-profile-resolved-1` | 0 | #111: a record of the version with `--macros` holds `sha256:` of the value at the scope of the version in `source.macroValues`. `resolvedValues` of the version gives the value of the project macro |
| `change`, `change-resolved` | 0 | #72: the macro gets another value, and `resolvedValues` gives it at once |
| `change-profile-resolved` | 0 | #111: `resolvedValues` of the version gives the new value at once |
| `change-version-macros`, `change-version-macros-says` | 1, 0 | #111: the record of the version with the digests reports `DRIFT`, and the verdict names the macro |
| `change-version`, `change-plain` | 0 | #72: the record of the version, and a cluster record without the digests, still verify |
| `change-cluster`, `change-cluster-says` | 1, 0 | #72: the cluster record with the digests reports `DRIFT`, and the verdict names the macro |
| `change-compare`, `change-compare-says` | 1, 0 | #72: `compare` of the two cluster records exits `1` with equal roots, and names the macro |
| `change-no-notification`, `change-no-audit` | 0 | #72: no notification opens for the cluster, and its audit log holds no new entry |
| `propagation` | 0 | #72: Palette writes the new value inside the cluster with no update to accept. It does this after 11 minutes in the recorded run. The check was recorded without an expected code |
| `undo-edit`, `undo-accept`, `undo-settled`, `undo-macro`, `undo-flag`, `undo-baseline` | 0 | the manifest back, the macro deleted, and the baseline |

## image-digest.sh

Read-write. It needs the example at its baseline with cluster A, `kubectl`, and
`crane`. Stage 2 lists the image with its digest in `pack.content.images`, and
has the manifest make a Deployment that names the tag. Stage 3 names the digest
in the manifest. Each stage reads the image field of the Deployment and the
image of the pod. Stage 4 puts each change back and deletes the Deployment with `kubectl`,
because Palette leaves an object that an edit drops from a manifest.

The manifest stands for a chart. The example has no chart that cluster A runs.
The script keeps the kubeconfig in a temporary file. A check with `?`
records its result without judging it, because the run finds the result.

A digest in `pack.content.images` alone does not pin the image. The list of the
values holds the digest, and the Deployment stays on the tag. A digest in the
manifest that the workload reads pins the image. The checks `list-spec` and
`digest-spec` show each case.

| Check | Expected exit | What it shows |
|---|---|---|
| `base-record`, `base-no-deployment` | 0 | #630: a record of the add-on version, and no Deployment in cluster A |
| `list-values`, `list-manifest` | 0 | #630: Palette takes the list of images and the Deployment, in place |
| `list-record`, `list-record-digest` | 0 | #630: a record holds the digest in the image list. Palette keeps the values of a manifest pack |
| `list-compare`, `list-compare-flag` | 1, 0 | #630: `compare` names the list with `IMAGES` |
| `list-accept`, `list-deployed` | 0 | #630: cluster A takes the update, and the Deployment runs there |
| `list-spec`, `list-spec-digest`, `list-pod` | ? | #630: what the Deployment and the pod run, when only the list has the digest. The field of the Deployment holds the tag, and `list-spec-digest` records `1`. The pod reports the digest that the node pulled for that tag |
| `digest-manifest`, `digest-record` | 0 | #630: the manifest names the image with its digest |
| `digest-compare`, `digest-compare-flag` | 1, 0 | #630: `compare` names a changed value with `AUTHORED`, and no change of the list |
| `digest-accept`, `digest-deployed` | 0 | #630: cluster A takes the update, and the image field holds the digest |
| `digest-spec`, `digest-pod` | ? | #630: what the Deployment and the pod run, when the manifest has the digest. The field of the Deployment holds the tag and the digest, and the pod reports the same digest as in the first case |
| `undo-manifest`, `undo-values`, `undo-accept` | 0 | the manifest and the values back, and cluster A takes them |
| `undo-gone` | ? | #630: the Deployment stays on cluster A after the edit drops it from the manifest. The check waited 20 minutes and the Deployment was still there, so it records `1`. See [An object that an edit drops from a manifest](../internals/palette/clusters.md#an-object-that-an-edit-drops-from-a-manifest) |
| `undo-delete`, `undo-clean` | 0 | #630: `kubectl` deletes the Deployment, and cluster A runs no Deployment of the example. The run leaves the cluster clean |
| `undo-baseline` | 0 | the example is at its baseline |

## macro-repave.sh

Read-write, and it replaces the nodes of cluster A. It needs the example at its
baseline with cluster A, and `AWS_PROFILE` to read the EC2 instances. It puts a
project macro in `kubeadmconfig.postKubeadmCommands` of the Kubernetes pack,
and has cluster A take that. Then it changes only the value of the macro, and watches
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
minute. The state became `Pending` 14 minutes 58 seconds after the change of the value,
at the next read of the agent.

`GET /v1/spectroclusters/<uid>/repave/status` gives the record. It holds the
source `hubble`, and the reason `PackValuesUpdated` with the message
`Pack 'kubernetes' values are updated`. It also holds `diffConfigKeys`
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
destroy` asked Palette to delete cluster A at 04:05:45. The cluster stayed
in `Deleting`, and its EC2 instances ran for 37 minutes, until
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

Stage 9 runs only with `VMO_CLUSTER=<uid>` of a cluster with the pack
`virtual-machine-orchestrator` and no cluster template. It moves the console of
the pack to a mark, and back. Each question sends the values of the cluster in
each profile with a pack of virtual machines, also `vm-migration-assistant`. A
profile by its uid alone can move the address that the console forwards to: see
[What the route saves](../internals/palette/validate-routes.md#what-the-route-saves).
The stage stops before the first question when a check before it fails. So a
cluster without the pack, or under a cluster template, gets no question.

The forward of the console keeps the address that it read, usually for at most
15 minutes. So the stage waits `VMO_WAIT` seconds (default 960) before the
first question. After each question, it reads the console each 5 seconds, for
at most `VMO_WAIT` seconds. In some cases, the forward keeps an address until
Palette starts the forward service again. Then `vmo-saved` or `vmo-back` fails,
and the stage names the forward as a cause. After such a `vmo-back`, the
console forwards to the mark until Palette starts the forward service again.

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
| `validate`, `validate-says`, `validate-json` | 0 | #134: at the baseline, `compare --validate` prints `palette    accepts`; `--json` gives `accepts` true and no error. The cluster runs the version, so strata asks the route of that version (#453), and Palette accepts the packs. |
| `validate-usage`, `validate-usage-says` | 2, 0 | #134: two profile versions and no cluster are a usage error for `--validate`. |
| `var-record-before`, `var-command` | 0 | #132: the Kubernetes pack of 1.0.1 has the kubeadm command `chmod 600 /var/lib/kubelet/config.yaml` |
| `var-record`, `var-in-values` | 0 | #132: with `next_variable` and `next_variable_in_values`, 1.0.1 has the profile variable `strata_mode`, default `600`, and the command reads it in place of `600` |
| `var-strata`, `var-strata-answer` | ? | #132: `compare --repave` of cluster A with 1.0.1 sends the uid of 1.0.1 alone, with no `packValues` and no variables. Palette says no repave: `required` false (recorded 1, 1) |
| `var-none`, `var-default` | ? | #132: `tenant.sh repave` sends the uid of 1.0.0, `replaceWithProfile` 1.0.1, and the `packValues` of 1.0.1. With no variables, and with `strata_mode` `600`, Palette says repave: `PackValuesUpdated` for `kubeadmconfig.postKubeadmCommands` (recorded 1, 1) |
| `var-control`, `var-control-says` | 1, 0 | #132: with `strata_mode` `644`, the command changes, and Palette says repave for `kubeadmconfig.postKubeadmCommands` |
| `var-undone` | 0 | 1.0.1 is as before the stage: its record from before verifies |
| `vmo-cluster`, `vmo-runs` | 0 | #717: the cluster runs `virtual-machine-orchestrator`, the values of the pack hold `consoleBaseAddress`, and no cluster template governs the cluster. No result yet: no live window ran stage 9. |
| `vmo-clear` | 0 | #741: no line `consoleBaseAddress` of a pack of virtual machines in the document of the cluster holds `********`. Palette writes no mask over the console address, so strata writes the mask in each record. No result yet: no live window ran stage 9. |
| `vmo-summary` | 0 | #717: the summary of the cluster gives the console address of the service `vm-dashboard`. No result yet: no live window ran stage 9. |
| `vmo-mark-body`, `vmo-before` | 0 | #717: the body holds the mark `https://strata-live-check.invalid` in `consoleBaseAddress`. Before the question, the console address forwards to a host that answers: no 404, no 502, and an answer in 30 seconds. No result yet: no live window ran stage 9. |
| `vmo-ask`, `vmo-ask-ok` | 0 | #717: `validate/repave` with the mark answers HTTP 200. No result yet: no live window ran stage 9. |
| `vmo-saved` | 0 | #717: in `VMO_WAIT` seconds, the console address answers 502: Palette saved the mark at the question, and the mark has no host. `vmo-saved.out` holds the status of each read. No result yet: no live window ran stage 9. |
| `vmo-own`, `vmo-own-ok`, `vmo-back` | 0 | #717: `validate/repave` with the values of the cluster in the profile of the pack answers HTTP 200, and saves the address of the cluster again. In `VMO_WAIT` seconds, the console address forwards to a host that answers again. No result yet: no live window ran stage 9. |

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

No cluster and no cost. In stages 1, 2, 3, 5, and 6, each request to Palette is a
read or a request to a validate route, which writes nothing. Stage 7 sends an update
of a cluster of the lab. It uses `cert-manager`
1.21.1 from the Public Repo, in an add-on candidate, and `gloo` 1.2.18 for the
mark `disabled`. These are the stages and the options:

- Stage 3 runs only with a profile version of the project. Give it as
  `PROFILE=<name>@<version>`. With the add-on version of the live example,
  Palette takes the version on the route of an update. It refuses a candidate
  with that name and version on the name route.
- Stage 4 makes the profile `strata-live-layer` and deletes it. It runs only
  when you give its number: `validate-packs.sh 4`.
- Stage 5 asks the name route and the import validate route.
- Stage 6 runs only when you give its number:
  `PROFILE=<name>@<version> validate-packs.sh 6`. It needs a profile version
  that a cluster runs and that pins Kubernetes two minor versions or more below
  the newest version.
- With `CLUSTER=<name>`, `behind` also asks about the record of the cluster.
  The script sends the route of the cluster a tag that no registry has. It
  asks, and it writes nothing.
- Stage 7 runs only when you give its number:
  `CLUSTER=<name> SKIP=<name>@<version> VARIABLES=<name>@<version> validate-packs.sh 7`.
  `SKIP` is a version of a profile that the cluster runs, with Kubernetes two
  minor versions above the cluster. `VARIABLES` is a version that adds a
  required variable with no default value. Give one of the two, or both. The
  stage asks the route about each move, then sends the update. When Palette
  accepts an update, the cluster moves, so give a cluster of the lab only.

| Check | Expected exit | What it shows |
|---|---|---|
| `validate-accepts`, `validate-accepts-record` | 0 | #152: Palette takes the packs of the candidate; `snapshot --validate` writes the record with `source.paletteValidation` `{"accepts": true}` |
| `validate-accepts-says` | 0 | #633: the line `strata: Palette accepts the name, the version, and the packs of '<file>'` says that Palette takes the candidate. No result yet: the line is new, and no live window ran it |
| `validate-not-yaml`, `validate-not-yaml-says`, `validate-not-yaml-no-record` | 2, 0, 1 | #152, #242: strata sends values that are not YAML to Palette. Palette refuses the whole request, `PackValuesYamlInvalid`, with no pack. Strata exits `2` and writes no record |
| `validate-not-yaml-status`, `validate-not-yaml-status-says` | 2, 0 | #152: Palette refuses the same body as the whole request, `PackValuesYamlInvalid` with no pack, and sends the refusal as HTTP 500 |
| `validate-no-pack`, `validate-no-pack-says` | 2, 0 | #152: Palette refuses a body with no pack before it checks the packs: HTTP 400 with `ShouldNotBeEmpty` |
| `validate-disabled-mark`, `validate-disabled-mark-says` | 2, 0 | #125: Palette refuses a candidate with `gloo` 1.2.18, which carries the mark `disabled` and no `system_state`. It refuses the whole request with `DisabledPacksForbidden`, `Disabled packs 'gloo' are forbidden`. The import validate route gives it with HTTP 500. Strata prints `palette    refuses`, exits `2`, and writes no record |
| `validate-version` | 0 | #152: `snapshot <ref> --validate` asks the route of an update, and Palette takes the version as it is |
| `validate-version-says` | 0 | #633: the line `strata: Palette accepts the packs of '<ref>' as an update of itself` says that Palette takes the version. No result yet: the line is new, and no live window ran it |
| `validate-exists`, `validate-exists-says` | 2, 0 | #273: a candidate with the name and the version of `PROFILE` gets `ClusterProfileAlreadyExists`, `Cluster Profile with name '<name>' and version '<version>' already exists`. Strata prints `palette    refuses`, exits `2`, and writes no record |
| `layer-registry` | 0 | #210: the registry gives `cert-manager` the layer `addon` |
| `layer-refused`, `layer-refused-says` | 2, 0 | #210: strata refuses a candidate that gives `cert-manager` the layer `csi`. The message names the layer, `csi`, `addon`, and the rule of Palette |
| `layer-validate`, `layer-validate-says` | 0 | #254: the validate route takes the pack with the layer `csi`. It answers `{"packs":{"results":[]}}`, with no result for the pack |
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
| `bump-profile-record` | 0 | #166: a record of `PROFILE`. |
| `bump-pair` | 0 | #166, #623: the newest version of `kubernetes` is two minor versions or more above the pin. `bump.updateChecks.skippedMinor` of `behind` says so. |
| `bump-asked` | 0 | #166: `behind` asks the route of the version, and the bump of `kubernetes` holds an answer of Palette in `bump.paletteValidation`. |
| `bump-refused`, `bump-clusters` | 0 | #166: Palette refuses the move with `DependencyPackUpgradeNotSupported`, and the error names a cluster that runs the version. |
| `bump-text-says` | 0 | #166: `behind` prints `palette refuses: DependencyPackUpgradeNotSupported on <clusters>` under the bump. |
| `bump-text-skip` | 0 | #623: `behind` prints `kubernetes <pin> -> <newest> skips a minor version: Palette refuses the update with K8sUpgradeNotSupported` under the bump. No result yet: no live window ran the check. |
| `bump-no-impact-says` | 0 | #166: `behind --no-impact` writes no bump, so it sends no question. |
| `bump-cluster-record` | 0 | #166: a record of the cluster `CLUSTER`. |
| `bump-cluster-same` | 0 | #419: with `CLUSTER`, the record of the cluster and the record of the profile get one bump of `kubernetes`. The two bumps have the same version, impact level, and change flags: the bump to 1.37.0, with the level `repave`. |
| `bump-cluster-asked` | 0 | #453: `behind` on the record of the cluster asks the route of the profile version that the cluster runs. The bump holds an answer of Palette in `bump.paletteValidation`. |
| `bump-cluster-refused` | 0 | #453: the route of the profile version that the cluster runs refuses the same move. The code is `DependencyPackUpgradeNotSupported`, and the message is `Pack upgrade to '1.37.0' is not compatible with the clusters '<cluster A>'`. Cluster A is in `clusters`. |
| `cluster-route-doc`, `cluster-route-tag`, `cluster-route-tag-says` | 0 | #453: the route of the cluster answers HTTP 200 with no error for a body with the tag `9.99.9` for `kubernetes`. No registry has that version, and the profile is one that the cluster runs. So the route does not check the sent tag. The body of a real cluster is longer than one argument can be (128 KiB). So `cluster-route-tag` writes the body to `cluster-route-body.json` and gives `api.sh post-status` the path of that file (#466). |
| `move-cluster-record` | 0 | #623: a record of the cluster `CLUSTER`. No result yet: no live window ran stage 7. |
| `skip-record`, `skip-validate`, `skip-asked` | 0, ?, 0 | #623: `compare cluster:<CLUSTER> <SKIP> --validate` asks the route about the move, and the comparison holds an answer. The answer is a fact of the route: the code of Palette shows that no validate route checks the minor version. No result yet: no live window ran stage 7. |
| `skip-says` | 0 | #623: `updateChecks.skippedMinor` of the comparison says that the move skips a minor version. No result yet: no live window ran stage 7. |
| `skip-update`, `skip-refused` | 0 | #623: `api.sh replace-status` sends the update, as the Spectro Cloud provider sends it. Palette refuses it with HTTP 400 and `K8sUpgradeNotSupported`, the code that `skippedMinor.code` names. No result yet: no live window ran stage 7. |
| `variables-record`, `variables-validate`, `variables-asked` | 0, ?, 0 | #623: `compare cluster:<CLUSTER> <VARIABLES> --validate` asks the route about the move, and the comparison holds an answer. No result yet: no live window ran stage 7. |
| `variables-accepts`, `variables-says` | 0 | #623: the route accepts the packs, and `updateChecks.requiredVariables.names` holds the required variables that the move adds. No result yet: no live window ran stage 7. |
| `variables-update`, `variables-refused` | 0 | #623: Palette refuses the update with HTTP 500 and `InvalidVariable`, the code that `requiredVariables.code` names. The message names a variable of `requiredVariables.names`. No result yet: no live window ran stage 7. |

Each check of stage 6 but `bump-text-skip` passes on cluster A, built on
Kubernetes 1.35.8. That version is two minor versions under the newest version
of the registry, 1.37.0. The stage exits `0`. `bump-text-skip` has no result
yet: no live window ran the check.

Stage 7 can use cluster A on Kubernetes 1.35.8 too. `SKIP` is then a version of
the release profile with Kubernetes 1.37.0. `VARIABLES` is a version of the
add-on profile of cluster A with one more variable: required, with no default
value.

Stage 6 needs a cluster two minor versions behind. When cluster A runs the
release version, Kubernetes 1.36.2, the route of the profile version accepts the
move to 1.37.0. Then the checks `bump-pair`, `bump-refused`, `bump-clusters`,
`bump-text-says`, and `bump-text-skip` fail. Build cluster A on the older
version with this command:

```bash
examples/live/tf.sh apply -var k8s_version=1.35.8 \
  -replace='spectrocloud_cluster_aws.cluster["a"]'
```

A script that checks the baseline needs `TF_VAR_k8s_version=1.35.8` after that
build. Then the plan of the baseline has no change.

No pack of the Public Repo lists a dependency in its tags route. Only stage 6
asks for a refusal of a dependency, the upgrade dependency, which no read route
gives.

The validate route does not check the layer of a pack. Palette answers a pack
with another layer than its registry with `{"packs":{"results":[]}}`, the
answer of a body that it takes. So strata refuses the candidate itself: see
[The layer of a pack](../internals/palette/packs.md#the-layer-of-a-pack).

## validate-route.sh

No cluster, no profile of its own, and no cost. It asks
`POST /v1/spectroclusters/{cloudType}/validate`, which changes nothing, through
`strata preflight --rule palette-validate`. It needs the key, a directory with
the body of a create for each engine, and the cloud accounts that the bodies
name.

A case makes one fault. The fault is one edit of the body of its engine, or a
body of its own. A case has a body of its own when the fault is in a profile or
needs a VPC of the lab.
[`route-bodies.sh`](#route-bodiessh) writes those bodies on AWS for each case
whose profile Palette takes, and for `cidr-overlap`, and runs this script on
them. A body of a case names a profile, so it is valid only while that profile is
in the tenant. A case whose body is not in the directory is skipped, and
`steps.log` names it.

A case that passes shows that the route answers the check. The part of the rule
that the case names can then leave the catalog. A case that fails shows that the
route does not answer the check, and the rule stays. See
[A check that a validate route answers](../reference/preflight-rules.md#a-check-that-a-validate-route-answers).

One case expects the opposite result. The route takes the body of `pack-cloud-support`, so its
two checks expect the verdict `passed` and no message.

| Check | Expected exit | What it shows |
|---|---|---|
| `aws-clean`, `edge-clean`, `eks-clean`, `gcp-clean`, `maas-clean`, `azure-clean`, `vsphere-clean` | 0 | #243: the route takes the clean body of the engine, so a refusal in a case below is about its fault. A live run shows it on AWS and on EKS: `aws-clean` and `eks-clean` passed. The lab has no cloud account of vSphere, Edge, Azure, or MAAS, so those bodies have no result (#409). |
| `vsphere-instance-floor-cpu`, `-memory`, `-disk`, `-no-type`, each with `-says` | 0 | #243: the route refuses a pool with no CPU, 512 MiB of memory, a 10 GiB disk, or no instance type (`vsphere-instance-floor`). No result yet: the lab has no cloud account of vSphere (#409). |
| `eks-ami-type-al2`, `eks-ami-type-unknown`, each with `-says` | 0 | #243, #409: the route refuses the type `AL2_x86_64` with `UnsupportedAmiType`, and a type that is not known (`BOTTLEROCKET_x86_64`) with `IsValidValue`. `palette-validate` gives the verdict `failed` with the message of Palette. The rule `eks-ami-type` has left the catalog. |
| `edge-two-node-pxk`, `edge-two-node-pxk-says` | 0 | #243: the route refuses a two-node cluster whose Kubernetes pack is not K3s with `TwoNodeClusterPackMissing` (`edge-two-node`). No result yet: the lab has no cloud account of Edge (#409). |
| `ssh-key-format-vsphere`, `ssh-key-format-edge`, each with `-says` | 0 | #243: the route refuses a key that is not an authorized key with `SshKeyValidationFailure` (`ssh-key-format`). No result yet: the lab has no cloud account of vSphere or Edge (#409). |
| `azure-ssh-key-empty`, `azure-ssh-key-empty-says` | 0 | #243: the route refuses an empty `sshKey` (`azure-ssh-key`). No result yet: the lab has no cloud account of Azure (#409). |
| `pack-state-disabled`, `pack-state-deleted`, each with `-says` | 0 | #243: the route refuses a profile that holds a pack in the state `disabled` or `deleted` (`pack-state`). No live run can show it: Palette refuses a new profile with such a pack at the create, as the cases of [`route-bodies.sh`](#route-bodiessh) show. |
| `profile-cloud-mismatch`, `profile-cloud-mismatch-says` | 0 | #243, #415: the route refuses a body with a profile of another cloud type with HTTP 500 and `ClusterProfilesNotSupported`. `palette-validate` gives the verdict `failed` with the message of Palette. The rule `profile-cloud-mismatch` has left the catalog. |
| `pack-cloud-support`, `pack-cloud-support-says` | 0 | #415: the route takes a body with a pack version that does not list the cloud type. `palette-validate` passes and gives no message. So no route answers `pack-cloud-support`, and the rule stays in the catalog as a gap that preflight fills. |
| `cidr-overlap`, `cidr-overlap-says` | 0 | #816: the route refuses a body whose pod CIDR overlaps the CIDR of its VPC, with `K8SCidrOverlap`. `palette-validate` gives the verdict `failed` with the message of Palette, also when the status is `500`. A live run of the first body, with no subnet for the pools, failed both checks. The route refused that body with `RequiredParam` (#824). A live run of the body with the subnets passes both checks. |
| `vsphere-static-ip-pool-empty`, `vsphere-static-ip-pool-dhcp`, each with `-says` | 0 | #243: the route refuses a static network with no `parentPoolUid`, and a static network on a DHCP cluster (`vsphere-static-ip-pool`). No result yet: the lab has no cloud account of vSphere (#409). |
| `vsphere-controlplane-endpoint-no-host`, `edge-vip-multi-node-no-host`, each with `-says` | 0 | #243: the route refuses the type `VIP` with an empty host (`vsphere-controlplane-endpoint`, `edge-vip-multi-node`). No result yet: the lab has no cloud account of vSphere or Edge (#409). |
| `azure-subnets-in-vnet-no-cidr`, `azure-subnets-in-vnet-no-cidr-says` | 0 | #243: the route refuses a `vnetName` with an empty `vnetCidrBlock` (`azure-subnets-in-vnet`). No result yet: the lab has no cloud account of Azure (#409). |
| `ntp-servers-vsphere`, `ntp-servers-edge`, `ntp-servers-maas`, each with `-says` | 0 | #243: the route refuses a server that is not an address or a name (`ntp-servers`). No result yet: the lab has no cloud account of vSphere, Edge, or MAAS (#409). |

## route-bodies.sh

No cluster, and no cost that stays. It writes the bodies that
[`validate-route.sh`](#validate-routesh) needs for five cases on AWS. The fault
of four cases is in a profile, and the fault of `cidr-overlap` needs a VPC. The
script needs the key, the release profile of the live example on AWS, and the
cloud account of `terraform.tfvars`.

A case body is the clean body of a create on AWS with one more profile. The
script makes that profile through the API, with the name `strata-route-<case>`,
and publishes it. It creates no content in a registry.

A body names its profile. A body that names a deleted profile fails with
`GET /v1/clusterprofiles/<uid> failed with HTTP 404`, and Palette gives
`ResourceNotFound`. So the profiles must live until `validate-route.sh` has sent
each body.

The script runs `validate-route.sh` on the bodies itself. The results of that run go
under `validate-route/` in the results of the script. Then the script deletes each
profile and the body of each case. It does the same when a step fails, when a check
of `validate-route.sh` fails, and when a signal stops it. A later run of
`validate-route.sh` on the directory skips the four cases, because no body of a case
stays. `aws-clean.json` and `cidr-overlap.json` stay, because they name the release
profile alone.

| Case | The profile |
|---|---|
| `pack-state-disabled` | An add-on profile with `gloo` 1.2.18, which carries the mark `disabled`. |
| `pack-state-deleted` | An add-on profile with `generic-vm-libvirt` 1.0.0, which is `deleted` in `Public Repo`. |
| `profile-cloud-mismatch` | An add-on profile of the cloud type `gcp`, with `cert-manager` 1.21.1. |
| `pack-cloud-support` | An add-on profile with a pack in the layer `addon` whose cloud types lack `aws`. The script reads the pack listing of `Public Repo` for it, or takes `PACK_CLOUD_SUPPORT=<pack>@<version>`. |

Palette can refuse the profile of a case, and the registry can lack the pack.
The create of a profile refuses the two packs of `pack-state`:

- `POST /v1/clusterprofiles` with `gloo` 1.2.18 answers HTTP 500,
  `DisabledPacksForbidden`, `Disabled packs 'gloo' are forbidden`.
- The same request with `generic-vm-libvirt` 1.0.0 answers HTTP 500,
  `ClusterProfileInvalidPackState`, with a message that ends
  `pack generic-vm-libvirt:1.0.0 is deleted`.

See [Pack states](../internals/palette/packs.md#pack-states). So no new profile
can hold a disabled pack or a deleted pack. Only a profile that holds the pack
before Palette disables or deletes it can carry it. The two cases of `pack-state`
stay skipped in the live example. The script skips a case when Palette refuses its
profile, and `steps.log` names the case with the refusal. No body of that case
stays in the directory, and `validate-route.sh` skips it.

One more case, `cidr-overlap`, makes no profile (#816). Its body is
`aws-clean.json` with a VPC from `GET /v1/clouds/aws/regions/{region}/vpcs`. It
is the first VPC of the region with a CIDR and a subnet in each zone of the
pools. Each pool names the subnet of that VPC in each of its zones. The route
refuses a pool with no subnet with `RequiredParam` (#824). The body also holds
the values of the Kubernetes pack of the release profile, with the CIDR of the
VPC as the pod CIDR.

The script sends the body to the route with `api.sh post-status`, and records
the status and the code. Then `validate-route.sh` sends it through
`palette-validate`. The script skips the case when Palette lists no VPC with a
CIDR, or when the Kubernetes pack has no `podCIDR`. It also skips the case when
no VPC with a CIDR has a subnet in each zone of the pools.

| Check | Expected exit | What it shows |
|---|---|---|
| `aws-clean-body` | 0 | #408: `aws-clean.json` names the cloud account of the lab and the release profile. |
| `pack-state-disabled-body`, `pack-state-deleted-body`, `profile-cloud-mismatch-body`, `pack-cloud-support-body` | 0 | #408: Palette takes the profile of the case, and the body holds the release profile and that profile. A case that is skipped has no check. |
| `cidr-overlap-body` | 0 | #816, #824: `cidr-overlap.json` names a VPC of the region, and each pool names a subnet of that VPC in each of its zones. The Kubernetes pack of the release profile has the CIDR of that VPC as its pod CIDR. A case that is skipped has no check. A live run of the first body, with no subnet for the pools, passed. A live run of the body with the subnets passes. |
| `cidr-overlap-status`, `cidr-overlap-code` | 0, 0 | #816: the route answers the body of `cidr-overlap`, and `cidr-overlap-status.out` holds the status and the answer. The route refuses the body with HTTP `500` and `K8SCidrOverlap`: the code of Palette shows it. A live run of the first body, with no subnet for the pools, gave `0` and `1`. The route refused that body with HTTP `500` and `RequiredParam`: `Parameter 'Subnets' is required` (#824). A live run of the body with the subnets gives `0` and `0`: HTTP `500` and `K8SCidrOverlap`. |
| `validate-route` | 0 | #413: `validate-route.sh` exits 0 on the bodies, while the profiles of the cases are in the tenant. Its checks are those of [`validate-route.sh`](#validate-routesh), in `validate-route/steps.log`. |
| `pack-state-disabled-deleted`, `pack-state-deleted-deleted`, `profile-cloud-mismatch-deleted`, `pack-cloud-support-deleted` | 0 | #408, #413: the script deletes the profile of each case that it made, after `validate-route.sh` has run. |

## export.sh

No cluster, no profile of its own, and no cost. Each request is a `GET`. It
reads the export and the download of three versions of the example:

- the release version, with four packs from the Public Repo
- the registries version, with a Helm pack and an OCI pack
- the add-on version, with a manifest pack and a variable

Give other versions as `PROFILES="<name>@<version> ..."`.
[The export of a version](../internals/palette/profiles.md#the-export-of-a-version)
states what it shows.
[The download of a version](../internals/palette/profiles.md#the-download-of-a-version)
states what stage 4 checks.

| Check | Expected exit | What it shows |
|---|---|---|
| `export-<name>` | 0 | #156: `GET /v1/clusterprofiles/{uid}/export?format=yaml` gives the file of the version |
| `export-record-<name>`, `export-same-<name>` | 0 | #156: `snapshot --from-candidate` reads the export, and `same-release` passes it with the record of the version |
| `export-root-<name>` | 0 | #156: the record of the export of the release version and of the registries version has the root of the version |
| `export-minted-<name>` | 0 | #156: the export of a version with a manifest pack differs only by the pack uid that Palette mints for that pack |
| `export-json-<name>`, `export-json-record-<name>`, `export-json-root-<name>` | 0 | #156: the export with `format=json` gives the same root |
| `export-accepts-json`, `export-accepts-json-no-file` | 0 | #156: to a request with `Accept: application/json`, Palette sends `{"Reader":{}}`, and not the file |
| `export-limit` | ? | #156: three exports with no wait between them. In two of four runs, Palette refused one or more with HTTP 429, `TooManyRequests`, "only 1 request in 10 seconds is allowed". In the other two, it refused none |
| `download-<name>` | 0 | #751: `GET /v1/clusterprofiles/{uid}/spc/download` gives the file of the version to a request that accepts a file. |
| `download-layout-<name>` | 0 | #751: the one `metadata.json` at the top names the list of the packs. The list names each pack of the record with a registry at its version, and no manifest pack by its name. |
| `download-archive-<name>` | 0 | #751: each archive of a pack in the list has one top folder, and its `pack.json` names the version of its entry. The check does not read a chart of a Helm registry: `download-chart` reads it. |
| `download-top-folders` | ? | #815: the top folders of each archive of a pack in each download, a Helm chart too. The check records them, and judges none. In one run, `download-archive-<name>` shows `<name>-<first 12 hex digits of the digest>` for each pack of the Public Repo. It shows `<name>-<version>` for the OCI pack of the lab OCI registry of packs. In one run, it recorded no folder for the chart of the registries version: the file was no archive, see `download-chart`. The folder of a chart has no result yet: no run read a chart. |
| `download-no-archive` | ? | #751: the name, the type, and `isRegistryPrivate` of each entry with no archive. The check records the entries, and judges none. In one run, it recorded no entry. Each pack of a registry in the three versions has an archive, and the list names the manifest pack only by its manifest. The lab OCI registry of packs is private, and its OCI pack has an archive too. |
| `download-chart` | ? | #817, #828: one line for each chart of a Helm registry with a file in a list. A chart has one top folder and no `pack.json`, and the line `version` of its `Chart.yaml` names the version of its entry. For a file that is no archive, the line gives its size and its start. The check exits 1 when a file is not such a chart, or when no list holds a chart. The registries version pins the chart `nginx` 25.1.5 of Bitnami, a registry that serves charts without credentials. In one run, Palette answered HTTP 200, and the file `packs/nginx/nginx-25.1.5.tgz` held an XML error of 263 bytes with the code `AccessDenied`. |
| `download-chart-reason` | ? | #828: what `same-release --allow renumber` gives for each chart of a Helm registry. A copy of each record that holds a chart names each chart at another version, with other hashes. Strata reads the download of the version once, and reads the file of the chart of the record. The check records `archivesNotReadable` and `archivesNotCompared` of the pair, and exits 1 when the pair does not pass. A live run gives 1 for the Helm chart of the registries version. `archivesNotReadable` names `addon/nginx` 25.1.5, and its reason starts with `263 bytes of text: <?xml`. |
| `download-accepts-json`, `download-accepts-json-no-file` | 0 | #751: to a request with `Accept: application/json`, Palette sends `{"Reader":{}}`, and not the file, as for the export. |
| `download-limit` | ? | #751, #815: two downloads of one version at once. The check starts the two before it waits for either. It records the HTTP status of each, and the code and the message of a refusal. Exit 2 when Palette refused one or both. A run can see no refusal. A live run gives HTTP `200` for each of the two downloads. |

## registry-kinds.sh

Read-write in the registries of its own host alone, and it needs no cluster. It
sets `registry_kinds`, which adds a host in AWS. The host serves a Helm
repository, a Helm repository with credentials, an OCI registry with basic
auth, and a Spectro pack registry. The variable also adds a Palette registry
over each. The script removes each at its end.
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
| `auth-candidate`, `auth-candidate-minted` | 0 | #154: a candidate with the chart builds a record before any profile pins it. The record has no pack uid and no digest. It has the layer `addon`, the type `helm`, and the tag as the version |
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
API, and removes it. It needs the example at its baseline with cluster A.

- Stage 5 changes the role bindings of cluster A through the API, and then restores
  them. It needs no cluster B: `tiers.sh 5` runs it alone.
- Stage 6 only reads. It needs a second key in `STRATA_LIVE_LIMITED_KEY`, the
  key of an active user with no role in the project. It runs only as
  `tiers.sh 6`. With no key set, it says `could not check` and exits `2`.
- Stage 7 sets both overrides on the worker pool of cluster A through the API,
  and then restores the pool. Each override is a YAML comment, so no machine
  changes. It needs no cluster B. It runs only as `tiers.sh 7`.
- Stage 8 only reads. It needs a third key in `STRATA_LIVE_NARROW_KEY`. Its
  user has a role in the project that does not give `cluster.get`, such as
  Cluster Profile Viewer. It runs only as `tiers.sh 8`. With no key set, it
  says `could not check` and exits `2`. The stage also runs `strata list` with
  that key (#620).
- Stage 9 only reads. It makes a key of 400 characters that no tenant has, so
  it needs no other key. It runs only as `tiers.sh 9`.

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
| `rbac-record`, `rbac-in-record` | 0 | #106: the `policy` tier of a record of cluster A holds the binding. It has the type `ClusterRoleBinding`, the role `ClusterRole` `view`, and the subject `barney@example.com` of type `User` |
| `rbac-edit` | 0 | #106: the subject of the binding becomes `fred@example.com` |
| `rbac-compare`, `rbac-compare-policy`, `rbac-compare-says` | 0 | #106: `compare` of the record with cluster A exits `0` with the root `IDENTICAL`, and names `policy.rbacs[0].subjects[name=fred@example.com]` |
| `rbac-verify`, `rbac-verify-says` | 0 | #106: `verify` of the record exits `0`, and names `policy CHANGED` |
| `rbac-undo`, `rbac-after-read`, `rbac-after` | 0 | the role bindings of cluster A are as before the stage |
| `limited-control` | 0 | #276: the key of the example reads the role bindings of cluster A |
| `limited-read`, `limited-status` | 0, 0 | #276: the second key does not read them: Palette answers HTTP 500, and not 403. `api.sh get-status` prints the status and the body, and exits `0`. No result yet for this form of the check: the earlier form used `api.sh get`, which exits `2`. |
| `limited-code`, `limited-code-says`, `limited-message` | ?, 0, 0 | #276: the body holds the code `ResourceAccessDenied` and the message `Project '<project uid>' access denied`. The checks read the body that `limited-read` printed after its status line. |
| `limited-record`, `limited-record-errors` | ? | #276: what strata makes of the answer. This is a record of cluster A that it reads with the second key, and the `errors` of its policy tier. Live, `strata snapshot` of cluster A with the second key writes no record. It exits `2`, and it prints the answer of Palette: HTTP 500, `ResourceAccessDenied`, `Project '<project uid>' access denied`. So no record holds `errors`, and `limited-record-errors` has no file to read (exit `2`). |
| `pool-route`, `pool-before`, `pool-before-none` | 0 | #515: the route of the cloud config of cluster A, and a record of cluster A before the stage. The worker pool holds neither override, because Palette omits a field with no value. |
| `pool-world`, `pool-entity`, `pool-body` | ?, 0, 0 | #515: `strata preflight` with the static tier and `--world` writes `entity.json` of cluster A. `pool-world` records its exit, which is 0, and judges none. The worker pool in it is the body of the PUT, with both overrides added to its `poolConfig`. |
| `pool-set`, `pool-read`, `pool-in-cloud-config` | 0 | #515: the PUT of the worker pool to `/v1/cloudconfigs/<cloud type>/<config uid>/machinePools/worker-pool` takes both overrides. The cloud config then holds each as a field of the pool in `spec.machinePoolConfig`. |
| `pool-cluster-level` | ? | #515: the values of `overrideClusterAPIConfig` and `overrideHealthCheckConfiguration` in `spec.clusterConfig` after the PUT. The check judges none, and records 0. Both values are `null`: an override of a pool does not reach the cloud part of the cluster. |
| `pool-record`, `pool-in-record` | 0 | #515: the `shape` tier of a new record keeps both overrides on the worker pool. It keeps them on no other pool and not in the cloud part. |
| `pool-compare`, `pool-compare-json`, `pool-compare-shape`, `pool-compare-says` | 0 | #515: `compare` of the record from before with the new record exits `0`. The `shape` tier differs at both overrides of the worker pool, and the report names `+ shape.pools[name=worker-pool].overrideHealthCheckConfiguration`. |
| `pool-verify`, `pool-verify-says` | 0 | #515: `verify` of the record from before exits `0` and names `shape CHANGED`. |
| `pool-undo`, `pool-after-read`, `pool-undone` | 0 | #515: the PUT of the worker pool as before the stage. The cloud config then holds neither override. |
| `pool-restored`, `pool-restored-says` | 0 | #515: `compare` of the record from before with cluster A exits `0` and names no change of the `shape` tier. |
| `narrow-control` | 0 | #665: the key of the example reads the role bindings of cluster A. No result yet: no live window ran stage 8. |
| `narrow-rbacs`, `narrow-namespaces`, `narrow-repave` | 0 | #665: the third key reads `config/rbacs`, `config/namespaces`, and `repave/status` of cluster A. `api.sh get-status` prints the status and the body of each answer, and exits `0`. No result yet: no live window ran stage 8. |
| `narrow-rbacs-code`, `narrow-namespaces-code`, `narrow-repave-code` | ? | #665: the error code of each answer. No result yet: no live window ran stage 8. |
| `narrow-rbacs-says`, `narrow-namespaces-says`, `narrow-repave-says` | 0 | #665: each answer is HTTP 500 with the code `OperationForbidden`, and its message names `'cluster.get'`. The code of Palette shows this answer, and the stand-in gives it. No result yet: no live window ran stage 8. |
| `narrow-list` | 2 | #620: `strata list` with the third key, in the project. It prints what it read, and exits `2` because it could not read the clusters. The run keeps its progress notes. No result yet: no live window ran stage 8. |
| `narrow-list-profiles` | 0 | #620: the listing holds the release profile, because the role gives `clusterProfile.list`. No result yet: no live window ran stage 8. |
| `narrow-list-says` | 0 | #620: the listing has the line `clusters: not read (...)` with the answer of Palette to the cluster listing: HTTP 500 with the code `ResourceAccessDenied`. The code of Palette shows this answer for a role in the project without `cluster.list`, and the stand-in gives it. No result yet: no live window ran stage 8. |
| `narrow-list-once` | 1 | #620: standard error of `narrow-list` holds no line `retry`, so strata asked for the clusters one time. No result yet: no live window ran stage 8. |

`spcHash` is its own for each cluster, so `compare` now reads `spcInfraHash`
alone between two clusters. On the two records of the run, `compare` then says
`cluster tiers unchanged`, and after the change names `policy.timezone` and
`labels.team` alone. The `verify` of B also names `applied CHANGED`: the change
of the timezone and the labels changed the `spcHash` of B.

No run makes a cluster run another pack version than its profile declares; the
book shows it on the stand-in.

Stage 5 reads the role bindings of cluster A. No script reads the namespaces
of a cluster with a key that can read them: stage 8 reads them only with a key
that cannot. Read-only requests to the tenant show the other facts
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
| `filters-project`, `filters-project-says`, `filter-project`, `filter-project-says` | 0 | #539: the saved filters belong to the tenant. `api.sh get-status` asks the listing with `filterType=tag`, and the filter by its uid, with the project of cluster A. Palette answers HTTP 403 with the code `ProjectUidIsNotEmpty` for both. Each check passes. |
| `set`, `set-files`, `set-record` | 0 | #80: `snapshot filter:<name> --dir --prune` writes one record, of cluster A, with the label. It removes the record of a cluster that left the set. It leaves a file that is not a record |
| `set-tag` | 0 | #80: `snapshot tag:env=prod --dir` does the same |
| `set-verify` | 0 | #80: the record that the set wrote verifies |
| `unlabel`, `set-again`, `set-removed` | 0 | #80: without the label, the next run removes the record of cluster A |
| `filter-delete`, `baseline` | 0 | the filter is deleted, and the example is at its baseline |

## upstream.sh

Read-write: it pushes three versions of the example's own pack into its own OCI
registry, one of them between two runs. It opens and closes issues of this
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
| `edit`, `edit-host-verify` | 0, 0 | #81: after an edit in place of the manifest of the add-on version, `verify` of the record of the edge host passes. The host keeps its own copy |
| `edit-host-vs-version` | 1 | #81: the version differs from the copy of the host |
| `edit-host-own-uid`, `edit-host-copy` | 0 | #81: the copy has a uid of its own, and its route gives the content from before the edit |
| `edit-host-raw`, `edit-host-manifest-uid` | 0 | #78: the template of the edge host names the manifest by its uid, with no content |
| `release-register`, `release-host-record`, `release-edit`, `release-host-verify` | 0 | #78: a second host bound to the release version keeps its copy of the values after an edit in place of the reclaim policy. `verify` of its record is OK |
| `release-host-raw`, `release-host-kept` | 0 | #78: the host holds `reclaimPolicy: Delete`, and the profile holds `Retain` |
| `release-undo`, `release-delete` | 0 | the edit is put back, and the second host deleted |
| `delete`, `after`, `after-no-host` | 0, 1, 0 | #78: after the delete, `consumers` no longer counts the edge host |

An edge host with no device is `unpaired`. No run pairs a device.

## cluster-group.sh

Read-write: it sets `cluster_group`, which builds a cluster group on cluster A
that pins the add-on version. The variable also builds a virtual cluster in the
group. The script edits the add-on version in place and back. It changes the
CPU limit of the group through OpenTofu and back, and removes the group. It
needs the example at its baseline with cluster A.

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
| `member-listed`, `member-listed-group` | 0 | #116: `list clusters --json` names the virtual cluster beside cluster A, with the cloud type `nested`. `virtual.clusterGroup` is the group, `virtual.hostCluster` is cluster A, and `includesVirtual` is true |
| `member-by-name` | 0 | #116: `cluster:<name>` of the virtual cluster resolves to its uid through the search |
| `search-profile`, `search-profile-runs` | 0 | #181: the cluster search with `clusterProfiles eq <add-on name>` answers cluster A, which runs the add-on version, and not the virtual cluster |
| `search-nested`, `search-nested-misses-member` | 0 | #184: the same search with `environment eq nested` does not answer the virtual cluster of the group, which runs the add-on version through its group |
| `member-consumer-search` | 0 | #149: `consumers` does its second reading through the search |
| `member-consumer-group`, `group-virtual-clusters` | 0 | #184: `consumers` finds the group that pins the add-on version, and the virtual clusters of the group, which it reads, name the virtual cluster |
| `edit`, `edit-group-verify` | 0, 0 | #81: after an edit in place of the manifest of the add-on version, `verify` of the record of the group passes. The group keeps its own copy |
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

The virtual cluster runs with a CPU limit of 3 cores. The memory limit leaves
1 GiB beside the 3.2 GiB that the agents of Palette take in it.

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
| `order-a-record`, `order-a-params`, `order-b-record`, `order-b-params` | 0 | #112: each record holds the orders in `packs[].params.installOrder`, and lists the tiers by them. The first lists `tier1` then `tier2`. The second lists `tier2` then `tier1` |
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

The everyday loop, end to end: [One change, start to end](../walkthrough.md) tells it. The
records are in a git repository that the script builds, and every script of CI
runs in it. It changes cluster A and the add-on version through the API, and
undoes each change. Stages 2 to 4 only ask Palette: the questions and the body
of a create change nothing in the tenant. `loop.sh N` resumes at stage N. It
needs the example at its baseline with cluster A.

| Check | Expected exit | What it shows |
|---|---|---|
| `record-release`, `record-addon`, `record-cluster`, `commit-main` | 0 | main records the release version, the add-on version, and cluster A |
| `cluster-runs-release` | 0 | #81: cluster A runs the recorded release |
| `candidate`, `candidate-record` | 0 | #81: the change: the next version as a candidate file, and as a record before Palette has it |
| `ask-candidate` | 0 | #385: `snapshot --from-candidate --validate` asks Palette if it accepts the packs of the candidate, under a name that the project lacks. The record holds the answer. |
| `comment`, `comment-says` | 0 | #81: `pr-comment.sh` writes the comment of the pull request, with the change and its impact |
| `commit-branch`, `merge-gate` | 0 | #81: the merge gate is clear: the records after the merge verify |
| `record-next`, `same-release`, `commit-publish` | 0 | #81: Palette holds the content that the reviewer read, by the root |
| `behind-profile`, `behind-profile-names` | 1, 0 | #707: after the publish of the next version, `behind` on the records of the release and of cluster A exits `1`. One entry of `profiles` holds the release for the two records, and its newer versions hold the next version. |
| `behind-carrier` | 0 | #709: the next version keeps the version of each pack. So each pin of the release that is behind has one carrier entry for the release. The entry has no carrier and no error. When no pin of the release is behind, the check holds no pin. A live run passes. |
| `behind-profile-move`, `compare-profile-move`, `behind-profile-move-says` | 1, 1, 0 | #708: with the impact, `behind` on the record of the release exits `1`, and the bump of the release names the next version. Its impact level is the `maxImpact` of `compare` of the two versions. Its flags are the flags of the packs of that compare, and its packs are the packs that the compare changes, adds, or removes. A live run passes these checks. |
| `same-release-renumber` | 0 | #730: `same-release --allow renumber` passes the same publish, and the pair names no renumber: no pack moved its version. |
| `ask-repave`, `ask-repave-answer` | 1, 0 | #385: `compare --repave` asks Palette if the move of cluster A to the next version repaves. The command exits `1` because the roots differ, and the report holds `paletteRepave.required`. |
| `ask-validate`, `ask-validate-answer` | 1, 0 | #385: `compare --validate` asks Palette if it accepts the packs of the same move. The report holds `paletteValidation.accepts`. |
| `cluster-plain` | 0 | #615: cluster A runs the release and the add-on. `compare` of the record of the release with `cluster:<uid>` holds the release alone, so the roots are equal. |
| `ask-repave-plain`, `ask-repave-plain-says` | 1, 0 | #615: `compare cluster:<uid> <next> --repave` asks Palette about the move of the release. The cluster side is `cluster:<uid>/<release>`, and the report names no pack of the add-on. |
| `ask-repave-comment`, `ask-repave-comment-says` | 0 | #157: `pr-comment.sh --repave` asks Palette about the same move. For the move of the CSI pack, Palette answers no repave. The comment holds `**Palette:** no repave`, and the note on standard error says `label repave: Palette says no repave; would remove label repave if present`. |
| `build-body`, `preflight-static`, `preflight-live` | 0 | #385: the body of a create on the release, in the cloud account of the lab. `preflight` finds no fault in it in the static tier with no request. It finds none in the tier of Palette, which calls the validate routes and creates nothing. |
| `preflight-comment`, `preflight-comment-says` | 0 | #158: `pr-comment.sh --preflight` posts the markdown report of the static tier as the comment of the pull request. The comment starts with `<!-- strata-preflight -->` and holds the verdict of the static tier. On aws, no rule of the static tier is a preflight check, so the verdict is `**NOT COUNTED: no check counted on aws**` (#619). |
| `preflight-job`, `preflight-job-says` | 0 | #618: `preflight.sh --offline` runs the static tier of the body, as the job of a pull request. The first line of its log is `preflight: the run is offline, so it reads nothing from Palette: --offline`. The report says `tiers static, offline`. |
| `preflight-job-key`, `preflight-job-key-says` | 0 | #618: with the key of the lab where strata finds it, `preflight.sh` reads Palette. The log has no line that says that the run is offline, and the report says `tiers static, palette`. |
| `preflight-each` | 0 | #144: `template-outside` makes one request for each profile of the body, here one, and `requests` of the report holds it |
| `preflight-offering` | 0 | #143: `instance-offering` asks the instance type route of the region with the account of the body. It makes one request for all its pools. The check passes. The type of the release body is offered in its zone |
| `preflight-key-pair` | 0 | #433: the rule that reads the key pair of the body passes. On AWS it is `iaas-pod-identity`, which sends the key pair to the property route. On EKS it is `ssh-key-pair`. A body on GCP or GKE has no key pair, and the check does not run. |
| `preflight-fault` | 1 | #385: a body with a cloud account that the project lacks fails the gate `--strict`. |
| `nightly` | 0 | #81: `verify-report.sh` finds every record unchanged |
| `fleet`, `fleet-has-a` | 0 | #81: the fleet of a label holds cluster A |
| `upstream` | 1 | #81: `upstream-report.sh` finds pins that the registry has passed |
| `upstream-asks`, `upstream-asks-says` | 1, 0 | #166: `upstream-report.sh` with the impact asks Palette about the bump of each pin that is behind. The bump column of an issue holds the answer. |
| `override`, `override-record`, `override-not-behind` | 0 | #81: a value overridden on cluster A opens no notification: Palette says that no update waits |
| `override-drift` | 1 | #81: the record of cluster A reports the drift |
| `override-release` | 0 | #81: the records of the versions verify: the profiles did not change |
| `override-undo`, `override-gone` | 0 | the override is undone, and the record of cluster A verifies again |
| `edit` | 0 | #81: a person edits the add-on version in place, through the API |
| `nightly-drift`, `nightly-names` | 1, 0 | #81: the nightly job opens an issue for the add-on record |
| `who`, `who-says` | 1, 0 | #81: `--who` names a person who wrote the edit |
| `nightly-who`, `nightly-who-says` | 1, 0 | #158: `verify-report.sh --who` puts the audit log in the issue of the add-on record. The body holds `who: 1 write in the audit log since the record`. It names the last write, an update by a user of the tenant. |
| `cluster-kept`, `behind-record`, `behind` | 0 | #81: cluster A keeps its copy, and Palette says that an update waits for it |
| `edit-undo`, `edit-gone` | 0 | the edit is undone, and the add-on record verifies again |
| `incident-list`, `incident-list-has` | 0 | #81: `list --pack` finds the release among the versions that carry the CSI pack |
| `incident-consumers`, `incident-compare` | 1, 0 | #81: cluster A runs the release, and exactly its content |
| `retire` | 0 | #81: nothing runs the next version: the retire gate is open |
| `report-log` | 0 | #396: `strata report` reads the log of the unknown values that this run wrote. The script sets `XDG_STATE_HOME` under the run directory, so the log holds this run alone. The command exits `0` when each line is a line of `strata-unknown/1`. The check passes. |
| `baseline` | 0 | the example is at its baseline |

## observer-kind.sh

Read-only for the tenant, except stage 8, which edits the add-on version in place
and puts it back. It makes a kind cluster, installs [the
observer](./observer-deploy.md) in it, and deletes the cluster. It needs the
example at its baseline with cluster A, `docker`, `kind`, `kubectl`, and the image
of `just observer-image`. The observer watches cluster A, and its records are the
release profile and the add-on profile. The key goes to the Secret through standard
input, and it is in no log. `observer-kind.sh N` resumes at stage N, and
`KEEP_CLUSTER=1` keeps the kind cluster.

The image has no shell, so the checks of the pod read its spec, and the kubelet
enforces the spec. The check of the refusal needs a kind that enforces network
policy. The observer also listens on `127.0.0.1` only, so the connection of another
pod fails in both cases. Each check passes against the real tenant, with cluster A
up.

Stage 7 (#646) shows how pods reach a mirror. It starts a registry in Docker on
the network of kind. It points containerd on each node at that registry for the
host `vendor.example`. One pod names `vendor.example`, as a pack does. Another
pod names the registry itself, as a rule that rewrites the image string does.

A node of kind reads the folder `certs.d` only when the config of the cluster
sets `config_path`. So stage 1 creates the cluster with that config.

Then the kustomization `examples/observer-pods` applies. It lists the base and
the component `deploy/observer/pods`, so the observer reads the pods. The checks
`mirror-image-id` and `mirror-image-string` print what each pod
reports and the row that the observer shows for it. They show whether the image
string or the image id decides which match works. The stage removes the pods, the
files of containerd, and the registry.

A mirror at the level of the node keeps the
name of the pack in `spec.image` and in `imageID`. A rewritten image string names the
mirror in both, so it matches no pack by name and tag. See [The pods of a
cluster](../internals/palette/clusters.md#the-pods-of-a-cluster).

Stage 8 (#648) shows the alerts. It starts a receiver in the cluster: a pod of
`busybox` that takes a `POST` on port 443 and keeps the body of each message. The
network policy of the observer lets it connect to port 443, so a message reaches the
receiver. The component `deploy/observer/alerts` and the file of
`examples/observer-alerts` apply, with `--alerts-allow-http` and a content cycle of
one minute. An edit in place of the add-on version opens a finding, and the undo clears it.
The script puts the edit back at its end, also when a check fails.

| Check | Expected exit | What it shows |
|---|---|---|
| `kind-create`, `image-load` | 0 | #644: a kind cluster, and the image of the observer in its nodes. The config of the cluster sets `config_path`, so containerd reads the folder `certs.d` |
| `namespace`, `config`, `key` | 0 | #644: the namespace with the restricted standard, the ConfigMap, and the Secret of the key |
| `record-release`, `record-addon` | 0 | #644: a record of each profile that cluster A runs |
| `records-apply`, `records-four`, `records-meta` | 0 | #644: the output of `observer-records.sh --revision` applies with `--server-side`. It makes four Secrets, and `meta.json` says `approved` with the revision |
| `manifests-apply`, `rollout` | 0 | #644: `deploy/observer` applies with `--server-side` under the restricted standard, and the Deployment becomes available |
| `pod-ready`, `pod-ready-status` | 0 | #644: the pod is Ready, and its container has not restarted |
| `pod-non-root`, `pod-read-only-root` | 0 | #644: the pod runs as non-root and has a read-only root file system with no capability |
| `pod-no-token` | 0 | #644: no volume and no mount holds a token of the service account |
| `reader-role`, `reader-can-forward` | 0 | #644: the reader role, bound to an account, lets that account open a port-forward |
| `reader-no-secret`, `reader-no-exec` | 1 | #644: the same account can read no Secret and run no exec |
| `reader-forward`, `reader-health`, `reader-page` | 0 | #644: `kubectl port-forward deploy/strata-observer` works for that account, and the page answers |
| `reader-cluster` | 0 | #644: after the first cycle, the summary names cluster A and its two approved records |
| `other-pod-refused` | 0 | #644: a pod of another namespace cannot connect to the page |
| `first-seen-apply`, `first-seen-meta` | 0 | #644: the output of `observer-records.sh --first-seen` applies with `--server-side`, and `meta.json` says `first-seen` |
| `mirror-registry`, `mirror-nodes`, `mirror-pods` | 0 | #646: a registry on the network of kind holds `product:1.0`. Containerd on each node asks it first for `vendor.example`. A pod that names `vendor.example` and a pod that names the registry are Ready |
| `component-apply`, `component-read` | 0 | #646: `examples/observer-pods` applies, and `/v1/images` says `read`. The token, the role, and the policy of the component work on kind |
| `mirror-image-id` | ? | #646: the pod that names `vendor.example`. The output has `spec.image`, `status.image`, `imageID`, and the row of the observer. The check records 0, so the row holds a digest. `spec.image` and `imageID` name `vendor.example`, and the digest is the one that the mirror served. `status.image` names `registry.k8s.io/pause:3.10` |
| `mirror-image-string` | ? | #646: the pod that names the registry. The output has the same four items. The check records 0, so the row holds a digest. `spec.image` and `imageID` name the registry, so the image matches no pack by name and tag. `status.image` names `registry.k8s.io/pause:3.10` |
| `mirror-clean` | 0 | #646: the pods, the files of containerd, and the registry go |
| `alert-receiver`, `alert-secret` | 0 | #648: a pod takes a `POST` at `/cgi-bin/hook` on port 443 and keeps each body. The Secret `strata-observer-alert-channels` holds the address of the receiver for each channel. |
| `alert-apply` | 0 | #648: the base, the component `deploy/observer/alerts`, and the file of `examples/observer-alerts` apply with `--server-side`. The Deployment becomes available, and the render holds the flag `--alerts`. |
| `alert-route` | 0 | #648: `/v1/alerts` says `set`, and it lists five rules with their states. It names the channels and shows no address. |
| `alert-edit` | 0 | #648: an edit in place of the add-on version opens the finding `published-changed`. |
| `alert-open` | 0 | #648: the receiver gets one message of the open. It starts with the name of the cluster and `Drift`, and it names the profile and its packs. |
| `alert-no-value` | 0 | #648: the detail route holds the text of the edit, so the search proves something. The receiver holds none of it, in the raw form or in the JSON form. It holds none of the text that the edit replaced. |
| `alert-undo`, `alert-clear` | 0 | #648: the add-on version goes back, and the observer finds no finding. The receiver then gets a message of the clear. |
| `alert-one-each` | 0 | #648: after one more content cycle, the receiver holds one message of the open and one of the clear. |
| `alert-clean` | 0 | #648: the receiver, its Service, and its ConfigMap go. |
| `kind-delete` | 0 | the kind cluster is deleted |

## observer-images.sh

Read-write. It runs [the observer](./observer-deploy.md) on cluster A with the
component `deploy/observer/pods`, and it reads the digest of each pod image. It
needs the example at its baseline with cluster A, which is billable until `just
live-down`. It also needs the key, `crane`, `kubectl`, `curl`, `jq`, and `git`.
`OBSERVER_IMAGE` names the image of the observer. That image must hold the code
of the commit that you test.

The image of the observer is on a public package of GitHub. The nodes of cluster
A pull it with no pull Secret. The script never tags it, never pushes it, and
never copies it to another registry, so the pod runs the image that CI built.
`no-pull-secret` makes sure that the pod names no pull Secret.

A throwaway tag on `ttl.sh` holds one image, and then another. These images are
copies of public images. The names of the images hold a random suffix, and
`ttl.sh` removes an image two hours after its push. A copy with `crane copy`
keeps the digest of the index of a multi-arch image.

The only write to the tenant is an edit in place of the values of one pack of the
add-on. The values list the images of the run, so that a pack names them. The
observer makes a finding only for an image that a pack names. The undo puts the
values back. The script applies no update to cluster A.

Every pod of the run pulls with `Always`, so a new pod pulls the tag again. The
undo removes the pods, the observer, the role, the binding, and the namespaces,
also after a failed check or an error. The script deletes no cluster. `KEEP=1`
skips the undo, so that `observer-images.sh N` can resume the run at stage N. A
check with `?` records its result without judging it.

The check `image-agents-no-finding` takes the images from the answer of the page.
It looks at each image with the match `none`. A cluster that Palette builds runs
agents of Palette, and no pack names them. The check fails when the page shows no
such image, or when a finding names one. It uses no list of namespaces.

| Check | Expected exit | What it shows |
|---|---|---|
| `tag-first`, `values-edit` | 0 | #646: the tag on `ttl.sh` holds its first image, and an image with a shell is there for the crash loop. The pack of the add-on lists the three images of the run, in place |
| `namespaces`, `settings`, `records` | 0 | #646: the namespace of the observer with the `restricted` standard, the namespace of the pods, the ConfigMap, and the Secret of the key. A record of each profile applies with `--server-side`, and the add-on record lists the tag |
| `observer-apply`, `observer-ready` | 0 | #646: the overlay lists the component and the image of `OBSERVER_IMAGE`. The Deployment becomes available |
| `no-pull-secret` | 0 | #732: the pod runs the image of `OBSERVER_IMAGE` with an image id, and its spec names no `imagePullSecrets`. So a node pulled the public image with no login. `no-pull-secret.out` shows the image, the image id, and the pull Secrets. A live run passes: the spec names no pull Secret. |
| `pods-role` | 0 | #646: the account of the observer gets and lists pods in each namespace. It cannot watch or write a pod, and it cannot read a Secret or a node |
| `pods-read` | 0 | #646: `/v1/images` says `read`, with a time and a count of images. The token, the role, and the policy of the component work on cluster A |
| `image-first` | 0 | #646: a pod runs the tag. The row holds one digest, the first digest, and it equals the digest of the registry. No finding names the tag |
| `image-split` | 0 | #646: the tag moves, and a new pod pulls it. The row says `split`, with the first digest and another digest. The finding `image-split` names the tag |
| `image-moved` | 0 | #646: the first pod is replaced, so each pod runs the second digest. The row says `moved`, and the finding `image-moved` replaces the split |
| `image-never-started` | 0 | #646: a pull that fails leaves the image id empty. The row reads Not checked, with the digest status `not-started`, and it makes no finding |
| `image-crash-loop` | 0 | #646: a container that restarted keeps its digest. The row shows that digest as the first digest, and it makes no finding |
| `image-agents-no-finding` | 0 | #646: the images that match no pack, such as those of the agents of Palette, make no finding. The answer of the page names them |
| `pull-policy` | ? | #646: a pod made with `IfNotPresent`. An admission plugin such as `AlwaysPullImages` can set `Always`. The check records 0: cluster A set `Always`, and the policy did not stay |
| `undo-values`, `undo-namespaces`, `undo-role` | 0 | #646: the pack names none of the images of the run. Both namespaces, the role, and the binding are gone |
| `undo-baseline` | 0 | the example is at its baseline |

## observer.sh

Read-write. It runs [the observer](./observer-deploy.md) on cluster A, `fred`,
in two ways. Way 1 is a plain `kubectl apply` of the rendered manifests. Way 2
is an add-on profile in Palette, with the rendered manifests as one manifest
layer. It needs the example at its baseline with cluster A Running, the key,
`kubectl`, `git`, and `go`. `OBSERVER_IMAGE` names the image.

The first cycle of the pod reads Palette from a node of cluster A. So
`reaches-palette` answers whether a workload cluster reaches the API. A failed
`reaches-palette` ends the run before stage 4, because the later stages need
the page. Its output names `lastError`, the kind and the status of the failed
cycle.

The script sets two short periods in an overlay: one minute for the content
cycle and five minutes for the versions cycle. The key goes to its Secret
through standard input. It is never in a profile, because a profile variable is
in clear in the copy of the cluster. The kubeconfig lives in a temporary file
that the script deletes.

The image of the observer is on a public package of GitHub. The nodes of cluster
A pull it with no pull Secret, and the script reads no token. `no-pull-secret`
makes sure that the pod names no pull Secret. `no-secret-in-run` searches the
files of the run for the key.

Stage 4 counts the requests of the cycles. `count-requests` is a small Go
program that sits between `strata` and Palette. It writes one line for each
request, with no query and no key. It counts the content cycle, the first command
of the versions cycle, and its moves.

Stage 4 also tests the click "Ask Palette" and the text of a diff. The pod runs in
the cluster, so the counter on the machine of the script cannot see the requests
that the pod sends to Palette. So `ask-one-post` runs the program of the
observer on that machine, built from the same checkout. It runs behind the
counter, with the records of the run and the key.

`ask-words` clicks on the pod too, through the port-forward, and records its
answer. The stage needs `gzip` for the records of the observer on the machine of
the script. `ASK_PORT` sets the local port of that observer. The default is
`PORT` plus 1.

The undo runs at the end, also after a failed check or an error. The edit in
place goes back, the profile of way 2 leaves the cluster and goes, and the
namespace goes. The script deletes no cluster. `KEEP=1` skips the undo, so that
`observer.sh N` can resume the run at stage N. A check with `?` records its
result without judging it.

Each check of #656 passes in both ways. A workload cluster reaches the API: the pod on
cluster A ends its first cycle against Palette. A profile in Palette installs
the observer too. The layer does not replace a node. When the profile leaves the
cluster, Palette deletes the Deployment.

The counts below are for cluster A, with two profiles and five packs. They grow
with the size of the tenant. A content cycle sends one more request with a
finding, because `verify --who` then reads the audit log.

| Command | At the baseline | With a finding |
|---|---|---|
| A content cycle | 12 | 13 |
| `behind`, the first command of a versions cycle | 13 | 13 |
| The moves of the first versions cycle after a start | 28 | 28 |

| Check | Expected exit | What it shows |
|---|---|---|
| `cluster-record`, `profile-records` | 0 | #656: a record of cluster A, and a record of each profile that it runs |
| `namespace`, `config`, `key` | 0 | #656: the namespace, the ConfigMap, and the Secret of the key. The key goes through standard input |
| `records-apply` | 0 | #656: the records of the Git revision apply with `--server-side` |
| `manifests-render`, `manifests-apply` | 0 | #656: `kubectl kustomize` renders the overlay, and a plain `kubectl apply -f observer.yaml` installs it |
| `apply-admitted` | 0 | #656: the namespace enforces `restricted`, and the standard admits the pod |
| `pod-ready` | 0 | #656: the Deployment is Available in 3 minutes. `strata-observer health` exits 0 through `kubectl exec` |
| `pod-hardened` | 0 | #656: the pod runs as non-root with a read-only root. It has no capability and no token volume |
| `no-pull-secret` | 0 | #732: the pod runs the image of `OBSERVER_IMAGE` with an image id, and its spec names no `imagePullSecrets`. So a node pulled the public image with no login. `no-pull-secret.out` shows the image, the image id, and the pull Secrets. A live run passes: the spec names no pull Secret. |
| `reaches-palette` | 0 | #656: `/v1/cluster` gives `page` `ok` and a `checkedAt` through `port-forward`. A workload cluster reaches the API |
| `no-finding-at-baseline` | 0 | #656: `/v1/findings` is empty for the records that the run made |
| `summary-matches-strata` | 0 | #656: the page shows the status that `strata verify` and `strata compare` give |
| `reader-role`, `reader-can-open` | 0 | #656: a user with only the reader role opens the page with `kubectl port-forward` |
| `reader-cannot-read` | 0 | #656: the same user gets `no` for `get secrets` and for `create pods --subresource=exec` |
| `nobody-cannot-open` | 0 | #656: a user with no role gets `no` for `create pods --subresource=portforward` |
| `other-pod-refused` | 0 | #656: a pod of another namespace cannot connect to the pod address on the port |
| `page-admin`, `page-admin-again` | 0 | #656: the port-forward is back, as the admin of the cluster |
| `requests-counted`, `requests-counted-drift` | ? | #656: the requests of a content cycle and of a versions cycle, at the baseline and with a finding. The files `*.out` and `*.log` hold the counts and the routes. The counts are in the table above |
| `drift-edit` | 0 | #656: an edit in place of the published add-on version |
| `drift-found`, `drift-who` | 0 | #656: within two content cycles, `/v1/findings` holds `published-changed` with the pack, and `who` names the actor |
| `cluster-copy-unchanged` | 0 | #656: `clusterCopy` of that profile still says `same`, because the cluster did not take the update |
| `variable-in-raw`, `no-value-leaves` | 0 | #656: the raw output of `strata compare` holds the value of a variable. No route and no line of the log holds it |
| `ask-one-post` | 0 | #647: the cycles of the observer send no POST to Palette. One click on a move to a newer pack version sends one POST to the validate route. A second click on the same move sends none, because the observer keeps the answer. The first click must get an answer, because the observer asks again after a request that failed. The files `ask-one-post.out` and `ask.log` hold the counts and the requests. A live run gave no POST in the cycles, one POST for the first click, and none for the second |
| `ask-words` | 0 | #647: the click on the pod gives an answer with one of three verdicts: `no-error`, `refuses`, or `no-answer`. The output records the verdict, and the code of a refusal or the status of a request that failed. The words on the page are "Palette gave no error", "Palette refuses: `<code>`", and "Palette gave no answer". The check does not judge which verdict Palette gives. It fails when the word "accepts" is in an answer of the API. A live run asked about a move of the Kubernetes pack, and the pod and this machine each gave `no-error` |
| `diff-text` | 0 | #647: the detail route `diff?side=published` of the pack that the edit changed holds the text of the edit, in `manifests[].unified` or in `values.unified`. The seven summary routes and the route of the profile hold none of that text |
| `drift-undo`, `drift-clears` | 0 | #656: the version goes back, and the next cycle clears the finding |
| `wrong-key`, `lost-after-wrong-key` | 0 | #656: a wrong key in the Secret and a restart of the pod. The page goes to `lost` after two cycles |
| `key-back`, `ok-after-key-back` | 0 | #656: the key back and a restart, and the page returns to `ok` |
| `restart-keeps-working` | 0 | #656: after `kubectl rollout restart`, the page goes to `ok`. The start of its state is later, because the state goes with the pod |
| `restart-first-load` | ? | #656: the page showed `first-load` after the restart. `pages-after-restart.txt` lists the states in order. The check records `1`: the list holds no `first-load` line |
| `first-seen-starts-again` | 0 | #656: an edit gives a finding, and a restart gives it a later `firstSeen`. The state goes with the pod |
| `manifests-delete`, `removal-is-clean` | 0 | #656: `kubectl delete -f observer.yaml` removes the namespace, and no object of the observer stays |
| `fred-unchanged` | 0 | #656: `strata verify` says that cluster A did not change |
| `profile-prepare`, `nodes-before` | 0 | #656: the namespace, the ConfigMap, the Secret of the key, and the records, made with `kubectl` before the profile. The machines of cluster A, before the layer |
| `profile-create`, `profile-attach` | 0 | #656: an add-on profile with the rendered manifests as one manifest layer goes to cluster A. The cluster is Running |
| `profile-applied` | 0 | #656: Palette applies the layer, and the pod is Ready |
| `profile-does-not-disturb` | 0 | #656: each other profile of cluster A still agrees with its record |
| `own-profile-not-checked` | 0 | #656: the profile of the observer shows as `no-record` under "Not checked", and as no finding |
| `profile-record`, `no-key-in-profile` | 0, 1 | #656: a record of the profile of the observer. A search of it for the key finds nothing |
| `no-repave` | ? | #656: cluster A keeps its machines when the layer is applied. The output lists the machines before and after. The two lists are the same, so Palette replaces no node for the layer |
| `profile-detach`, `profile-removal` | 0 | #656: the profile leaves cluster A, and the Deployment goes. Palette deletes the Deployment of a whole layer when a person removes the layer. See [An object that an edit drops from a manifest](../internals/palette/clusters.md#an-object-that-an-edit-drops-from-a-manifest) |
| `undo-profile`, `undo-namespace` | 0 | #656: no profile of way 2 stays in Palette, and the namespace is gone |
| `no-secret-in-run` | 1 | #656: no file of the run holds the key |
| `undo-baseline` | 0 | the example is at its baseline |


## observer-role.sh

It sends each request of the two cycles of [the observer](./observer-deploy.md)
twice. The first key is the key of the example. The second key belongs to a user with
the role under test. The script records the status and the error code of each answer.

It also sends the request of the click "Ask Palette" of the observer:
`POST /v1/clusterprofiles/{uid}/validate/packs` for the add-on version. That route
changes nothing.

The script then runs `verify --who`, a record of cluster A, and `behind` with both keys.
Stage 3 runs the rules of the validate route of the create on cluster A with both keys:
`palette-validate`, `worker-cpu-floor`, and `pack-dependency`. That route needs `cluster.create`:
see [The permission of the route](../internals/palette/validate-routes.md#the-permission-of-the-route).
It needs the example at its baseline with cluster A,
`OBSERVER_ROLE_KEY`, and `OBSERVER_ROLE`, the name of the role. With no key, it says
`could not check` and exits `2`.

A maintainer makes the user. The steps are in the header of the script and in [The
live example](./live-example.md). Run the script once for each role under test, such
as Project Viewer and Cluster Viewer. The checks with `?` record the answer and do not
judge it, because the run finds the answer.

A live run used both roles. Project Viewer gives every answer. Cluster Viewer gives
every answer but the audit log: see [The role that a key
needs](../internals/palette/clusters.md#the-role-that-a-key-needs).

| Check | Expected exit | What it shows |
|---|---|---|
| `control-<name>` | 0 | #645: the key of the example reads each request of the cycles |
| `role-<name>` | ? | #645: the status and the error code of the same request with the key of the role under test. The output of each check holds `HTTP`, the status, and the code. With Project Viewer, each request answers HTTP 200. With Cluster Viewer, each request answers HTTP 200 but the audit log (`role-audits`), which answers HTTP 403 with the code `AccessDenied` |
| `command-record`, `command-control-verify`, `command-control-cluster` | 0 | #645: with the key of the example, `verify --who` and two records |
| `command-role-verify`, `command-role-cluster`, `command-control-behind`, `command-role-behind` | ? | #645: the same commands with the key of the role, and `behind` with both keys. The exit code says whether strata gets an answer. Each check records 0 with both roles, so `verify`, the record of cluster A, and `behind` get an answer with Cluster Viewer too. The record that `verify` reads is unchanged, so it needs no audit log. A record that drifted still reports `DRIFT`: see [who.sh](#whosh) |
| `validate-body` | 0 | #647: the body of the validate request, made from the record of the add-on version |
| `control-validate-packs` | 0 | #647: the key of the example sends the validate request, and Palette answers 2xx |
| `role-validate-packs` | ? | #647: the status and the error code of the same request with the key of the role under test. The click of the observer sends this request. The check records the answer and does not judge it. With Project Viewer, the request answered HTTP 200 |
| `preflight-control-validate` | 0 | #619: with the key of the example, the rules of the validate route of the create give no `error` on cluster A. No result yet: the check is new, and no live window ran it |
| `preflight-role-validate` | ? | #619: the verdict and the first message of the same rules with the key of the role under test. With Project Viewer, each rule must give `error`, with HTTP 403, the code `AccessDenied`, and a message that names `cluster.create`. No result yet: the check is new, and no live window ran it |

## observer-chart.sh

Read-write. It asks whether Palette syncs the public chart of
[the observer](./observer-deploy.md) from a registry with no login (#732). Step 1 of
[Install with a profile in Palette](../automation/observer.md#install-with-a-profile-in-palette)
adds that registry with a token now. The script needs no cluster, and it changes
nothing else in the tenant. It needs the key, with a role that adds a registry at
tenant scope, `curl`, and `jq`.

The script makes an OCI registry of the type `basic`, with the provider `helm` and
the auth type `noAuth`, for `https://ghcr.io/orgs/<owner>` and the folder `charts`.
`ENDPOINT` and `BASE_PATH` give another address and folder. The name of the
registry holds a random suffix. Palette reads the address when it makes the
registry, so a refusal of that read fails the create. The output of each check
holds the answer of Palette, with its code or its message.

A status of `Completed` alone does not show that Palette read the chart. So only
`chart-no-login` shows it. The end deletes the registry, also after a failed check
or an error. The checks with `?` record what Palette does and do not judge it.

| Check | Expected exit | What it shows |
|---|---|---|
| `registry-no-login` | ? | #732: Palette makes the registry with the auth type `noAuth`. 1 says that Palette refuses the registry, and `registry-no-login.err` holds the code. A live run gives 1 for `ghcr.io`: the create answers HTTP 500 with `InvalidRegistryCredentials`. A read of the catalog of `ghcr.io` with no login answers HTTP 401. |
| `sync-no-login` | ? | #732: a sync of the registry ends with the status `Completed`. 1 says that the sync ends with another status, and the output holds the status and the message. The live run did not get to this check, because Palette made no registry. |
| `chart-no-login` | ? | #732: the packs of the registry hold the chart `strata-observer`, and the output names its version. 1 says that Palette read no chart. The live run did not get to this check, because Palette made no registry. |
| `registry-gone` | 0 | #732: the registry of the run is deleted, and a read of it answers HTTP 404. A live run gives 0: no registry stays. |

## version-states.sh

This is the other answer to a version bump that overwrites a version. Each
version is in a state of its own, a workspace of `terraform-version/`, with no
feature preview flag. It builds its own profile, attaches it to cluster A for a while, and
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
| `pool-change`, `pool-machines` | 0 | #89: another instance type for the node group, through OpenTofu, is an update in place of the cluster. Nodes of the new type run it. The cluster stays `Running` |
| `nodegroups-change` | 0 | #89: EKS holds two node groups: Palette made one of the new type and kept the old one, with its nodes |
| `nodegroups-back` | 0 | #89: the change back leaves both groups |
| `shape-2`, `shape-differs`, `shape-differs-says` | 0 | #89: compare of the records before and after names the change of the shape tier, `shape.pools[name=worker-pool].instanceType`. A tier does not change the exit |
| `pool-back`, `baseline` | 0 | the node group back, and the example at its baseline |

The old node group is one that Palette no longer tracks. Its nodes keep
running, and a delete of the cluster removes the new group alone. The EKS
control plane cannot go while the old group exists. The delete waits at
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
| `release`, `release-cloud`, `release-layers` | 0 | #90: a record of the GKE release version, of cloud type `gke`, with an OS, a Kubernetes, a CNI, and a CSI layer. The packs are `cos-gke`, `kubernetes-gke`, `cni-vpc-native-gke`, and `gke-csi-gcp-driver` |
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
the GKE cluster, and stays `Deleting`. The machine pool of GKE that has no
machine pool of the cluster API keeps its finalizer. So the delete of the
cluster does not end by itself.

The EKS change of [eks.sh](#ekssh) is of the same kind. A change of the node
pool is not a change of a layer. So the impact table gives it no level, and
`compare` shows it in the shape tier alone. The recovery is in
[A GKE cluster](./live-example.md#a-gke-cluster).

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

The test plan of #91 for one engine, with no cluster. Each fault the lab can make
fails when it must, and the clean entity passes. Each run appends to the ledger,
`crates/strata-preflight/evidence/runs.yaml`, and `strata preflight --promotion` reads it. The
checks of each run, and what each one shows, are in `summary.md` of the run.
This table names the faults.

| Engine | The faults made on purpose, by rule id |
|---|---|
| `gke` | `palette-validate` (a pool of 4), `gke-no-release-channel`, `gke-no-gpu`, `gke-autopilot`, `gcp-name-length`, `account-missing`, `instance-offering` |
| `eks` | `eks-one-zone`, `eks-worker-needed`, `update-strategy-eks`, `api-endpoint-reach`, `account-missing`, `instance-offering`, `ssh-key-pair` |
| `aws` | `worker-cpu-floor`, `palette-validate` (one worker of four cores), `api-endpoint-reach`, `account-missing`, `instance-offering`, `iaas-pod-identity` (a key pair that the region lacks) |
| `gcp` | `worker-cpu-floor`, `palette-validate`, `gcp-name-length`, `account-missing`, `instance-offering`, `gcp-properties` |

Three rules of the plan read a list of the cloud account from Palette (#143). For
`instance-offering`, the plan reads the instance type route. It takes the first
type with a zone in `nonSupportedZones`, and puts the first pool in that zone.
The fault of `ssh-key-pair` is a key pair that the account lacks, on EKS alone.
The fault of `gcp-properties` is a project that the account cannot read.

The check `instance-types-shape` expects `instanceTypes[]` with `type`, and
`nonSupportedZones` as an array or `null`. A run showed that AWS gives `null` for most
types. The check passes on that answer. For GCP, no run has
shown the shape, and the check shows it (no result yet: the live window ran the plan on AWS alone). The check `key-pairs-shape`
expects `keyNames[]` with the key pair of the entity. It passes on AWS.

Five checks ask the list routes that the rules read (#393). Each check of a key list expects the
keys that the route gives, and no other key. A key beside them is a field that a rule of the cloud
tier reads, such as the tags of a subnet. That rule can then move to Palette.

A call by hand showed the default filter of the instance type route on AWS. With no query, the
route lists no `t3.small` and no `t3.micro`. With `cpuGtEq=0&memoryGtEq=0`, it lists both, each with
`cpu` 2. It lists no `m3.large` in both cases. The two checks `instancetypes-cpu` and
`instancetypes-default-filter` show the filter on every engine of the plan. The
quota rules and `instance-offering` send the query.

A run of the plan on AWS passes each check of the table. The other engines have no result yet: the live window ran
the plan on AWS alone.

| Check | What it expects |
|---|---|
| `instancetypes-cpu` | The instance type route, asked with `cpuGtEq=0&memoryGtEq=0`, lists each pool type of the entity with `cpu` as a number above 0. The quota rules count that number as the vCPUs. The list holds a type under 2 vCPUs or 4 GiB, with `cpu` above 0. |
| `instancetypes-default-filter` | The route with no query for the filter lists no type under 2 vCPUs or 4 GiB. The query `cpuGtEq=0&memoryGtEq=0` lists more types, with a small type among them. |
| `vpcs-route-keys` | The VPC route lists `vpcs[]` with `vpcId`, and each subnet with `subnetId` and `az`. A VPC has no key beside `cidrBlock`, `name`, `subnets`, and `vpcId`. A subnet has none beside `az`, `isPrivate`, `mapPublicIpOnLaunch`, `name`, and `subnetId`. |
| `zones-route-keys` | The zone route lists `zones[]` with `name`, `state`, and `zoneId`, and no other key. So it lists no zone type. |
| `sgs-route-keys` | The security group route lists `groups[]` with `groupId` and `groupName` for the first VPC of the region. A group has no key beside `groupId`, `groupName`, and `ownerId`. |

See
[The routes of Palette beside the calls](../internals/preflight.md#the-routes-of-palette-beside-the-calls).

A run that makes a fault
puts `caught` in the ledger, and the clean entity of the same run puts `passed`. The ledger holds
both results of `instance-offering`, `ssh-key-pair`, and `iaas-pod-identity` for AWS.

On AWS, the plan sends the key pair that the region lacks to `iaas-pod-identity`, because the
property route of AWS refuses it. The route answers HTTP 500 with the code
`AwsKeyPairNotFound`: see [The property routes](../internals/palette/validate-routes.md#the-property-routes).
A check reads a 5xx as an error, so the rule names that code in
[`refusal_codes`](../reference/preflight-rules.md#a-refusal-at-a-5xx). The check
`properties-key-pair` expects the report of the fault to fail the rule with `AwsKeyPairNotFound`,
and not to give `ERROR`. The step `fault-iaas-pod-identity` expects `caught` in the ledger. A run on
AWS passes both.

Part 3 of each run also runs the custom probes of the lab,
`examples/live/preflight/lab-probes.yaml` (`lab-region`, `lab-name`), beside
the catalog, with `--probes`. This is a custom probe on a real tenant.

With `--build`, the run adds three steps:

- the build of cluster A through the provider, with the watch on it from its
  first listing
- `strata preflight cluster:UID` on the cluster as Palette holds it
- a change of the instance type of its worker pool, which `gke-type-change`,
  `eks-type-change-orphan`, or `pool-replace-fields` must fail

`--down` removes the example and reads the inventory.

The runs of the plan, and what each one showed, are in the section
[What the preflight runs show](#what-the-preflight-runs-show) below.

## watch.sh

`just live-watch --build --down` on aws built cluster B beside cluster A. The
watch on it ran from the moment Palette listed it, on the terminal. There were
43 readings, and the cluster was `Running` after 1335 seconds. Each watch rule
passed at the end.

At 662 seconds `webhook-call-failed` warned. The warning was a
`Reconciler error` of the controller in the new cluster, "failed to set
controlPlane type annotation". It was the call of `mspectrocluster.kb.io`,
refused right after the pivot, as on gcp. The warning held for 20 readings while
the event stayed in the 50 newest. The build then reached `Running`, so the
watch did not stop.

The removal of cluster B did not run. The script was edited during the run, and
bash reads a script as it runs. So the process resumed in the changed file and
ended. The script is now one function, which bash parses before it runs. The
cluster was removed by hand with `tofu apply -var second_cluster=false`.

## preflight-walkthrough.md

The story of [Preflight: one cluster, end to end](./preflight-walkthrough.md), run
once as written, on aws.

The clean entity of the last plan run passed every tier in 31 seconds, with exit
0. The copy with one worker of four cores failed `worker-cpu-floor` and
`palette-validate` in 33 seconds, with exit 1 under `--strict`.
`POST /v1/spectroclusters/aws` with the same body answered the uid in 4 seconds.

The watch started at once. It took 25 readings, and the cluster was `Running`
after 764 seconds. Each watch rule passed, and there was no warning this time.
On the cluster as Palette holds it, there were two notes, `bastion-cost` and
`variables-in-use`. On the change of the worker instance type to `m5.xlarge`,
`pool-replace-fields` warned that the machines roll.

The delete was accepted at once. Ten minutes later the events named the security
groups and the VPC as deleted. The inventory of AWS read 0 on all 12 lines.

## What the preflight runs show

The runs of `preflight-plan.sh` on the tenant of the live example, with the
profiles of each engine and no cluster. Each run is an entry of the ledger.

**aws.** Four faults made on purpose, four caught. `POST /v1/spectroclusters/aws/validate`
refuses one worker of four cores with `MachinePoolWithMinCpuRequired`: "A
minimum allocation of '4.1' CPU cores is required across all worker nodes.
Current allocation : '4' CPU cores", in under 4 seconds. Through the provider
the same fault comes after a full apply. A cloud account uid that is not in the
project is a 404 of `GET /v1/cloudaccounts/aws/{uid}`. An internal control
plane load balancer is a static failure.

On the clean entity, 37 checks ran and 36 passed. The exec rules ran against AWS
with the profile of the lab (`aws-quotas`, `aws-ami`, `capa-bootstrap-stack`,
`ebs-csi-node-policy`, `iam-ten-policies` (now `iam-role-policy-quota`),
`aws-region-not-enabled`, `leftovers-before-build`).

The one failure was a wrong result of
`role-partial-permissions`. `aws iam simulate-principal-policy` with
`iam:PassRole` and no resource ARN gives `implicitDeny` for a role whose policy
allows it on the CAPA roles alone. That role has built clusters. The rule now
leaves `iam:PassRole` to a reproduce line with the three CAPA role ARNs. The run
is in the ledger as the first wrong result of a check.

**eks.** Six faults made on purpose, five caught. The five were:

- one zone
- no worker pool
- an `updateStrategy` that EKS does not read
- a private endpoint alone
- a cloud account uid that is not in the project

The sixth, one worker
of four cores, was not caught. `POST /v1/spectroclusters/eks/validate` answers
`200` with no error for it. The AWS route answers `MachinePoolWithMinCpuRequired`
for the same pool.

No run shows whether an EKS create with one worker of four cores fails later. So
the CPU floor on EKS is a reading and not a fact. On the clean entity, 44 checks
ran and 42 passed.

The two wrong results were the two rules that simulate the IAM policy of the
role of the cloud account. `aws iam simulate-principal-policy` with no
`--resource-arns` evaluates each action against the resource `*`. A statement
that names a resource does not match it. The EKS policy of Palette has such
statements for `eks:*`. The role built the EKS clusters of #89.

Both rules are warnings now. One simulates the actions that name a resource, with
the cluster, the node group, and the CAPA role ARNs of the build. The other
simulates the actions on `*` alone, and gives the PassRole line with the CAPA
roles as a command to run by hand. A simulation with no request context cannot
prove a build, so neither is a `fail`.

**gke.** Seven faults made on purpose, seven caught. `POST /v1/spectroclusters/gke/validate`
refuses a pool of 4 with `InvalidParam`, in under 200 ms. The message is
"Invalid parameter 'Node pool size'; caused by: Node pool size should be
multiple of 3". Through the provider, #90 saw the same refusal after a full
apply. Five more faults are static failures or a 404, in milliseconds:

- a cluster with no release channel
- a GPU machine type
- Autopilot with a pool
- a name over 40 characters
- a cloud account uid that is not in the project

On the clean entity, 34 checks ran and 32 passed. The two failures say something
about the GCP project of the lab, and not about the build. The service account
holds no `roles/storage.objectViewer` at the project level, and the Cloud
Resource Manager API is off. The account built the GKE clusters of #90 and the
GCP clusters of #74 nevertheless. So a documented role or API that a check does
not find is a warning to read, and both rules are `warn`.

The plan does not build, so a rule from the source stays a probe after these
runs. `strata preflight --promotion` promotes a static rule only when a live run
shows the fault (`status: seen`), such as `gke-no-release-channel` from #90.

**gcp.** Four faults made on purpose, four caught. The GCP validate route
applies the CPU floor as the AWS one does: one worker of four cores is
`MachinePoolWithMinCpuRequired` in under a second. A name over 52 characters
gives a derived network name over 63, which the static rule computes as Palette
derives it (`<name>-network`, `<name>-network-<region>`). On the clean entity,
29 checks ran and 27 passed. The two failures are the two findings about the
GCP project of the gke run. They are `warn` now.

Over the four engines, the plan made 21 faults and the checks caught 20. The
one not caught is the CPU floor on EKS, which the EKS validate route does not
report. So `worker-cpu-floor` no longer names `eks`, and the plan no longer
makes that fault there. Of the 144 checks that ran on the clean entities, 137
passed. The seven failures were three wrong results of two IAM simulations and
the two GCP project findings, twice.

After the fixes, the plan ran again on gcp and on eks. Every fault was caught,
and every rule of severity `fail` passed on the clean entities.
`strata preflight --promotion` exits `0` with the ledger of the six runs. The
IAM simulation on eks warns that `eks:CreateCluster` is `implicitDeny` on each
resource of the build, for the role that built the #89 clusters. The rule now
says that this is a warning to read.

**aws, with the list routes.** The plan ran again on aws with the checks of the list routes. Each
route gave the keys that its check expects. The instance type route dropped a type under 2 vCPUs or
4 GiB when the query gave no filter. On the clean entity, 22 checks passed. The plan made six faults
on purpose and caught five.

The sixth fault was a key pair that the region lacks, for `iaas-pod-identity`. Palette answered
HTTP 500 with `AwsKeyPairNotFound`, and a check read that 500 as an error. So 21 of the 22 steps
passed. The rule now names the code in `refusal_codes`. The next run caught all six faults, and all
22 steps passed. The ledger counts the catch of `iaas-pod-identity` as 1 of 1.

**A custom probe on the tenant.** Each run since the build phase also runs the
two custom probes of the lab. They are `lab-region` and `lab-name`, from
`examples/live/preflight/lab-probes.yaml`, beside the catalog. Both ran on the
clean entity of the gke run and passed. A file of a company runs the same way,
with no change to strata.

**gke, with a build.** The plan built cluster A through the provider, with
the watch on it from the moment Palette listed it. There were 39 readings, and
the cluster was `Running` after 1190 seconds. Each watch rule passed
(`agent-egress`, `node-egress-images`, `status-not-cloud`,
`pivot-webhook-timeout`). On the cluster as Palette holds it, 38 checks ran and
34 passed. The three warnings are the two GCP project findings and
`position-zero`. The provider put the add-on profile at position 0 of the
cluster, as the order report says it does.

The one failure was a wrong result of strata. The entity built from the cloud
config carried `skipK8sUpgrade: "disabled"`, which Palette stores on each pool.
The GKE validate route refuses the field with `InvalidParam`. The entity now
carries it when it is enabled alone. The change of the instance type of the
worker pool, `e2-standard-4` to `e2-standard-8`, was caught by
`gke-type-change` and `pool-replace-fields` before any apply. This is the fault
of #90, found in a millisecond.

The destroy of the cluster ended with `context deadline exceeded` at the
deadline of the provider, and a second destroy completed, as
`provider-delete-timeout` says. The inventory step of the run could not ask GCP.
`gcloud` demanded a reauthentication in the middle of the campaign, which a
non-interactive shell cannot give. This is the `cloud-cli-auth` row of the lab.
The inventory, run by hand afterward, was 0 on every line. It counted the GKE
cluster.

**eks, with a build.** The watch stopped at 1355 seconds, after 44 readings,
on `pivot-webhook-timeout`, as the rule was then. An event named a failed call
of the webhook `mawscloudconfig.kb.io` while the controller set the owner of
the cloud config. This was right after the pivot, with the cluster
`Provisioning` and no error in its status.

The message of the rule cut the event at 240 characters, before the cause. The
`world.json` of the watch held no reading. So it is not known whether the call
timed out or was refused. Both faults are fixed below. The provider then ended
its apply with exit 0 while the cluster was still `Provisioning`.

On the cluster as Palette holds it, 46 checks ran and 42 passed.
`delete-while-provisioning` failed, because the cluster was not `Running`, and
said what a delete does. The plan then deleted the cluster, and the delete
waited, as the rule said. The first destroy ended at the deadline of the
provider. The cluster was `Deleting`, and the EKS cluster, the VPC, and three
NAT gateways were still in AWS.

The change of the instance type of the worker pool, `t3.xlarge` to
`m5.xlarge`, was caught by `eks-type-change-orphan` and `pool-replace-fields`
before any apply. This is the fault of #89, found in a millisecond.

Two wrong results of strata came out of this run and are fixed:

- The validate route refuses the name of an existing cluster with
  `ResourceAlreadyExists`. So `palette-validate` and `worker-cpu-floor` now check
  an existing cluster under a suffixed name.
- The plan now waits for `Running` before it deletes.

The delete did not end by itself. After 75 minutes the EKS control plane, its
node group, the VPC, and three NAT gateways were still in AWS. The cluster was
`Deleting`, with `CloudInfrastructureCleanedUp: DeletingInfrastructure`.

The recovery was the one of the book: `eks-nodegroups.sh --delete`,
`aws-cleanup.sh fred us-east-2 --yes`, the inventory at 0, then a second plain
delete. The findings report on the pivot webhook gives that delete as the step
that ends the wait. It did not end it here. From then on each reconcile of the
delete failed, every five minutes, with the cloud empty. The error was "failed to
delete additional resources: failed to get existing aws managed control plane:
AWSManagedControlPlane fred-cp not found".

In this run the delete path read a control plane object that was gone as an
error, and not as done. So the delete did not end. A force delete of the record
ended it. The mechanism is the one of that report, on EKS this time, with this
one more step, which a second plain delete did not end.

One hour of the recovery went to a fault of the operator. `tofu output -raw`
printed a warning beside the project uid, and a shell exported both as
`PALETTE_PROJECT_UID`. Every request with that header answered a plain-text
`400 Bad Request`. This looked like a refusal from Palette, but it was not
one. With the uid alone, the API answered as it does.

`lib.sh` reads the outputs as JSON, with the warnings dropped, so the scripts of
the lab do not meet this fault. A shell that copies a value by hand can.

**aws, with a build.** The plan built cluster A through the provider, with the
watch on it from the moment Palette listed it. There were 41 readings, and the
cluster was `Running` after 1278 seconds. Each watch rule passed
(`agent-egress`, `csi-crash-after-running`, `node-egress-images`,
`infrastructure-stalled`, `pivot-webhook-timeout`, `stale-worker-condition`).

Before the build, the four faults made on purpose were caught:

- one worker of four cores, by `worker-cpu-floor` and `palette-validate`
- an internal control plane load balancer, by `api-endpoint-reach` in a
  millisecond
- a cloud account uid that is not in the project, by `account-missing`

On the clean entity and on the cluster as Palette holds it, 40 checks ran and 40
passed, with two notes. The notes are `bastion-cost`, and `variables-in-use`
once the profile had a cluster. The change of the instance type of the worker
pool, `t3.xlarge` to `m5.xlarge`, was caught by `pool-replace-fields` before any
apply. The teardown ran after `Running`. It was one destroy of 14 resources, and
the inventory of AWS was 0 on every line.

**gcp, with a build.** The four faults were caught before the build. The derived
network name of a long cluster name was one of them (`gcp-name-length`, in a
millisecond).

The watch stopped at 671 seconds, after 22 readings, on `pivot-webhook-timeout`
as the rule was then. Right after the pivot, two events named a failed call,
eight seconds apart:

- a `ReconcileError` of the controller named a failed call of
  `mspectrocluster.kb.io` while it set the type of the control plane
- a `Log` event of the agent named a failed call of `vspectrocluster.kb.io`
  while it updated the cluster

Palette made the calls again. The wait of the plan saw `Running` 23 minutes
after the watch started, with no other event of note. The teardown ran after it.
It was one destroy of 14 resources, and the inventory of GCP was 0 on every
line.

On the clean entity, 33 checks ran and 31 passed. On the cluster as Palette
holds it, 34 ran and 32 passed. The two warnings are the two GCP project
findings of the gke run. The change of the instance type of the worker pool,
`e2-standard-4` to `e2-standard-8`, was caught by `pool-replace-fields` before
any apply.

**Three wrong results of strata came out of the gcp run, and are fixed.**

- `pivot-webhook-timeout` failed on one minute of failed webhook calls, which the
  build survived. The rule now fails on one of these events:
  - the failed move of the cluster
  - the cloud config that the controller cannot read
  - 10 minutes or more of failed calls while the cluster is not `Running`

  The new warning `webhook-call-failed` names each failed call. Both show the
  start and the end of each event. The end names the cause, a timeout or a
  connection refused, and the old message cut it.
- The `world.json` of a watch held the world before the first reading, with no
  conditions and no events. So the full events of the eks and the gcp watches
  are lost. It now holds the last reading.
- The watch ended at the first failure of any rule. It now ends at the first
  counted failure alone, as the exit code counts them. A failed probe or a
  warning is in the report while the watch continues.

The ledger gives the gcp run as `passed` for the narrowed rule. Its events are in
the fixture `watch-webhook-refused` that the tests of the rule replay. The ledger
gives `caught` for the warning on gcp and on eks. The eks entry of the old rule
is gone, because the cluster was deleted before the outcome was known. The
findings report on the build runs holds the timeline of the eks delete and the
source of each finding.

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

## The type of a cluster that Palette builds

A read by hand, with cluster A of `live-up` and no write: no script holds it.
`strata list clusters --json` gives cluster A the `clusterType` `PureManage`.
`strata list clusters --applied` gives the release profile and the add-on
profile of cluster A. No run imports a cluster: see
[An imported cluster](../internals/palette/clusters.md#an-imported-cluster).

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
a manifest of the add-on, as on AWS. The two clouds use one upgrade engine.

What differs is outside strata:

- **The pack names and versions.** The OS and CSI packs are the packs of the
  cloud. The CSI values write the reclaim line as `reclaimPolicy: "Delete"`,
  and not `reclaimPolicy: Delete`: the edit of each script comes from the output
  `csi_reclaim_delete`.
- **The Kubernetes version.** The Kubernetes pack lists `gcp`, but no GCP image
  is shared for Kubernetes 1.36.2 with Ubuntu 24.04. GCP answers
  `Required 'compute.images.useReadOnly' permission` for the image in the project
  `spectro-image-build`, and no machine starts. The image of 1.35.6 is shared.
- **The service account of the nodes.** The nodes run as the Compute Engine
  default service account of the project. In an organization that gives that
  account no role, the GCE cloud controller exits, the node keeps the taint
  `node.cloudprovider.kubernetes.io/uninitialized`, and the cluster stays
  `Provisioning`. `roles/compute.admin` on the project lets it continue.

## The CAPA stack after an upgrade

A create of cluster A on AWS by `live-up` stopped at `Provisioning`. The events
of the cluster said "failed to setup cloud specific prerequisites" again and
again, and no condition was `False`. Reads of the account by hand, with no
write, gave the cause:

- CloudTrail holds three updates of the CAPA stack by the role of the cloud
  account, one at the first create after each upgrade. Each writes the tag
  `palette-version` on the six CAPA roles: `4.9.24`, `4.10.4`, and `4.10.12`.
  The first two changed only tags.
- The update of `4.10.12` also changed the trust policy of the roles. AWS
  refused `iam:UpdateAssumeRolePolicy` with `AccessDenied`, and then refused the
  rollback. The stack stayed in `UPDATE_ROLLBACK_FAILED`.
- The role held the documented core policies of Palette, unchanged since the
  account was made. `aws iam simulate-principal-policy` gives `implicitDeny` for
  `iam:UpdateAssumeRolePolicy` and `iam:DetachRolePolicy` on the CAPA roles, and
  for `cloudformation:ContinueUpdateRollback` and `cloudformation:DeleteStack`
  on the stack.

`live-down` removed the cluster, and the stack stayed in
`UPDATE_ROLLBACK_FAILED`. Then `strata preflight` ran the three rules of the
stack against the account, with `--release 4.10.12`:

| Rule | Result | Message |
|---|---|---|
| `capa-bootstrap-stack` | `PROBE FAIL` | the stack is `UPDATE_ROLLBACK_FAILED`: AWS refuses each update, and an administrator must recover it |
| `capa-stack-update-denied` | `WARN` | `iam:UpdateAssumeRolePolicy` and `iam:DetachRolePolicy` are not allowed on each of the 6 CAPA roles |
| `capa-stack-release-behind` | `NOTE` | Palette `4.10.4` applied the stack last, and the tenant runs `4.10.12` |

The tag says `4.10.4` because AWS rolled back the update of `4.10.12`. Before
the create, `capa-bootstrap-stack` passes, because the stack is complete: the
create itself breaks it. The two new rules find the risk before the create. See
[The CAPA stack of an AWS account](../internals/palette/clusters.md#the-capa-stack-of-an-aws-account).

## What has no live run

- `in-place.sh`, `registry-move.sh`, `registry.sh`, `template.sh`,
  `cluster.sh`, and `tag-only.sh` have run under OpenTofu, with the same
  provider that Terraform used, and give the same answers. `drift-report.sh` has
  run under Terraform only. Stages 1 to 3 of `order.sh` read the OpenTofu
  outputs and apply nothing. No live window ran stage 4.
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
- No script installs a Zarf package on a cluster. Palette gives a Zarf pack the
  values of an app profile, `{{.spectro.system.appdeployment.tiername}}`, so it
  needs an app deployment of Palette Dev Engine.
