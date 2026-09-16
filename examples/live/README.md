# Live example: strata against a real cluster

The offline examples in this directory's parent synthesize profiles and a
cluster from real registry data. This example builds the real thing in a
Palette tenant and runs every strata check that needs one:

- **#1, release versus run.** A cluster that runs a recorded profile version
  compares `IDENTICAL`. Against a newer record it shows the one change.
- **#12, consumers.** The version the cluster runs lists the cluster with its
  name, project, and state. A version nothing runs lists no consumers.
- **#4, candidates.** A candidate file resolved through registry lookups
  reproduces the published version's root.
- **#3, digests.** The summary records whether each pack-ref digest equals the
  registry digest.

It uses AWS because that is the cloud account the verified run had. Strata
does not care which cloud runs the cluster. To use another, swap the cluster
resource and the OS and CSI packs in `terraform/main.tf`.

## What it builds

`terraform/` creates three things in one Palette project:

| resource | what it is |
|---|---|
| profile `strata-live-profile` 1.0.0 | Ubuntu 24.04, Kubernetes 1.36.2, Cilium 1.20.1, EBS CSI 1.65.0, default values. The cluster runs it. |
| profile `strata-live-profile` 1.0.1 | The same, with the EBS storage class `reclaimPolicy` changed from `Delete` to `Retain`. Nothing runs it. |
| cluster `strata-live-cluster` | One control plane and two workers, `t3.xlarge`, one availability zone. |

The pack versions are those of `examples/profiles/aws-pxk@1.1.0.json`.

The cluster is billable while it exists: three EC2 instances plus the load
balancer and NAT gateway Palette creates. Creation takes about 20 to 40
minutes, and deletion about 15 to 20. Tear it down when the checks are done.

## Prerequisites

- A Palette tenant with a project, and an AWS cloud account registered in it
- An EC2 key pair in the region; the provider requires one
- A Palette API key in `PALETTE_API_KEY` or `~/.palette.env`
- `terraform` 1.5 or later, `jq`, `just`, and a Rust toolchain

## Steps

1. Fill in the names from your tenant:

   ```
   cp examples/live/terraform/terraform.tfvars.example examples/live/terraform/terraform.tfvars
   $EDITOR examples/live/terraform/terraform.tfvars
   ```

   The file is gitignored. It holds names only; the API key never goes in it.

2. Build it. `tf.sh` passes the key to Terraform from the environment or
   `~/.palette.env` without printing it.

   ```
   just live-up
   ```

3. Run the checks. `verify.sh` reads the uids from Terraform's outputs, runs
   each check, and compares each exit code with the expected one. It writes
   every output, the raw API responses, and `summary.md` under
   `examples/live/runs/<UTC time>/`, which is gitignored because raw responses
   can hold variable values. It exits 0 only when every check passes.

   ```
   just live-verify
   ```

4. Optional, for what Palette does with an edit to a published version.
   Nothing runs 1.0.1, so this touches no cluster:

   ```
   examples/live/tf.sh apply -var edit_next_in_place=true
   target/release/strata verify examples/live/runs/<time>/next.json    # DRIFT, exit 1
   examples/live/tf.sh apply                                            # undo the edit
   ```

   Set `PALETTE_PROJECT_UID` to the `project_uid` output first; `verify.sh`
   does that for its own checks.

5. Tear it down:

   ```
   just live-down
   ```

## The checks

| check | expected exit | what it shows |
|---|---|---|
| `snapshot-cluster` | 0 | strata reads what a live cluster runs |
| `cluster-runs-release` | 0 | the cluster reports the release profile version |
| `snapshot-release` | 0 | record the version the cluster runs |
| `compare-release-cluster` | 0 | #1: a cluster that runs the recorded version is `IDENTICAL` |
| `snapshot-next` | 0 | record the newer version |
| `compare-next-cluster` | 1 | #1: against a newer record, the cluster shows the change |
| `only-the-edited-value` | 0 | #1: the only change is `csi-aws-ebs` `reclaimPolicy` |
| `consumers-release` | 1 | #12: the release version has a consumer |
| `consumer-is-the-cluster` | 0 | #12: listed with its name, project, and state |
| `consumers-next` | 0 | #12: nothing runs the newer version |
| `candidate-file` | 0 | turn the newer version's record into a candidate file |
| `snapshot-candidate` | 0 | #4: snapshot the candidate through registry lookups |
| `candidate-same-release` | 0 | #4: the candidate's root equals the published root |
| `verify-release` | 0 | an untouched published version verifies |
| `verify-cluster-record` | 0 | a cluster record verifies against the cluster |

`candidate-from-snapshot.jq` builds the candidate file from a record: the same
packs, tags, registries, values, params, manifests, and variables. Its tags
matter. Palette stores the tag a profile was authored with, and Terraform pins
the version, `24.04`, where the registry names that Ubuntu version
`LTS__24.x`. The first live run found that strata rejected a version used as a
tag; strata now accepts one.

