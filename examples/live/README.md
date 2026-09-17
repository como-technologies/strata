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
| cluster `barney` | One control plane and two workers, `t3.xlarge`, one availability zone. |
| profile `strata-live-registries` 1.0.0 | An add-on profile with one chart from a Helm registry (default `nginx` 25.1.5 from `Bitnami`) and one pack from an OCI pack registry that `terraform.tfvars` names. Nothing runs it. |

The pack versions are those of `examples/profiles/aws-pxk@1.1.0.json`.

`in-place.sh` adds cluster `fred`, with the same two profiles, for the length
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
| `terraform/` | the profiles and the cluster, and the variables the two scripts below set |
| `tf.sh` | runs Terraform with the key and the Palette address strata uses |
| `tenant.sh` | read-only lookups: the names `terraform.tfvars` needs, a cluster's state, and a pack's digest read from its Helm or OCI registry |
| `verify.sh` | runs every check and writes `runs/<UTC time>/` |
| `in-place.sh` | the in-place edit test (#27): edits the add-on profile in place, creates cluster B, applies the edit to the cluster, and bumps the version, with the flag off or on |
| `registry-move.sh` | the registry-move check (#24) |
| `api.sh` | the writes the scripts make outside Terraform: an in-place edit, pushing a profile to a cluster, and deleting a profile version |
| `aws-inventory.sh` | what the cluster still has in AWS, for confirming a teardown |

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
   registry. Both scripts write to the tenant, one at a time, and run only
   while no cluster B exists:

   ```
   examples/live/in-place.sh off                                        # #27, legacy provider behavior
   OFF_RUN=examples/live/runs/<time>/in-place/off examples/live/in-place.sh on   # #27, with the flag
   examples/live/registry-move.sh                                       # #24
   ```

   `in-place.sh` is billable: it creates cluster `fred`, which
   takes about 20 minutes, and deletes it at the end, which takes 10 to 30.
   It waits for that cluster to be `Running` before deleting it, because
   Palette does not carry out the deletion of a cluster that is still
   provisioning. `registry-move.sh` runs no cluster and takes a few minutes.

   Each script checks exit codes as `verify.sh` does and writes `summary.md`
   and every output under `runs/<UTC time>/in-place/<off|on>/` or
   `runs/<UTC time>/registry-move/`. An interrupted `in-place.sh` resumes
   with `OUT=<that directory> examples/live/in-place.sh <off|on> <stage>`.

   With the flag on, provider 0.29.7 refuses every plan that includes an
   existing profile at an unchanged version, even one that changes nothing.
   Palette stores the packs a little differently from the configuration: no
   trailing newline on values and manifests, and pack type and registry
   fields filled in. That provider's check counts the difference as a change.
   Both scripts record the refusal as an expected check,
   `plan-refuses-no-change`, until the provider release that fixes it. Until
   then they apply around it: a version bump targets the profile it bumps,
   and an apply that only creates or deletes cluster B runs with the flag
   off, which governs nothing but profiles, and with `keep_profile_versions`
   holding `skip_destroy` on.

5. Tear it down, and confirm it is gone. Keep the uids first; the outputs
   disappear with the state.

   ```
   C=$(examples/live/tf.sh output -raw cluster_uid)
   export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
   just live-down
   examples/live/tenant.sh cluster "$C"                 # exit 0: Deleted
   AWS_PROFILE=<profile> examples/live/aws-inventory.sh  # exit 0: nothing left
   ```

   `live-down` also deletes every version of `strata-live-addon` and
   `strata-live-move`, by exact name: with the flag on, `skip_destroy` keeps
   the versions a bump replaced out of Terraform's state, so a destroy alone
   leaves them in Palette.

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
| `consumer-is-the-cluster` fails with state `unknown` | the key cannot read the cluster in its project; `consumers-release.out` prints the error next to the cluster |
| `only-the-edited-value` fails with two changed values | 1.0.1 was edited in place, with `edit_next_in_place`, and the edit was not undone |
| `plan-refuses-no-change` fails with exit 0 | the provider no longer refuses a flag-on plan that changes nothing: a release with the fix; expect 0 |
| `registry-digest-oci` fails with "crane digest ... failed" | no login to the OCI registry, or an expired one; an AWS registry login lasts 12 hours |
| `ebs-csi-controller` pods in `CrashLoopBackOff`, `UnauthorizedOperation` on `ec2:DescribeAvailabilityZones` | the worker node role lacks `AmazonEBSCSIDriverPolicy`; see Prerequisites |

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

Run `20260917T002933Z` on 2026-09-17, with strata built from `main`, in a
tenant's `strata-test` project on AWS us-east-2, against cluster `barney`.
All 24 checks passed:

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
strata-live-profile@1.0.0 (uid 6aab30f2b6a8fe13e4abb1d4)  ->  strata-live-profile@1.0.0 on cluster barney (uid 6aab30f89dde75fd5dcb6256)
root       eada36c2e3fd -> eada36c2e3fd   IDENTICAL
```

```
strata-live-addon@1.0.0 (uid 6aab30f39dde75fcac792fe7)  ->  strata-live-addon@1.0.0 on cluster barney (uid 6aab30f89dde75fd5dcb6256)
root       7d1079372122 -> 7d1079372122   IDENTICAL
```

A newer record against the same cluster shows the one value that differs:

```
strata-live-profile@1.0.1 (uid 6aab30f8da23f105f62d4451)  ->  strata-live-profile@1.0.0 on cluster barney (uid 6aab30f89dde75fd5dcb6256)
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
strata-live-profile@1.0.0 (uid 6aab30f2b6a8fe13e4abb1d4): 1 consumer
  barney  6aab30f89dde75fd5dcb6256  project 6aaab0567993b0008ad902db  Running
```

```
IDENTICAL      candidate-snapshot.json -> next.json   root 7d7689e8321c
SAME RELEASE: 1 pairs passed
```

Every pack-ref digest equals the digest Palette lists for the pack version.
The four `spectro` digests match `examples/INDEX.md` for the same versions.
For the Helm and OCI packs, `tenant.sh digest` read the same digest from the
registry itself: the sha256 of the `nginx` 25.1.5 chart archive, and the
manifest digest of `argocd` 10.4.0.1:

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

Run `20260917T003010Z`, `registry-move.sh`, with the flag on. All 15 checks
passed. The bump re-pinned `csi-aws-ebs` 1.65.0 from `Public Repo` to the OCI
registry `Palette Registry`, which changed its type from `spectro` to `oci`,
its registry uid, its pack uid, and its digest, and nothing else
(`docs/model.md`, "A real registry move"). `same-release --allow
registry-move` passed the pair as `registry-move` with 1 pack re-pinned, and
failed it after an in-place edit of 1.0.1's values, naming only
`pack csi/csi-aws-ebs AUTHORED: charts.aws-ebs-csi-driver.storageClasses[name=spectro-storage-class].reclaimPolicy`.

| result | check | expected exit | got | what it shows |
|---|---|---|---|---|
| PASS | `plan-refuses-no-change` | 1 | 1 | expected with provider 0.29.7: with the flag on, Terraform refuses a plan that changes nothing |
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

