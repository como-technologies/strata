# Live runs

The recorded runs of [the live example](./live-example.md), newest last in each
section. `examples/live/runs/` is not tracked, so this page and the issue
comments keep the evidence. The runs used Terraform 1.16.2. The check
descriptions are quoted as each run recorded them.

Each uid below is an example, as everywhere in this book. The check tables, the
run ids, the counts, and the roots are the recorded ones.

## The first run with OpenTofu

Run `20260917T183929Z-tofu`, with OpenTofu 1.12.6 and the Spectro Cloud provider
0.30.1. It is the run that moved the example from Terraform to OpenTofu, and the
only run that OpenTofu made. The earlier runs used Terraform 1.16.2.

| Step | Result |
| --- | --- |
| `tofu init` | Read the state that Terraform wrote, and took the provider from `registry.opentofu.org`. The lock file names that registry now. |
| `tofu plan` | Exit 2 with the creates of the whole example, and no other change. |
| `tofu apply` | `Apply complete! Resources: 5 added`. The four profiles took 3 to 4 seconds each, and cluster A 16m3s. |
| `verify.sh` | 24 of 24 checks passed, with the same answers as the Terraform runs. |
| `tofu destroy` | `Destroy complete! Resources: 5 destroyed`. Cluster A took 19m17s. |
| `api.sh delete-named` | Each of the four profile names holds no version. |
| After the teardown | The cluster has the state `Deleted`, the project holds no profile, and `aws-inventory.sh` prints 0 on each line. |

The plan and apply text that the checks read did not change: OpenTofu writes the
same `Plan:`, `Apply complete!`, and `# <resource> will be updated in-place`
lines, and `-detailed-exitcode` answers 2 for a change.

`registry-digest-oci` failed in the first attempt of `verify.sh` with HTTP 401.
The cause was the login, not the tool: `aws ecr get-login-password` had failed
with `AccessDeniedException`, and `crane auth login` then stored an empty
credential and reported a success. A login with an AWS profile that has
`ecr:GetAuthorizationToken` fixed it, and the check read the recorded digest
`sha256:99cac22f712b...`.

The scripts that write to the tenant (`in-place.sh`, `registry-move.sh`,
`tag-only.sh`, `drift-report.sh`) have no OpenTofu run yet.

## verify.sh

Session `20260917T031126Z` on 2026-09-17: one `just live-up`, then `just
live-session`, in the `strata-test` project on AWS us-east-2, with cluster A
`fred` and cluster B `barney`. The four scripts ran 144 checks, and none failed:
`verify.sh` 24, `in-place.sh off` 51, `in-place.sh on` 53, and
`registry-move.sh` 16. No step was manual.

| Result | Check | Expected exit | Got | What it shows |
|---|---|---|---|---|
| PASS | `snapshot-cluster` | 0 | 0 | strata reads what a live cluster runs |
| PASS | `cluster-runs-release` | 0 | 0 | the cluster reports the release profile version |
| PASS | `cluster-runs-two-profiles` | 0 | 0 | #23: the cluster runs the release profile, then the add-on profile |
| PASS | `snapshot-release` | 0 | 0 | record the version the cluster runs |
| PASS | `compare-release-cluster` | 0 | 0 | #1: a cluster that runs the recorded version is IDENTICAL |
| PASS | `snapshot-addon` | 0 | 0 | record the add-on profile version the cluster runs |
| PASS | `compare-addon-cluster` | 0 | 0 | #23: the add-on record is IDENTICAL to the cluster's add-on selection |
| PASS | `snapshot-next` | 0 | 0 | record the newer version |
| PASS | `compare-next-cluster` | 1 | 1 | #1: against a newer record, the cluster shows the change |
| PASS | `compare-next-cluster-json` | 1 | 1 | the same comparison as JSON |
| PASS | `only-the-edited-value` | 0 | 0 | #1: the only change is csi-aws-ebs reclaimPolicy |
| PASS | `consumers-release` | 1 | 1 | #12: the release version has a consumer |
| PASS | `consumers-release-json` | 1 | 1 | the same listing as JSON |
| PASS | `consumer-is-the-cluster` | 0 | 0 | #12: listed with its name, project, and state |
| PASS | `consumers-next` | 0 | 0 | #12: nothing runs the newer version |
| PASS | `candidate-file` | 0 | 0 | #21: write the newer version's record as a candidate file |
| PASS | `snapshot-candidate` | 0 | 0 | #4: snapshot the candidate through registry lookups |
| PASS | `candidate-same-release` | 0 | 0 | #4: the candidate's root equals the published root |
| PASS | `verify-release` | 0 | 0 | an untouched published version verifies |
| PASS | `verify-cluster-record` | 0 | 0 | a cluster record verifies against the cluster |
| PASS | `snapshot-registries` | 0 | 0 | record the profile with a Helm pack and an OCI pack |
| PASS | `registries-kinds` | 0 | 0 | #2: the profile holds one helm pack and one oci pack |
| PASS | `registry-digest-helm` | 0 | 0 | #2: read the helm pack's digest from the registry itself |
| PASS | `registry-digest-oci` | 0 | 0 | #2: read the oci pack's digest from the registry itself |