## The recorded run

Run `20260916T153806Z` on 2026-09-16, with strata built from `main`, in a
tenant's `strata-test` project on AWS us-east-2. All 17 checks passed:

| result | check | expected exit | got | what it shows |
|---|---|---|---|---|
| PASS | `snapshot-cluster` | 0 | 0 | strata reads what a live cluster runs |
| PASS | `cluster-runs-release` | 0 | 0 | the cluster reports the release profile version |
| PASS | `snapshot-release` | 0 | 0 | record the version the cluster runs |
| PASS | `compare-release-cluster` | 0 | 0 | #1: a cluster that runs the recorded version is IDENTICAL |
| PASS | `snapshot-next` | 0 | 0 | record the newer version |
| PASS | `compare-next-cluster` | 1 | 1 | #1: against a newer record, the cluster shows the change |
| PASS | `compare-next-cluster-json` | 1 | 1 | the same comparison as JSON |
| PASS | `only-the-edited-value` | 0 | 0 | #1: the only change is csi-aws-ebs reclaimPolicy |
| PASS | `consumers-release` | 1 | 1 | #12: the release version has a consumer |
| PASS | `consumers-release-json` | 1 | 1 | the same listing as JSON |
| PASS | `consumer-is-the-cluster` | 0 | 0 | #12: listed with its name, project, and state |
| PASS | `consumers-next` | 0 | 0 | #12: nothing runs the newer version |
| PASS | `candidate-file` | 0 | 0 | turn the newer version's record into a candidate file |
| PASS | `snapshot-candidate` | 0 | 0 | #4: snapshot the candidate through registry lookups |
| PASS | `candidate-same-release` | 0 | 0 | #4: the candidate's root equals the published root |
| PASS | `verify-release` | 0 | 0 | an untouched published version verifies |
| PASS | `verify-cluster-record` | 0 | 0 | a cluster record verifies against the cluster |

The cluster runs the release:

```
strata-live-profile@1.0.0 (6aaab2f6da23e353e96e8c69)  ->  strata-live-profile@1.0.0 on cluster strata-live-cluster (6aaab352da23e35da8a5a544)
root       eada36c2e3fd -> eada36c2e3fd   IDENTICAL
```

A newer record against the same cluster shows the one value that differs:

```
strata-live-profile@1.0.1 (6aaab2fbb6a8f072d83984d1)  ->  strata-live-profile@1.0.0 on cluster strata-live-cluster (6aaab352da23e35da8a5a544)
root       7d7689e8321c -> eada36c2e3fd   CHANGED
~ [csi  ] csi-aws-ebs                1.65.0             AUTHORED
      values: 1 changed, 0 added, 0 removed
        ~ charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy: "Retain" -> "Delete"
      impact: cluster-wide infra addon update
```

The version the cluster runs lists it, and the candidate reproduces 1.0.1:

```
strata-live-profile@1.0.0 (6aaab2f6da23e353e96e8c69): 1 consumer
  strata-live-cluster  6aaab352da23e35da8a5a544  project 6aaab0567993b0008ad902db  Running
```

```
IDENTICAL      candidate-snapshot.json -> next.json   root 7d7689e8321c
SAME RELEASE: 1 pairs passed
```

Every pack-ref digest equals the registry digest, and each matches
`examples/INDEX.md` for the same version:

| pack | ref digest | registry digest | fed UPSTREAM | |
|---|---|---|---|---|
| ubuntu-aws | `sha256:2df05daa4eb4` | `sha256:2df05daa4eb4` | packRef.digest | equal |
| kubernetes | `sha256:c4f7f20f40c7` | `sha256:c4f7f20f40c7` | packRef.digest | equal |
| cni-cilium-oss | `sha256:917d9de9a696` | `sha256:917d9de9a696` | packRef.digest | equal |
| csi-aws-ebs | `sha256:bd09e8b4187b` | `sha256:bd09e8b4187b` | packRef.digest | equal |

The optional in-place edit changed `volumeBindingMode` on the published 1.0.1.
Palette accepted it: same uid, same version, still published, and no new
version. `strata verify` on the earlier record exited `1`:

```
root       7d7689e8321c -> 6fdc871ef9a6   CHANGED
~ [csi  ] csi-aws-ebs                1.65.0             AUTHORED
        ~ charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].volumeBindingMode: "WaitForFirstConsumer" -> "Immediate"
DRIFT: published version strata-live-profile@1.0.1 differs from the copy recorded 2026-09-16T15:38:09Z
```

What the first attempts ran into, now built into the example:

- Palette refused the cluster with one 4-core worker: "A minimum allocation
  of '4.1' CPU cores is required across all worker nodes". Two workers clear
  it. The cluster then took 19 minutes to reach `Running`.
- The candidate check failed until strata accepted a version used as a tag,
  because that is the tag Palette stored.
