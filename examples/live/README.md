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
- **#2, Helm and OCI digests.** The same comparison for a pack from a Helm
  registry and one from an OCI registry, with the digest each registry serves
  read from the registry itself.
- **#23, a cluster with two profiles.** The cluster runs the release profile
  and an add-on profile; each profile's record compares `IDENTICAL` against
  its selection.
- **#27, in-place edits and version bumps.** `in-place.sh` edits the add-on
  profile in place and bumps its version while two clusters run it, with the
  Terraform provider's `immutable-clusterprofiles` flag off and on.
- **#24, a real registry move.** `registry-move.sh` moves a profile's CSI pack
  to an OCI registry as a version bump and runs `same-release --allow
  registry-move` on the pair.

It uses AWS because that is the cloud account the verified run had. Strata
does not care which cloud runs the cluster. To use another, swap the cluster
resource and the OS and CSI packs in `terraform/main.tf`.

## What it builds

`terraform/` creates five things in one Palette project:

| resource | what it is |
|---|---|
| profile `strata-live-profile` 1.0.0 | Ubuntu 24.04, Kubernetes 1.36.2, Cilium 1.20.1, EBS CSI 1.65.0, default values. The cluster runs it. |
| profile `strata-live-profile` 1.0.1 | The same, with the EBS storage class `reclaimPolicy` changed from `Delete` to `Retain`. Nothing runs it. |
| profile `strata-live-addon` 1.0.0 | An add-on profile with one manifest, the ConfigMap `strata-live-config` in `default`. The cluster runs it after the release profile. |
| cluster `fred` | One control plane and two workers, `t3.xlarge`, one availability zone. |
| profile `strata-live-registries` 1.0.0 | An add-on profile with one chart from a Helm registry (default `nginx` 25.1.5 from `Bitnami`) and one pack from an OCI pack registry that `terraform.tfvars` names. Nothing runs it. |

The pack versions are those of `examples/profiles/aws-pxk@1.1.0.json`, which
`just examples` builds.

`in-place.sh` adds cluster `barney`, with the same two profiles, for the length
of its run. The two cluster names share no part, so no prefix or substring
match finds both. `registry-move.sh` adds profile `strata-live-move` 1.0.0 and
1.0.1, with the release packs, and deletes both at its end.

The cluster is billable while it exists. In the recorded run it owned four EC2
instances (the three `t3.xlarge` nodes and a `t3.micro`), a NAT gateway, an
elastic IP, a classic load balancer, a VPC, and four volumes. Creation took 15
to 19 minutes. Healthy deletions of this kind of cluster take 9 to 29 minutes. Tear
it down when the checks are done, and confirm it is gone.

## Files