The cluster runs the release, and the add-on profile after it. Each record
compares `IDENTICAL` with its selection:

```text
strata-live-profile@1.0.0 (uid 64e0000000000000000000f1)  ->  strata-live-profile@1.0.0 on cluster fred (uid 64f1000000000000000000f5)
root       eada36c2e3fd -> eada36c2e3fd   IDENTICAL
```

```text
strata-live-addon@1.0.0 (uid 64e2000000000000000000f1)  ->  strata-live-addon@1.0.0 on cluster fred (uid 64f1000000000000000000f5)
root       9c68e5319d53 -> 9c68e5319d53   IDENTICAL
```

A newer record against the same cluster shows the one value that differs:

```text
strata-live-profile@1.0.1 (uid 64e0000000000000000000f2)  ->  strata-live-profile@1.0.0 on cluster fred (uid 64f1000000000000000000f5)
root       7d7689e8321c -> eada36c2e3fd   CHANGED
consumers  0 -> 0 clusters

  variables unchanged

  [os   ] ubuntu-aws                 24.04              unchanged
  [k8s  ] kubernetes                 1.36.2             unchanged
  [cni  ] cni-cilium-oss             1.20.1             unchanged
~ [csi  ] csi-aws-ebs                1.65.0             AUTHORED
      values: 1 changed, 0 added, 0 removed
        ~ charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy: "Retain" -> "Delete"
      impact: cluster-wide infra addon update
```

The version that the cluster runs lists it. The candidate from the 1.0.1 record,
with its values in files, gives the root of 1.0.1:

```text
strata-live-profile@1.0.0 (uid 64e0000000000000000000f1): 1 consumer
  fred  64f1000000000000000000f5  project 64f20000000000000000a0d1  Running
```

```text
IDENTICAL      candidate-snapshot.json -> next.json   root 7d7689e8321c
SAME RELEASE: 1 pairs passed
```

Each pack-ref digest equals the digest that Palette lists for the pack version.
For the Helm and OCI packs, `tenant.sh digest` read the same digest from the
registry itself:

| Pack | Type | Ref digest | Digest of Palette | Digest of the registry | Fed UPSTREAM | |
|---|---|---|---|---|---|---|
| ubuntu-aws | spectro | `sha256:2df05daa4eb4` | `sha256:2df05daa4eb4` | - | packRef.digest | equal |
| kubernetes | spectro | `sha256:c4f7f20f40c7` | `sha256:c4f7f20f40c7` | - | packRef.digest | equal |
| cni-cilium-oss | spectro | `sha256:917d9de9a696` | `sha256:917d9de9a696` | - | packRef.digest | equal |
| csi-aws-ebs | spectro | `sha256:bd09e8b4187b` | `sha256:bd09e8b4187b` | - | packRef.digest | equal |
| nginx | helm | `08816cde6bea` | `08816cde6bea` | `08816cde6bea` | packRef.digest | equal |
| argocd | oci | `sha256:99cac22f712b` | `sha256:99cac22f712b` | `sha256:99cac22f712b` | packRef.digest | equal |

In the earlier run `20260916T153806Z`, an optional edit in place changed
`volumeBindingMode` on the published 1.0.1. `strata verify` on the earlier
record exited `1`:

```text
root       7d7689e8321c -> 6fdc871ef9a6   CHANGED
~ [csi  ] csi-aws-ebs                1.65.0             AUTHORED
        ~ charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].volumeBindingMode: "WaitForFirstConsumer" -> "Immediate"
DRIFT: published version strata-live-profile@1.0.1 differs from the copy recorded 2026-09-16T15:38:09Z
```

The first attempts found two problems. Both are part of the example now:

- Palette refused the cluster with one 4-core worker: "A minimum allocation of
  '4.1' CPU cores is required across all worker nodes". Two workers have enough.
  The cluster then took 19 minutes to become `Running`.
