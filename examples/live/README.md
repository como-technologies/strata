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

The cluster is billable while it exists. In the recorded run it owned four EC2
instances (the three `t3.xlarge` nodes and a `t3.micro`), a NAT gateway, an
elastic IP, a classic load balancer, a VPC, and four volumes. Creation took 19
minutes. Healthy deletions of this kind of cluster take 9 to 29 minutes. Tear
it down when the checks are done, and confirm it is gone.

## Files

| file | what it does |
|---|---|
| `terraform/` | the two profile versions and the cluster |
| `tf.sh` | runs Terraform with the key and the Palette address strata uses |
| `tenant.sh` | read-only lookups: the names `terraform.tfvars` needs, and a cluster's state |
| `verify.sh` | runs every check and writes `runs/<UTC time>/` |
| `candidate-from-snapshot.jq` | turns a profile record into a candidate file |
| `aws-inventory.sh` | what the cluster still has in AWS, for confirming a teardown |

## Prerequisites

- A Palette tenant with a project, and an AWS cloud account registered in it
- An EC2 key pair in the region; the provider requires one
- A Palette API key in `PALETTE_API_KEY` or `~/.palette.env`
- `terraform` 1.5 or later, `jq`, `just`, `curl`, and a Rust toolchain
- The AWS CLI with read access to the account, only for `aws-inventory.sh`

For a self-hosted Palette, set `PALETTE_API` to its address in the
environment or `~/.palette.env`. Strata reads it, and `tf.sh` and `tenant.sh`
pass it on, so every step talks to the same Palette.

## Steps

1. Find the names from your tenant. `tenant.sh names` lists the projects, the
   AWS cloud accounts, and each account's zones and key pairs in a region.
   The recorded run's tenant answered:

   ```
   $ examples/live/tenant.sh names us-east-2
   projects (palette_project):
     iris-on-palette  uid 6a860681da93f9c74843a019
     strata-test  uid 6aaab0567993b0008ad902db
   AWS cloud accounts (cloud_account_name), with zones (az) and key pairs (ssh_key_name) in us-east-2:
     como-aws
       zones:     us-east-2a, us-east-2b, us-east-2c
       key pairs: iris-on-palette, bastion-iris-on-palette
   ```

   Put them in `terraform.tfvars`: a project name as `palette_project`, an
   account name as `cloud_account_name`, the region, a zone as `az`, and a key
   pair as `ssh_key_name`.

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
   export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
   examples/live/tf.sh apply -var edit_next_in_place=true
   target/release/strata verify examples/live/runs/<time>/next.json    # DRIFT, exit 1
   examples/live/tf.sh apply                                            # undo the edit
   ```

   `verify.sh` sets `PALETTE_PROJECT_UID` for its own checks; a command run by
   hand needs the export. Undo the edit before running `verify.sh` again:
   with it in place, two values differ and `only-the-edited-value` fails.

5. Tear it down, and confirm it is gone. Keep the uids first; the outputs
   disappear with the state.

   ```
   C=$(examples/live/tf.sh output -raw cluster_uid)
   export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
   just live-down
   examples/live/tenant.sh cluster "$C"                 # exit 0: Deleted
   AWS_PROFILE=<profile> examples/live/aws-inventory.sh  # exit 0: nothing left
   ```

   Terraform's "Destroy complete" is not the confirmation. During a gateway
   outage Palette answers a plain-text 404, and the provider can take that
   for a finished deletion while the cluster still runs. A deleted cluster
   answers with state `Deleted`; `tenant.sh cluster` exits 2 on a 404 that is
   not Palette's JSON `ResourceNotFound`.

## When a check fails

`verify.sh` keeps going after a failed check, so one run shows every result.
In `runs/<UTC time>/`:

- `summary.md` has the table of results and the digest table.
- `<check>.out` and `<check>.err` hold what each command printed. A failed
  comparison names the layer and the leaf in its `.out`.
- `raw/cluster`, `raw/release`, and `raw/next` hold the API responses strata
  hashed. Compare them with the SDK models when a shape looks wrong.

Run `just live-verify` again after a fix; each run gets its own directory. The
cluster does not need to be `Running`: the checks read Palette's record of
the cluster, which exists once creation starts.

What went wrong before, and why:

| symptom | cause |
|---|---|
| `live-up` fails with "A minimum allocation of '4.1' CPU cores is required across all worker nodes" | workers total 4 cores or fewer; the default of two `t3.xlarge` workers clears it |
| `snapshot-candidate` fails with `pack-not-found` naming a tag | a strata build older than the tag fix; the profile's tag is the version |
| `consumer-is-the-cluster` fails with state `unknown` | the key cannot read the cluster in its project; `consumers-release.out` prints the error next to the cluster |
| `only-the-edited-value` fails with two changed values | the optional in-place edit was not undone |

## When teardown is slow or stuck

These rules come from six observed teardowns of the same kind of cluster:

- Deletion continues in Palette after Terraform gives up or is interrupted.
  Keep checking with `tenant.sh cluster` instead of starting over.
- Judge progress by `aws-inventory.sh`, not by elapsed time. AWS empties first,
  in about 6 to 12 minutes for a cluster without add-ons, and the state turns
  `Deleted` 3 to 8 minutes after that.
- If the state stays `Deleting` and the inventory stops shrinking for several
  readings, read the cluster's events in the Palette console; the message
  usually names what is stuck. A cluster with Pause Agent Upgrades enabled
  can stay in `Deleting`; disable it and delete again.
- Force delete only when `aws-inventory.sh` exits 0. A force delete removes
  Palette's records and nothing in AWS; forcing earlier leaves resources
  billing with nothing tracking them. Palette refuses a force delete for about
  15 minutes after deletion starts, and can answer 500 before accepting it.
  In the console, or with the key and address exported:

  ```
  set -a; . ~/.palette.env; set +a     # when the key lives there
  curl -X DELETE -H "ApiKey: $PALETTE_API_KEY" -H "ProjectUid: $PALETTE_PROJECT_UID" \
    "${PALETTE_API:-https://api.spectrocloud.com}/v1/spectroclusters/$C?forceDelete=true"
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