| file | what it does |
|---|---|
| `terraform/` | the profiles and the clusters, and the variables the scripts below set |
| `lib.sh` | what the scripts share: the checks and their summary, the Terraform applies with the one provider workaround, waiting for a cluster, and the baseline |
| `baseline.sh` | checks that the example is at its baseline; every script requires it before it starts and after it ends |
| `tf.sh` | runs Terraform with the key and the Palette address strata uses |
| `tenant.sh` | read-only lookups: the names `terraform.tfvars` needs, a cluster's state and the conditions that hold it, a cluster's diagnostics, whether Palette would repave a cluster for a move to another profile version, and a pack's digest read from its Helm or OCI registry |
| `diagnose.sh` | saves diagnostics for every cluster in Terraform's state; `in-place.sh`, `just live-up`, and `just live-down` run it when an apply fails or a wait gives up |
| `verify.sh` | runs every read-only check and writes `runs/<UTC time>/` |
| `in-place.sh` | the in-place edit test (#27): edits the add-on profile in place, creates cluster B, applies the edit to cluster A, and bumps the version, once with the flag off and once with it on |
| `registry-move.sh` | the registry-move check (#24) |
| `drift-report.sh` | the drift report test (#25): `scripts/verify-report.sh` around an in-place edit and a version bump of the add-on profile, with the flag off or on; issues go to this repository under the label `strata-live-drift` |
| `tag-only.sh` | the tag-only test (#19): versions that differ from 1.0.0 only in the OS pack's tag name, a move of cluster A to one and back, watched for a repave, and an in-place rename of the tag |
| `api.sh` | the writes the scripts make outside Terraform: an in-place edit, pushing a profile to a cluster, and deleting profile versions |
| `aws-inventory.sh` | what the cluster still has in AWS, for confirming a teardown |

## Design

The example is a controlled experiment: each script changes one thing and
holds the rest, so a difference in its results has one cause.

**The baseline** is the state `just live-up` builds, and `baseline.sh`
checks it: a Terraform plan with the default variables changes nothing,
cluster A (`fred`) is `Running` and runs the release and add-on profiles'
current content, no cluster B (`barney`) exists, and each profile name holds
exactly the versions Terraform manages. Every script requires it before it
starts and leaves it as it found it.

| script | varies | holds |
|---|---|---|
| `verify.sh` | nothing; it only reads | the baseline |
| `in-place.sh off` and `on` | the provider's `immutable-clusterprofiles` flag, in every plan and in every apply that changes a profile | the in-place edit (`api.sh edit` in both runs), the clusters, the steps and their order, the baseline it starts from and restores |
| `registry-move.sh` | the move profile's CSI registry, then its values | the flag (on), the other packs, the baseline |
| `drift-report.sh off` and `on` | the provider's `immutable-clusterprofiles` flag, in the plan and in the bump | the edit (`api.sh edit` in both runs), the records, the steps and their order, the issue label, the baseline |
| `tag-only.sh` | the tag name the OS pack is pinned under, and which of two such versions cluster A runs | the pack uid, every other pack, the values, the flag (off), the baseline |

**One definition per kind.** Clusters A and B come from one Terraform
resource and differ only in their names, which share no part. Every profile
has the same lifecycle: `skip_destroy` follows the flag, and
`create_before_destroy` is on.

**What Terraform does is a check, not a step.** The in-place edit goes
through the API in both `in-place.sh` runs, because Terraform refuses it with
the flag on; `plan-edit` records what Terraform does with the same edit.

**One workaround, labeled.** Provider 0.29.7 refuses a flag-on plan that
changes nothing (`plan-no-change`). `lib.sh` works around it in one place,
and names what to remove when a provider release fixes it.

## Prerequisites

- A Palette tenant with a project, and an AWS cloud account registered in it
- An EC2 key pair in the region; the provider requires one
- A Palette API key in `PALETTE_API_KEY` or `~/.palette.env`
- `terraform` 1.5 or later, `jq`, `just`, `curl`, `helm`, `crane`, and a Rust toolchain
- Read access to an OCI pack registry in the tenant, and a login to it that
  `crane` can use (`crane auth login` or `docker login`). The Helm registry
  must serve charts without credentials; `Bitnami` does.
- The AWS CLI with read access to the account, only for `aws-inventory.sh`
- The AWS managed policy `AmazonEBSCSIDriverPolicy` attached to the account's
  worker node role, `nodes.cluster-api-provider-aws.sigs.k8s.io`, so the EBS
  CSI pack runs as it ships. Without it, its controller crash-loops on every
  cluster with `UnauthorizedOperation` on `ec2:DescribeAvailabilityZones`. No
  strata check depends on volumes, and Palette still reports such a cluster
  `Running`; the recorded runs before 2026-09-17 01:35 UTC ran without it.

For a self-hosted Palette, set `PALETTE_API` to its address in the
environment or `~/.palette.env`. Strata reads it, and `tf.sh` and `tenant.sh`
pass it on, so every step talks to the same Palette.

## Steps

1. Find the names from your tenant. `tenant.sh names` lists the projects, the
   AWS cloud accounts, each account's zones and key pairs in a region, the
   Helm registries, and the OCI pack registries. The recorded run's tenant
   answered:

   ```
   $ examples/live/tenant.sh names us-east-2
   projects (palette_project):
     iris-on-palette  uid 6a860681da93f9c74843a019
     strata-test  uid 6aaab0567993b0008ad902db
   AWS cloud accounts (cloud_account_name), with zones (az) and key pairs (ssh_key_name) in us-east-2:
     como-aws
       zones:     us-east-2a, us-east-2b, us-east-2c
       key pairs: iris-on-palette, bastion-iris-on-palette
   Helm registries (helm_registry_name):
     Bitnami
     Public Spectro Helm Repo
   OCI pack registries (oci_registry_name):
     Palette Registry  415789037893.dkr.ecr.us-east-1.amazonaws.com
     Palette Community Registry  415789037893.dkr.ecr.us-east-1.amazonaws.com
     iris-packs  517293881804.dkr.ecr.us-east-2.amazonaws.com
   ```

   Put them in `terraform.tfvars`: a project name as `palette_project`, an
   account name as `cloud_account_name`, the region, a zone as `az`, and a key
   pair as `ssh_key_name`. Name an OCI pack registry you can log in to as
   `oci_registry_name`, and an add-on pack in it with its version as
   `oci_pack` and `oci_pack_version`. The Helm chart defaults to `nginx`
   25.1.5 from `Bitnami`; `helm_registry_name`, `helm_chart`, and
   `helm_chart_version` change it.

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

   Log in to the OCI pack registry first, so `crane` can read the pack's
   digest. For a registry in AWS:

   ```
   aws ecr get-login-password --region <region> | crane auth login -u AWS --password-stdin <registry endpoint>
   just live-verify
   ```

4. Edit a published version that clusters run, and move a pack to another
   registry. The scripts write to the tenant, so run them one at a time. Each
   starts from the baseline (see "Design") and restores it, so they run in
   any order:

   ```
   examples/live/in-place.sh off                                        # #27, legacy provider behavior
   OFF_RUN=examples/live/runs/<time>/in-place/off examples/live/in-place.sh on   # #27, with the flag
   examples/live/registry-move.sh                                       # #24
   ```

   `just live-session` runs `verify.sh` and these three in that order under
   one `runs/<UTC time>/` directory, and stops at the first that fails. The
   recorded runs below come from it.

   `in-place.sh` is billable: it creates cluster `barney`, which
   takes about 20 minutes, and deletes it at the end, which takes 10 to 30.
   It waits for that cluster to be `Running` before deleting it, because
   Palette does not carry out the deletion of a cluster that is still
   provisioning. `registry-move.sh` runs no cluster and takes a few minutes.

   Each script checks exit codes as `verify.sh` does and writes `summary.md`
   and every output under `runs/<UTC time>/in-place/<off|on>/` or
   `runs/<UTC time>/registry-move/`. An interrupted `in-place.sh` resumes
   with `OUT=<that directory> examples/live/in-place.sh <off|on> <stage>`.

   With the flag on, provider 0.29.7 refuses every plan that includes an
   existing profile at an unchanged version, even one that changes nothing:
   Palette stores the packs a little differently from the configuration (no
   trailing newline on values and manifests, pack type and registry fields
   filled in), and that provider counts the difference as a change. The
   check `plan-no-change` records it. `lib.sh` holds the one workaround, and
   says what to remove once a provider release fixes it.

5. Tear it down, and confirm it is gone. Keep the uids first; the outputs
   disappear with the state.

   ```
   C=$(examples/live/tf.sh output -raw cluster_uid)
   export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
   just live-down
   examples/live/tenant.sh cluster "$C"                 # exit 0: Deleted
   AWS_PROFILE=<profile> examples/live/aws-inventory.sh  # exit 0: nothing left
   ```

   `live-down` also deletes every version of every profile name the example
   uses (the `profile_names` output), by exact name: with the flag on,
   `skip_destroy` keeps the versions a bump replaced out of Terraform's
   state, so a destroy alone leaves them in Palette.

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
- `raw/cluster`, `raw/release`, `raw/next`, and `raw/registries` hold the API responses strata
  hashed. Compare them with the SDK models when a shape looks wrong.

Run `just live-verify` again after a fix; each run gets its own directory. The
cluster does not need to be `Running`: the checks read Palette's record of
the cluster, which exists once creation starts.

What went wrong before, and why:

| symptom | cause |
|---|---|
| `live-up` fails with "A minimum allocation of '4.1' CPU cores is required across all worker nodes" | workers total 4 cores or fewer; the default of two `t3.xlarge` workers clears it |
| `snapshot-candidate` fails with `pack-not-found` naming a tag | a strata build older than the tag fix; the profile's tag is the version |
| a flag-on bump fails with `DeletionResourceInUseError: Unable to delete the resource as fred spectrocluster(s) in-use` | the profile's state held `skip_destroy = false`, and Terraform deletes a replaced version with the value its state holds, not the new one (provider 0.29.7). `apply_profile` now stops before such an apply; run `hold_versions` first (`lib.sh`) |
| a script stops with "target/release/strata is older than <file>; run just build" | the sources changed after the last release build. A script run directly, not through a `just` recipe, does not build first; without this check the `drift-report.sh off` run of 2026-09-17 ran a binary without `verify --summary` and failed 18 checks. `just build`, then run it again |
| `consumer-is-the-cluster` fails with state `unknown` | the key cannot read the cluster in its project; `consumers-release.out` prints the error next to the cluster |
| `only-the-edited-value` fails with two changed values | 1.0.1 was edited in place and the edit was not undone; `baseline.sh` names it |
| `plan-no-change` fails with exit 0 with the flag on | the provider no longer refuses a flag-on plan that changes nothing: a release with the fix. Remove the workaround in `lib.sh` and expect 0 |
| a script stops with "not at its baseline" | an earlier run did not finish, or something changed the example by hand; `baseline-before.txt` in the run names each difference. `just live-down && just live-up` rebuilds it |
| `registry-digest-oci` fails with "crane digest ... failed" | no login to the OCI registry, or an expired one; an AWS registry login lasts 12 hours |
| `ebs-csi-controller` pods in `CrashLoopBackOff`, `UnauthorizedOperation` on `ec2:DescribeAvailabilityZones` | the worker node role lacks `AmazonEBSCSIDriverPolicy`; see Prerequisites |
| a cluster stays `Provisioning` with `not done: WorkerNodeAdditionDone: provision worker nodes`, although its machines are `Running` and its nodes `Ready` | Palette did not check the worker nodes again after a node that was still joining failed its first check (`fred`, 2026-09-17); restarting the Palette controller in the cluster did not clear it. Its deletion ran normally: delete it, and send its diagnostics to Palette support |

## When teardown is slow or stuck

These rules come from the observed teardowns of the same kind of cluster:

- First save what holds the cluster: `examples/live/diagnose.sh` (with
  `AWS_PROFILE` set, for the AWS inventory too), or `tenant.sh diagnose <uid>
  <dir>` for one cluster. Once the cluster is gone, so is the evidence.
- Never delete a cluster that is still provisioning, unless its machines are
  `Running` and its nodes `Ready`: Palette then does not carry out the
  deletion, and every cloud resource stays in place.
- Deletion continues in Palette after Terraform gives up or is interrupted.
  Keep checking with `tenant.sh cluster` instead of starting over. While the
  state is not `Running` or `Deleted`, it prints the conditions not yet done
  and the three latest events, so a deletion that is moving shows it: the
  last AWS steps, then `Cluster <name> has been deleted`.
- Judge progress by `aws-inventory.sh`, not by elapsed time, and only 0 on
  every line means AWS is empty. The instances, NAT gateway, and load balancer
  go first; the security groups, route tables, subnets, internet gateway, and
  VPC go last, some after a retry. Palette then logs the event `Cluster <name>
  has been deleted`, but the state stays `Deleting` for several minutes more,
  and Terraform waits for the state. On 2026-09-17 cluster B (`barney`, run
  `20260917T031126Z`) was deleted at 03:33:11 UTC; its VPC went and the event
  came at 03:44:39, with `aws-inventory.sh` at 0 on every line; the state
  turned `Deleted` and Terraform finished at 03:52, 19 minutes after the
  delete, with nothing done by hand. A force delete in that gap does no harm
  once the inventory is empty, and gains a few minutes at most. Before that day `aws-inventory.sh` counted only instances,
  NAT gateways, VPCs, elastic IPs, volumes, and the load balancer, so it could
  read 0 while the network was still being deleted, and a healthy deletion
  looked stuck.
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
| `cluster-runs-two-profiles` | 0 | #23: the cluster runs the release profile, then the add-on profile |
| `snapshot-release` | 0 | record the version the cluster runs |
| `compare-release-cluster` | 0 | #1: a cluster that runs the recorded version is `IDENTICAL` |
| `snapshot-addon` | 0 | record the add-on profile version the cluster runs |
| `compare-addon-cluster` | 0 | #23: the add-on record is IDENTICAL to the cluster's add-on selection |
| `snapshot-next` | 0 | record the newer version |
| `compare-next-cluster` | 1 | #1: against a newer record, the cluster shows the change |
| `only-the-edited-value` | 0 | #1: the only change is `csi-aws-ebs` `reclaimPolicy` |
| `consumers-release` | 1 | #12: the release version has a consumer |
| `consumer-is-the-cluster` | 0 | #12: listed with its name, project, and state |
| `consumers-next` | 0 | #12: nothing runs the newer version |
| `candidate-file` | 0 | #21: write the newer version's record as a candidate file |
| `snapshot-candidate` | 0 | #4: snapshot the candidate through registry lookups |
| `candidate-same-release` | 0 | #4: the candidate's root equals the published root |
| `verify-release` | 0 | an untouched published version verifies |
| `verify-cluster-record` | 0 | a cluster record verifies against the cluster |
| `snapshot-registries` | 0 | record the profile with a Helm pack and an OCI pack |
| `registries-kinds` | 0 | #2: the profile holds one helm pack and one oci pack |
| `registry-digest-helm` | 0 | #2: read the helm pack's digest from the registry itself |
| `registry-digest-oci` | 0 | #2: read the oci pack's digest from the registry itself |

`strata candidate next.json -o candidate/profile.yaml --files-dir
candidate/files` writes the candidate file from a record: the same packs,
tags, registries, params, and variables, with each layer's values and each
manifest in a file under `candidate/files`. Its tags matter. Palette stores
the tag a profile was authored with, and Terraform pins the version, `24.04`,
where the registry names that Ubuntu version `LTS__24.x`. The first live run
found that strata rejected a version used as a tag; strata now accepts one.

## The recorded run

Session `20260917T031126Z` on 2026-09-17: one `just live-up`, then `just
live-session`, in a tenant's `strata-test` project on AWS us-east-2, with
cluster A `fred` and cluster B `barney`. The four scripts ran 144 checks, and
none failed: `verify.sh` 24, `in-place.sh off` 51, `in-place.sh on` 53, and
`registry-move.sh` 16. Nothing was done by hand. `verify.sh`:

| result | check | expected exit | got | what it shows |
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

The cluster runs the release, and the add-on profile after it; each record
compares `IDENTICAL` against its selection:

```
strata-live-profile@1.0.0 (uid 6aab5674b6a80225b1a53a66)  ->  strata-live-profile@1.0.0 on cluster fred (uid 6aab5678b6a8022628c8c750)
root       eada36c2e3fd -> eada36c2e3fd   IDENTICAL
```

```
strata-live-addon@1.0.0 (uid 6aab5674da23f51fbe101d42)  ->  strata-live-addon@1.0.0 on cluster fred (uid 6aab5678b6a8022628c8c750)
root       9c68e5319d53 -> 9c68e5319d53   IDENTICAL
```

A newer record against the same cluster shows the one value that differs:

```
strata-live-profile@1.0.1 (uid 6aab5678b6a8022625c1992d)  ->  strata-live-profile@1.0.0 on cluster fred (uid 6aab5678b6a8022628c8c750)
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

The version the cluster runs lists it, and the candidate written from the
1.0.1 record, with its values in files, reproduces 1.0.1:

```
strata-live-profile@1.0.0 (uid 6aab5674b6a80225b1a53a66): 1 consumer
  fred  6aab5678b6a8022628c8c750  project 6aaab0567993b0008ad902db  Running
```

```
IDENTICAL      candidate-snapshot.json -> next.json   root 7d7689e8321c
SAME RELEASE: 1 pairs passed
```

Every pack-ref digest equals the digest Palette lists for the pack version.
The four `spectro` digests match `examples/INDEX.md`, which `just examples`
builds, for the same versions. For the Helm and OCI packs, `tenant.sh digest`
read the same digest from the registry itself: the sha256 of the `nginx`
25.1.5 chart archive, and the manifest digest of `argocd` 10.4.0.1:

| pack | type | ref digest | Palette's digest | registry's digest | fed UPSTREAM | |
|---|---|---|---|---|---|---|
| ubuntu-aws | spectro | `sha256:2df05daa4eb4` | `sha256:2df05daa4eb4` | - | packRef.digest | equal |
| kubernetes | spectro | `sha256:c4f7f20f40c7` | `sha256:c4f7f20f40c7` | - | packRef.digest | equal |
| cni-cilium-oss | spectro | `sha256:917d9de9a696` | `sha256:917d9de9a696` | - | packRef.digest | equal |
| csi-aws-ebs | spectro | `sha256:bd09e8b4187b` | `sha256:bd09e8b4187b` | - | packRef.digest | equal |
| nginx | helm | `08816cde6bea` | `08816cde6bea` | `08816cde6bea` | packRef.digest | equal |
| argocd | oci | `sha256:99cac22f712b` | `sha256:99cac22f712b` | `sha256:99cac22f712b` | packRef.digest | equal |

In the earlier run `20260916T153806Z`, the optional in-place edit changed
`volumeBindingMode` on the published 1.0.1. Palette accepted it: same uid,
same version, still published, and no new version. `strata verify` on the
earlier record exited `1`:

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

## The registry-move run

`registry-move.sh` in session `20260917T031126Z`, with the flag on. All 16
checks passed. The bump re-pinned `csi-aws-ebs` 1.65.0 from `Public Repo` to
the OCI registry `Palette Registry`, which changed its type from `spectro` to
`oci`, its registry uid, its pack uid, and its digest, and nothing else
(`docs/model.md`, "A real registry move"). `same-release --allow
registry-move` passed the pair as `registry-move` with 1 pack re-pinned, and
failed it after an in-place edit of 1.0.1's values, naming only
`pack csi/csi-aws-ebs AUTHORED: charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`. The run
started from the baseline and ended at it (`baseline-after`).

| result | check | expected exit | got | what it shows |
|---|---|---|---|---|
| PASS | `plan-no-change` | 1 | 1 | provider 0.29.7 refuses a flag-on plan that changes nothing (lib.sh) |
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

## The tag-only run

`AWS_PROFILE=<profile> tag-only.sh` in run `20260917T124102Z`, with the flag
off. All 32 checks passed. Versions 1.0.2 and 1.0.3 differ from 1.0.0 only
in the tag name the OS pack is pinned under, `LTS__24.04` and `LTS__24.x`,
with one pack uid and version for all three. `compare` calls each pair
`SAME CONTENT` with the impact `none (tag renamed, same pack build)`, and
`same-release` passes the three. Palette's own repave check answers no repave
for either move and a repave for the control. Cluster A ran 1.0.2 and then
1.0.0 again, each for 15 minutes, on the same four EC2 instances throughout.
After the rename of 1.0.2's tag in place, `verify` of its earlier record
reports `DRIFT` with no node impact (`docs/model.md`, "A tag name for the
same build"). The first run, `20260917T044135Z`, recorded the same answers
before the expectations were set.

```
strata-live-profile@1.0.0 (uid 6aab5674b6a80225b1a53a66)  ->  strata-live-profile@1.0.2 (uid 6aabe001b6a811035f4fe2a5)
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

| result | check | expected exit | got | what it shows |
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

## The in-place runs

`in-place.sh off` and `in-place.sh on` in session `20260917T031126Z`, against
cluster A `fred`, with cluster B `barney` created and deleted in each. The two
runs differ only in the provider's `immutable-clusterprofiles` flag: both edit
the add-on version through the API, and `plan-edit` records what Terraform
does with the same edit. Each started from the baseline and restored it
(`baseline-after`). No check failed: 51 with the flag off and 53 with it on.
Each deletion of cluster B finished without help, in about 19 minutes; see
"When teardown is slow or stuck". `docs/model.md`, "A version that clusters
run", states what the answers show.

| check | flag off | flag on | what it shows |
|---|---|---|---|
| `a1` | PASS 0 | PASS 0 | record cluster A |
| `a1-addon` | PASS 0 | PASS 0 | record cluster A's add-on selection |
| `release1` | PASS 0 | PASS 0 | record the release version |
| `addon1` | PASS 0 | PASS 0 | record the add-on version |
| `addon1-is-1.0.0` | PASS 0 | PASS 0 | the add-on version is 1.0.0 |
| `a1-updates` | PASS 0 | PASS 0 | what waits for cluster A |
| `a-owns-manifest-copy` | PASS 0 | PASS 0 | cluster A runs its own copy of the add-on manifest, not the profile's |
| `plan-no-change` | PASS 0 | PASS 1 | Terraform plans no change at the baseline; provider 0.29.7 refuses that plan with the flag on (lib.sh) |
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