- The candidate check failed until strata accepted a version as a tag, because
  Palette stored that tag.

## registry-move.sh

Run `20260917T150025Z`, flag on, provider 0.30.1. All 16 checks passed.
`hold_versions` changed only `skip_destroy` on the four profiles (`0 added, 4
changed, 0 destroyed`). A flag-on plan then changed nothing (`plan-no-change`).
Each apply ran without `-target`. The bump pinned `csi-aws-ebs` 1.65.0 from
`Public Repo` to `Palette Registry`, and changed its type, registry uid, pack
uid, and digest, and nothing else. The flag kept 1.0.0 (`move1-kept`).
`same-release --allow registry-move` passed the pair with 1 pack re-pinned. After
an edit in place of the values of 1.0.1, it failed the pair and named only
`pack csi/csi-aws-ebs AUTHORED: charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`.
`apply_baseline` removed the move profile from the state and set `skip_destroy`
back to false (`0 added, 4 changed, 1 destroyed`). The API deleted both versions,
and the run ended at the baseline.

| Result | Check | Expected exit | Got | What it shows |
|---|---|---|---|---|
| PASS | `plan-no-change` | 0 | 0 | Terraform plans no change once skip_destroy is held for the flag |
| PASS | `move1` | 0 | 0 | record 1.0.0, every pack from the pack registry |
| PASS | `move-new-uid` | 1 | 1 | the flag made 1.0.1 a new profile version |
| PASS | `move1-kept` | 0 | 0 | 1.0.0 is still there, unchanged |
| PASS | `move2` | 0 | 0 | record 1.0.1 |
| PASS | `move2-csi-is-oci` | 0 | 0 | 1.0.1's CSI pack comes from the OCI registry |
| PASS | `compare-move` | 1 | 1 | every field that differs between the two records |
| PASS | `same-release-exact` | 1 | 1 | an exact check fails: the registry moved |
| PASS | `same-release-registry-move` | 0 | 0 | #24: the registry-move policy passes a real move, which changes the pack type |
| PASS | `api-edit-move2` | 0 | 0 | edit 1.0.1's CSI values through the API |
| PASS | `move2-edited` | 0 | 0 | record the edited 1.0.1 |
| PASS | `same-release-after-edit` | 1 | 1 | #24: the policy fails the pair after an in-place edit |
| PASS | `edit-is-authored` | 0 | 0 | the reason names the AUTHORED change |
| PASS | `delete-move2` | 0 | 0 | delete 1.0.1 |
| PASS | `delete-move1` | 0 | 0 | delete 1.0.0 |
| PASS | `baseline-after` | 0 | 0 | the live example is back at its baseline |

## tag-only.sh

Run `20260917T124102Z`, flag off. All 32 checks passed. Versions 1.0.2 and 1.0.3
differ from 1.0.0 only in the tag of the OS pack, `LTS__24.04` and `LTS__24.x`,
with one pack uid and version for all three. `compare` calls each pair `SAME
CONTENT` with the impact `none (tag renamed, same pack build)`, and
`same-release` passes the three. The repave check of Palette answers no repave
for each move, and a repave for the control. Cluster A ran 1.0.2 and then 1.0.0
again, each for 15 minutes, on the same four EC2 instances. After the rename of
the tag of 1.0.2 in place, `verify` of its earlier record reports `DRIFT` with
no node impact. The first run, `20260917T044135Z`, recorded the same answers
before the expectations were set.

```text
strata-live-profile@1.0.0 (uid 64e0000000000000000000f1)  ->  strata-live-profile@1.0.2 (uid 64e0000000000000000000f3)
root       eada36c2e3fd -> afdc4d3e06d1   SAME CONTENT
note       the roots differ only by the tag name a pack is pinned under: the same pack build
consumers  1 -> 0 clusters

  variables unchanged

~ [os   ] ubuntu-aws                 24.04              tag 24.04 -> LTS__24.04
      impact: none (tag renamed, same pack build)
  [k8s  ] kubernetes                 1.36.2             unchanged
  [cni  ] cni-cilium-oss             1.20.1             unchanged
  [csi  ] csi-aws-ebs                1.65.0             unchanged
```

| Result | Check | Expected exit | Got | What it shows |
|---|---|---|---|---|
| PASS | `release1` | 0 | 0 | record 1.0.0, the OS pack under tag 24.04 |
| PASS | `a1` | 0 | 0 | record cluster A's release selection |
| PASS | `instances1` | 0 | 0 | cluster A's EC2 instances before any move |
| PASS | `exact1` | 0 | 0 | record 1.0.2 |
| PASS | `floating1` | 0 | 0 | record 1.0.3 |
| PASS | `os-packs` | 0 | 0 | the OS pack's tag, version, and pack uid as Palette stored each version |
| PASS | `one-build` | 0 | 0 | Palette stored the tag each version was authored with, and one pack uid and version for all three |
| PASS | `compare-exact` | 1 | 1 | 1.0.0 against 1.0.2, the exact tag: the roots differ |
| PASS | `compare-exact-says` | 0 | 0 | #19: the pair is SAME CONTENT, names the rename, and gives no node impact |
| PASS | `compare-exact-json` | 1 | 1 | the same comparison as JSON |
| PASS | `compare-exact-json-says` | 0 | 0 | #19: sameContent, the OS pack tagRenamed, max impact none |
| PASS | `compare-floating` | 1 | 1 | 1.0.0 against 1.0.3, the floating tag: the roots differ |
| PASS | `compare-floating-says` | 0 | 0 | #19: the floating tag is the same content too |
| PASS | `same-release-tags` | 0 | 0 | #19: 1.0.0, 1.0.2, and 1.0.3 are one release |
| PASS | `palette-repave-exact` | 0 | 0 | Palette: a move from 1.0.0 to 1.0.2 repaves nothing |
| PASS | `palette-repave-floating` | 0 | 0 | Palette: a move from 1.0.0 to 1.0.3 repaves nothing |
| PASS | `palette-repave-control` | 1 | 1 | the control: 1.0.0 with one more kubeadm command repaves |
| PASS | `a2` | 0 | 0 | record cluster A's release selection |
| PASS | `a2-runs-exact` | 0 | 0 | cluster A's selection is IDENTICAL to 1.0.2 |
| PASS | `instances2` | 0 | 0 | cluster A's EC2 instances after the move |
| PASS | `repaved-to-exact` | 1 | 1 | #19: the move to 1.0.2 kept cluster A's instances (a repave replaces them: 0) |
| PASS | `palette-repave-back` | 0 | 0 | Palette: a move from 1.0.2 back to 1.0.0 repaves nothing |
| PASS | `a3` | 0 | 0 | record cluster A's release selection |
| PASS | `a3-runs-release` | 0 | 0 | cluster A's selection is IDENTICAL to 1.0.0 |
| PASS | `instances3` | 0 | 0 | cluster A's EC2 instances after the move back |
| PASS | `repaved-back` | 1 | 1 | #19: the move back to 1.0.0 kept cluster A's instances (a repave replaces them: 0) |
| PASS | `retag` | 0 | 0 | #19: rename 1.0.2's OS pack tag through the API, keeping its pack uid |
| PASS | `exact2` | 0 | 0 | record the renamed 1.0.2 |
| PASS | `retag-kept-pack-uid` | 0 | 0 | the rename kept the pack uid |
| PASS | `verify-exact1` | 1 | 1 | #19: verify of 1.0.2's record from before the rename reports DRIFT |
| PASS | `verify-exact1-says` | 0 | 0 | #19: the drift names the rename, the same content, and no node impact |
| PASS | `baseline-after` | 0 | 0 | the live example is back at its baseline |

## drift-report.sh

`drift-report.sh off` and `drift-report.sh on` in session `20260917T150538Z`,
provider 0.30.1, against cluster A `fred`, with the issues in this repository
under the label `strata-live-drift`. The two runs differ only in the
immutable-clusterprofiles flag. No check failed: 28 with the flag off and 27 with
it on. Each started from the baseline with no open issue, and restored it with
each issue that it opened closed. `hold_versions` changed nothing with the flag
off, and only `skip_destroy` on the four profiles with it on. The flag-on bump
ran without `-target` (`1 added, 0 changed, 1 destroyed`).

In both runs, the edit in place through the API opened one issue for the add-on
record, the second report updated that issue, and the undo closed it. The record
of cluster A, whose copy the edit did not reach, stayed `OK`. The bump
differed with the flag. Without it, the bump overwrote the first add-on uid in
place. The report opened one issue for the add-on record, whose report says the
version "now names strata-live-addon@1.0.1 ... was overwritten in place", and one
for the record of cluster A, whose add-on version was gone. The restore closed
both. With the flag, the bump kept 1.0.0, and no issue opened:

```text
off  verify-report: 3 records: 2 unchanged, 1 drifted, 0 could not be checked; opened #48 for records-off/addon.json
off  verify-report: 3 records: 1 unchanged, 2 drifted, 0 could not be checked; opened #49 for records-off/addon.json; opened #50 for records-off/cluster-a.json
off  verify-report: 3 records: 3 unchanged, 0 drifted, 0 could not be checked; closed #49 for records-off/addon.json; closed #50 for records-off/cluster-a.json
on   verify-report: 3 records: 3 unchanged, 0 drifted, 0 could not be checked
```

Session `20260917T133523Z` passed the same checks with provider 0.29.7 and its
workaround (#47). The session before, `20260917T132642Z`, found two problems in
the example, both fixed. GitHub lists a new or closed issue about five seconds
later, so the checks now wait for the listing. The flag-on bump asked Palette to
delete the version that cluster A runs, because the state still held
`skip_destroy = false`.

| Check | Flag off | Flag on | What it shows |
|---|---|---|---|
| `record-release` | PASS 0 | PASS 0 | record the release version |
| `record-addon` | PASS 0 | PASS 0 | record the add-on version |
| `record-cluster-a` | PASS 0 | PASS 0 | record cluster A |
| `report-baseline` | PASS 0 | PASS 0 | #25: the report over the three records at the baseline |
| `report-baseline-says` | PASS 0 | PASS 0 | every record verifies, and no issue opens |
| `plan-edit` | PASS 2 | PASS 1 | Terraform's plan for an in-place edit of the add-on version: an update in place without the flag, a refusal with it |
| `plan-edit-says` | PASS 0 | PASS 0 | the plan says which |
| `api-edit` | PASS 0 | PASS 0 | edit the add-on version in place through the API, which Palette accepts whatever the flag |
| `report-edit` | PASS 1 | PASS 1 | #25: the report after the edit exits 1 |
| `report-edit-opened` | PASS 0 | PASS 0 | #25: it opens one issue, for the add-on record; cluster A's copy is untouched |
| `issues-after-edit` | PASS 0 | PASS 0 | #25: one open issue, for the add-on record |
| `issue-has-report` | PASS 0 | PASS 0 | #25: the issue holds the report of the drift |
| `report-edit-again` | PASS 1 | PASS 1 | #25: a second report while the edit stands |
| `report-edit-again-updated` | PASS 0 | PASS 0 | #25: it updates the issue and opens none |
| `issues-after-edit-again` | PASS 0 | PASS 0 | #25: still exactly one open issue |
| `api-undo` | PASS 0 | PASS 0 | undo the edit through the API |
| `report-undo` | PASS 0 | PASS 0 | #25: the report after the undo exits 0 |
| `report-undo-closed` | PASS 0 | PASS 0 | #25: it closes the issue |
| `issue-edit-closed` | PASS 0 | PASS 0 | #25: the issue is closed on the host |
| `issues-after-undo` | PASS 0 | PASS 0 | no issue is open |
| `report-bump` | PASS 1 | PASS 0 | #25, #27: the report after the bump: drift only without the flag |
| `report-bump-opened` | PASS 0 | PASS 0 | #25, #27: the issues the bump opens: the add-on record's and cluster A's without the flag, none with it |
| `issues-after-bump` | PASS 0 | PASS 0 | #27, #30: without the flag, one open issue each for the add-on record and cluster A's record; with it, none |
| `bump-issue-names-overwrite` | PASS 0 | - | #27: the add-on record's issue says the version was overwritten in place |
| `report-restored` | PASS 0 | PASS 0 | #25: the report after the restore exits 0 |
| `report-restored-closed` | PASS 0 | PASS 0 | #25: it closes each issue the bump opened |
| `issues-after-restore` | PASS 0 | PASS 0 | no issue is open |
| `baseline-after` | PASS 0 | PASS 0 | the live example is back at its baseline |

### Through the reusable workflow

The same report through `.github/workflows/verify.yml@v0.5.0`, run on 2026-09-17
(`runs/20260917T134851Z/workflow/`) with [act](https://github.com/nektos/act)
0.2.82 and the runner image `catthehacker/ubuntu:gh-latest`, which has `gh` as
the runners of GitHub do. The caller was a scratch repository whose `origin` is
this repository, with one record of the add-on version and this workflow:

```yaml
name: strata drift
# A caller of the reusable verify workflow, run with act for #25.
on:
  workflow_dispatch:
permissions:
  contents: read
  issues: write
jobs:
  strata:
    uses: como-technologies/strata/.github/workflows/verify.yml@v0.5.0
    with:
      records: records/*.json
      label: strata-live-drift
      strata-version: v0.5.0
    secrets:
      strata-token: ${{ secrets.STRATA_READ_TOKEN }}
      palette-api-key: ${{ secrets.PALETTE_API_KEY }}
```

```bash
act workflow_dispatch -W .github/workflows/drift.yml -P ubuntu-latest=catthehacker/ubuntu:gh-latest --secret-file <file with GITHUB_TOKEN, STRATA_READ_TOKEN, PALETTE_API_KEY>
```

Each run checked out the caller and this repository at `v0.5.0`, downloaded the
`v0.5.0` binary, checked it against `SHA256SUMS`, and ran the script. At the
baseline, the job succeeded and opened nothing. After an edit in place through
`api.sh edit`, it opened issue #46 with the report, and the job failed with exit
1. After the undo, it closed #46 and succeeded:

```text
baseline  verify-report: 1 records: 1 unchanged, 0 drifted, 0 could not be checked
edited    verify-report: 1 records: 0 unchanged, 1 drifted, 0 could not be checked; opened #46 for records/addon.json
edited    exitcode '1': failure
undone    verify-report: 1 records: 1 unchanged, 0 drifted, 0 could not be checked; closed #46 for records/addon.json
```

## packs.sh

The pack ref checks (#55), run `20260918T003543Z` with strata 0.5.2: read-only, no
cluster, no cost. `just live-packs`. All 15 checks passed.

The tenant holds `csi-aws-ebs` in two registries, `Public Repo` and the OCI
registry `Palette Registry` that the registry-move run added, so a bare
`pack:csi-aws-ebs@1.63.1` is ambiguous and every reading ref names its registry:

```text
strata: pack 'csi-aws-ebs' at version '1.63.1' is in several registries: Palette Registry, Public Repo; name one, as in pack:<registry>/csi-aws-ebs@1.63.1
```

The first run of this script printed those two names in the other order, because
Palette lists the registries in the order it likes. Strata now sorts them, so
the message reads the same on every run.

| result | check | expected exit | got | what it shows |
|---|---|---|---|---|
| PASS | `list-packs` | 0 | 0 | #55: the profiles list the pack versions they pin |
| PASS | `compare-versions` | 1 | 1 | #55: pack:Public Repo/csi-aws-ebs@1.63.1 against @1.65.0 differs |
| PASS | `compare-json` | 1 | 1 | #55: the diff document names the pack on both sides |
| PASS | `compare-same` | 0 | 0 | #55: pack:Public Repo/csi-aws-ebs@1.65.0 against itself is identical |
| PASS | `record` | 0 | 0 | #55: snapshot writes a record of a pack version |
| PASS | `record-title` | 0 | 0 | #55: the record names the pack, not a profile |
| PASS | `verify-record` | 0 | 0 | #55: the record verifies against the registry it came from |
| PASS | `latest` | 0 | 0 | #55: pack:Public Repo/csi-aws-ebs@latest resolves to a version |
| PASS | `latest-is-newest` | 0 | 0 | #55: latest gives the newest version the registry offers |
| PASS | `two-registries` | 2 | 2 | #55: a bare ref for a pack in two registries is an error that names both |
| PASS | `two-registries-says` | 0 | 0 | #55: the error names each registry and the ref that fixes it |
| PASS | `other-registry` | 0 | 0 | #55: the same pack version reads from the other registry too |
| PASS | `registries-differ` | 1 | 1 | #55: the two registries hold different builds of the version |
| PASS | `unknown-version` | 2 | 2 | #55: a version the registry does not have is an error |
| PASS | `unknown-registry` | 2 | 2 | #55: a registry the tenant does not have is an error |

## merge-gate.sh

The merge gate (#57), run `20260918T003905Z` with strata 0.5.2: the live
profiles, no cluster, and one in-place edit that the run undoes. `just
live-merge-gate`. All 15 checks passed.

The repository is local, with a bare origin, so the fetch and the merge are real
git; every record and every verify is live. Main pinned
`strata-live-profile@1.0.0` and then `@1.0.1`, and the branch pins `1.0.0`
again:

```text
BEHIND: releases/dev.json would replace what main pinned with an older version
  the profile: main has 1.0.1, the merge result has 1.0.0
  main pinned this release before, on 2026-09-17 in 53e65a4
  the branch wrote this record after main moved to strata-live-profile@1.0.1
  a rollback says so, in a commit of the branch:
    Rollback: releases/dev.json to strata-live-profile@1.0.0
  or in the rollback label, which gives the job --allow-rollback.
```

With the rollback declared in the branch's commit, the gate passes. Then the
add-on version is edited in place through the API, and the gate goes red on a
record that git cannot fault, while the declared rollback still passes:

```text
DRIFTED: releases/addon.json does not verify; a cluster built from it would not get what it says
  strata-live-addon@1.0.0 (uid 6aac455cb6a81bf97d926581)  ->  strata-live-addon@1.0.0 (uid 6aac455cb6a81bf97d926581)
  root       2d6991fdf460 -> e264fb17e249   CHANGED
  note       both sides are strata-live-addon@1.0.0 (uid 6aac455cb6a81bf97d926581): the version was edited in place
```

| result | check | expected exit | got | what it shows |
|---|---|---|---|---|
| PASS | `record-newer` | 0 | 0 | #57: record strata-live-profile@1.0.1, which main pins |
| PASS | `record-addon` | 0 | 0 | #57: record strata-live-addon@1.0.0, which both sides pin |
| PASS | `record-older` | 0 | 0 | #57: record strata-live-profile@1.0.0, which the branch pins |
| PASS | `behind` | 1 | 1 | #57: the gate is red: the merge result pins the earlier version |
| PASS | `behind-says` | 0 | 0 | #57: the report names the record and both versions |
| PASS | `behind-evidence` | 0 | 0 | #57: the evidence says main pinned this release before |
| PASS | `behind-declares` | 0 | 0 | #57: the report gives the line that declares a rollback |
| PASS | `declared` | 0 | 0 | #57: the gate passes once the commit declares the rollback |
| PASS | `declared-says` | 0 | 0 | #57: the report names it a rollback, declared in a commit |
| PASS | `api-edit` | 0 | 0 | #57: edit the add-on version in place through the API |
| PASS | `drifted` | 1 | 1 | #57: the gate is red on a record Palette no longer matches |
| PASS | `drifted-says` | 0 | 0 | #57: the report names the record that does not verify |
| PASS | `drifted-keeps-rollback` | 0 | 0 | #57: the declared rollback still passes; only the record fails |
| PASS | `api-undo` | 0 | 0 | #57: undo the edit through the API |
| PASS | `restored` | 0 | 0 | #57: the gate passes again once Palette matches the record |

## in-place.sh

`in-place.sh off` and `in-place.sh on` in session `20260917T151426Z`, provider
0.30.1, against cluster A `fred`, with cluster B `barney` created and deleted in
each. The two runs differ only in the immutable-clusterprofiles flag. Both edit
the add-on version through the API, and `plan-edit` records what the provider
does with the same edit. Each started from the baseline and restored it
(`baseline-after`). No check failed: 51 with the flag off and 53 with it on.
Each apply took the flag of the run with no `-target`, except the restore of the
baseline. With the flag on, `hold_versions` changed only `skip_destroy` on the
four profiles. The apply that created cluster B after the edit through the API
changed no profile. The bump kept the first add-on uid (`1 added, 0 changed, 1
destroyed`). Each deletion of cluster B finished with no help, in 19m1s and
19m44s. Each answer is the answer that session `20260917T031126Z` recorded with
provider 0.29.7 and its workaround, except `plan-no-change`, which that provider
refused with the flag on. [What Palette does](../internals/palette.md#cluster-copies)
states what the answers show.

| Check | Flag off | Flag on | What it shows |
|---|---|---|---|
| `a1` | PASS 0 | PASS 0 | record cluster A |
| `a1-addon` | PASS 0 | PASS 0 | record cluster A's add-on selection |
| `release1` | PASS 0 | PASS 0 | record the release version |
| `addon1` | PASS 0 | PASS 0 | record the add-on version |
| `addon1-is-1.0.0` | PASS 0 | PASS 0 | the add-on version is 1.0.0 |
| `a1-updates` | PASS 0 | PASS 0 | what waits for cluster A |
| `a-owns-manifest-copy` | PASS 0 | PASS 0 | cluster A runs its own copy of the add-on manifest, not the profile's |
| `plan-no-change` | PASS 0 | PASS 0 | Terraform plans no change once skip_destroy is held for the flag |
| `plan-edit` | PASS 2 | PASS 1 | #27: Terraform's plan for an in-place edit of the add-on version: an update in place without the flag, a refusal with it |
| `plan-edit-says` | PASS 0 | PASS 0 | the plan says which |
| `api-edit` | PASS 0 | PASS 0 | #27: edit the add-on version in place through the API |
| `a2` | PASS 0 | PASS 0 | record cluster A again, applying nothing |
| `a2-addon` | PASS 0 | PASS 0 | record cluster A's add-on selection |
| `b2-addon` | PASS 0 | PASS 0 | record cluster B's add-on selection |
| `addon2` | PASS 0 | PASS 0 | record the edited add-on version |
| `edit-kept-manifest-uid` | PASS 0 | PASS 0 | the edit kept the add-on manifest's uid, so cluster copies still name it |
| `a2-updates` | PASS 0 | PASS 0 | what waits for cluster A |
| `b-owns-manifest-copy` | PASS 0 | PASS 0 | cluster B was created with its own copy of the add-on manifest |
| `a-kept-old-content` | PASS 0 | PASS 0 | #27: cluster A keeps its copy from before the edit |
| `a-vs-b` | PASS 1 | PASS 1 | #27: cluster A differs from cluster B, created from the edited version |
| `a-vs-b-edited-in-place` | PASS 0 | PASS 0 | #27: the comparison says the version was edited in place |
| `verify-a1` | PASS 0 | PASS 0 | #27: cluster A's record from before the edit still verifies |
| `verify-release1` | PASS 0 | PASS 0 | #23: the release record is untouched by the add-on edit |
| `verify-addon1` | PASS 1 | PASS 1 | #23: the add-on record reports DRIFT after its in-place edit |
| `addon1-vs-a2` | PASS 0 | PASS 0 | #27: the add-on record from before the edit matches cluster A's selection |
| `consumers-addon` | PASS 1 | PASS 1 | both clusters use the add-on version |
| `apply-keeping-notification` | PASS 0 | PASS 0 | push the add-on's content to cluster A without resolving its notification |
| `a-changed-without-resolve` | PASS 1 | PASS 1 | #27: three minutes after that push, cluster A's copy has taken the edit |
| `apply-to-a` | PASS 0 | PASS 0 | #27: accept the waiting update on cluster A, resolving the notification |
| `a3` | PASS 0 | PASS 0 | record cluster A after the apply |
| `a3-updates` | PASS 0 | PASS 0 | what waits for cluster A |
| `a3-matches-b` | PASS 0 | PASS 0 | #27: after the apply, cluster A runs what cluster B runs |
| `verify-a1-after-apply` | PASS 1 | PASS 1 | #27: once the update is applied, cluster A's record from before the edit reports DRIFT |
| `addon-uid-kept` | PASS 0 | PASS 1 | #27: the bump keeps the add-on uid without the flag and makes a new one with it |
| `addon-old4` | PASS 0 | PASS 0 | record what the first add-on uid holds now |
| `addon-new4` | PASS 0 | PASS 0 | record the bumped add-on version |
| `a4-addon` | PASS 0 | PASS 0 | record cluster A's add-on selection |
| `b4-addon` | PASS 0 | PASS 0 | record cluster B's add-on selection |
| `verify-addon1-after-bump` | PASS 1 | PASS 1 | #27: the add-on record from before the edit still reports DRIFT |
| `addon1-overwritten` | PASS 0 | PASS 1 | #27: verify says the recorded version was overwritten, only without the flag |
| `a4-matches-old-version` | PASS 1 | PASS 0 | #23, #27: cluster A's selection equals the first add-on uid only with the flag, which keeps the old version |
| `a4-vs-old-json` | PASS 1 | PASS 0 | the same comparison as JSON |
| `a4-names-overwrite` | PASS 0 | PASS 1 | #29: the comparison names the first add-on uid as overwritten in place, only without the flag |
| `a4-still-a3` | PASS 0 | PASS 0 | #27: the bump left cluster A as it was |
| `verify-a3-after-bump` | PASS 1 | PASS 0 | #30: verify of cluster A's record from stage 3 fails only without the flag: the version it runs was overwritten |
| `a3-names-overwritten-profile` | PASS 0 | PASS 1 | #30: the verify names the add-on profile, only without the flag |
| `consumers-old` | PASS 1 | PASS 1 | #27: both clusters are still consumers of the first add-on uid |
| `consumers-old-overwritten` | PASS 0 | PASS 1 | #28: consumers of the first add-on uid marks both clusters as running a version it no longer names, only without the flag |
| `clone-carried-nothing` | - | PASS 0 | #27, #32: the flag's clone is the same release as the legacy bump of the same configuration, whatever packUid Palette minted |
| `clone-same-content` | - | PASS 0 | #27: the clone has the values, manifests, and variables of the legacy bump of the same configuration |
| `b-gone` | PASS 0 | PASS 0 | cluster B is deleted |
| `a5-matches-addon` | PASS 0 | PASS 0 | cluster A runs the restored add-on version |
| `baseline-after` | PASS 0 | PASS 0 | the live example is back at its baseline |
