# The live example

The offline examples build profiles and a cluster from real registry data. The
live example builds the real objects in a Palette tenant, and runs each strata
check that needs them. [What the live example verifies](./live-runs.md) lists
those checks, and [What Palette does](../internals/palette.md) states what they
show.

| Script | Checks |
| --- | --- |
| `verify.sh` | Release against run (#1), consumers (#12), the profiles a cluster applies (#63), candidates (#4, #21), pack-ref digests (#2, #3), a cluster with two profiles (#23). Read-only. |
| `in-place.sh off`, `in-place.sh on` | An edit in place and a version bump of a version that two clusters run, with the `immutable-clusterprofiles` flag off and on (#27). |
| `registry-move.sh` | A pack moved to an OCI registry, and `same-release --allow registry-move` (#24). |
| `tag-only.sh` | Versions that differ only in a tag name, a cluster move, and a rename in place (#19). |
| `drift-report.sh off`, `drift-report.sh on` | `scripts/ci/verify-report.sh` around an edit in place and a version bump (#25). |
| `packs.sh` | `pack:` refs against the tenant's own registries (#55). Read-only, and it needs no cluster. |
| `merge-gate.sh` | `scripts/ci/merge-gate.sh` over records of the live profiles, in a git repository that the script builds (#57). It needs the profiles, and no cluster. |
| `knurl.sh` | [The knurl adapter](../automation/knurl.md): each method of the port against the strata answer for the same real object. Read-only. It needs the profiles; the instance checks need the cluster. |

The example uses AWS, because the recorded runs had an AWS cloud account. Strata
makes no rule about which cloud runs the cluster. For another cloud, change the cluster
resource and the OS and CSI packs in `terraform/main.tf`.

## What it builds

`terraform/` creates these objects in one Palette project:

| Resource | What it is |
| --- | --- |
| profile `strata-live-profile` 1.0.0 | Ubuntu 24.04, Kubernetes 1.36.2, Cilium 1.20.1, EBS CSI 1.65.0, default values. Cluster A runs it. |
| profile `strata-live-profile` 1.0.1 | The same, with the EBS storage class `reclaimPolicy` `Retain` in place of `Delete`. No cluster runs it. |
| profile `strata-live-addon` 1.0.0 | An add-on profile with one manifest, the ConfigMap `strata-live-config` in `default`, and one variable, `purpose`, with a default. Cluster A runs it after the release profile. |
| cluster `fred` (cluster A) | One control plane node and two workers, `t3.xlarge`, one availability zone. |
| profile `strata-live-registries` 1.0.0 | An add-on profile with one chart from a Helm registry (default `nginx` 25.1.5 from `Bitnami`), and one pack from the OCI pack registry that `terraform.tfvars` names. No cluster runs it. |
| registry `strata-live-registry` | The example's own OCI pack registry: two ECR repositories, `strata-live/spectro-packs/archive/strata-live-pack` and `.../strata-live-dep`, in the AWS account that `aws_profile` names, a user that reads them, and a Palette registry over them. `registry.sh` pushes into it; nothing else does. |
| profile `strata-live-owned` 1.0.0 | An add-on profile with the one pack of that registry. `registry.sh` makes it after the first push, and removes it at its end. |
| profile `strata-live-dep` 1.0.0 | An add-on profile with `strata-live-pack` 1.1.0, which requires `strata-live-dep`, and that pack. `registry.sh` makes it after the pushes, and removes it at its end. |
| registry `strata-live-helm-registry` | The same repository host, read by Palette as an OCI Helm registry, with one chart repository. `registry.sh` pushes into it. |
| profile `strata-live-owned-helm` 1.0.0 | An add-on profile with the one chart of that registry. `registry.sh` makes it after the first push, and removes it at its end. |
| policy `strata-live-maintenance`, template `strata-live-template` | A maintenance policy, and a cluster template that links the release version and the add-on version. `template.sh` makes them before it builds a cluster, and removes them at its end. |
| profile `strata-live-addon2` 1.0.0 | A second add-on profile with one manifest. `template.sh` runs it on the cluster beside the first, for the install order of add-on packs. |

The pack versions are those of `examples/profiles/aws-stack@1.1.0.json`.

`in-place.sh` adds cluster `barney` (cluster B), with the same two profiles, for
the length of its run. The two cluster names share no part, so no match by
prefix or substring finds both. `registry-move.sh` adds the profile
`strata-live-move` 1.0.0 and 1.0.1 with the release packs, and deletes both at
its end.

> **The cluster costs money while it exists.** In the recorded run, it had four
> EC2 instances (three `t3.xlarge` nodes and a `t3.micro`), a NAT gateway, an
> elastic IP, a classic load balancer, a VPC, and four volumes. Creation took 15
> to 19 minutes. A deletion took 9 to 29 minutes. Remove it when the checks are
> done, and make sure that it is gone.

## Files

| File | Function |
| --- | --- |
| `terraform/` | The profiles and the clusters, and the variables that the scripts set. |
| `tofu-install.sh` | Installs the pinned OpenTofu. `just tofu-install` runs it. |
| `tf.sh` | Runs `tofu` with the key and the Palette address that strata uses. |
| `lib.sh` | What the scripts share: the checks and their summary, the applies, the wait for a cluster, and the baseline. |
| `baseline.sh` | Checks that the example is at its baseline. Each script needs it before it starts and after it ends. |
| `tenant.sh` | Read-only lookups: the names for `terraform.tfvars`, the state of a cluster and what holds it, the cluster listing itself, diagnostics, the repave check of Palette, and a pack digest from its Helm or OCI registry. |
| `diagnose.sh` | Saves diagnostics for each cluster in the state. The scripts and `just live-up` and `just live-down` run it when an apply fails or a wait stops. |
| `verify.sh` | Runs each read-only check, and writes `runs/<UTC time>/`. |
| `in-place.sh` | The edit in place test (#27). |
| `registry-move.sh` | The registry move test (#24). |
| `tag-only.sh` | The tag name test (#19). |
| `drift-report.sh` | The drift report test (#25). It opens and closes issues in this repository with the label `strata-live-drift`. |
| `api.sh` | The writes outside OpenTofu: an edit in place, a push of a profile to a cluster, the delete of profile versions, and the delete of a cluster. |
| `packs.sh` | The pack ref test (#55). |
| `merge-gate.sh` | The merge gate test (#57). |
| `knurl.sh` | The knurl adapter test. |
| `registry.sh` | The registry test (#66): pushes into the example's own OCI pack registry and OCI Helm registry, and reads what Palette does with each push. |
| `pack/`, `chart/` | The two packs and the one chart that `registry.sh` pushes. |
| `template.sh` | The cluster template test (#66): the lock on a linked version, a cluster outside the template, and the install order of add-on packs. It ends with the removal of the example. |
| `cluster.sh` | The cluster test (#66): a change of the value the cluster gives a variable, and a deletion through the API. It ends with the cluster gone. |
| `aws-inventory.sh` | What the cluster still has in AWS, to confirm a teardown. |

## Design

The example is a controlled experiment. Each script changes one thing and holds
the rest, so a difference in its results has one cause.

**The baseline** is the state that `just live-up` builds. `baseline.sh` checks
it:

- an OpenTofu plan with the default variables changes nothing
- cluster A (`fred`) is `Running`, and runs the current content of the release
  and add-on profiles
- no cluster B (`barney`) exists
- each profile name holds exactly the versions that OpenTofu manages

Each script needs the baseline before it starts, and leaves the example as it
found it.

| Script | Changes | Holds |
| --- | --- | --- |
| `verify.sh` | nothing; it only reads | the baseline |
| `in-place.sh off` and `on` | the `immutable-clusterprofiles` flag, in each plan and apply | the edit (`api.sh edit` in both runs), the clusters, the steps and their order, the baseline |
| `registry-move.sh` | the CSI registry of the move profile, then its values | the flag (on), the other packs, the baseline |
| `drift-report.sh off` and `on` | the flag, in the plan and in the bump | the edit (`api.sh edit` in both runs), the records, the steps and their order, the issue label, the baseline |
| `tag-only.sh` | the tag name of the OS pack, and which of two such versions cluster A runs | the pack uid, each other pack, the values, the flag (off), the baseline |
| `packs.sh` | nothing; it only reads | the baseline |
| `merge-gate.sh` | the add-on version, which stage 4 edits in place and stage 5 puts back | the profiles, the clusters, the baseline |
| `knurl.sh` | nothing; it only reads | the baseline |

**One definition for each kind.** Clusters A and B come from one resource, and
differ only in their names. Each profile has the same lifecycle:
`skip_destroy` follows the flag, and `create_before_destroy` is on.

**What OpenTofu does is a check, not a step.** The edit in place goes through the
API in both `in-place.sh` runs, because the provider refuses it with the flag
on. The check `plan-edit` records what the provider plans for the same edit.

**`skip_destroy` before the first apply.** A flag-on version bump replaces the
profile resource. The provider deletes the replaced version with the
`skip_destroy` value in its state, not the new value. So each script that
applies first stores `skip_destroy` for its flag, with an apply at the baseline
that changes nothing else (`hold_versions` in `lib.sh`). Its last apply sets it
back to false with the default variables (`apply_baseline`).

## Prerequisites

- A Palette tenant with a project, and an AWS cloud account in it.
- An EC2 key pair in the region. The provider requires one.
- A Palette API key in `PALETTE_API_KEY` or `~/.palette.env`.
- `tofu` 1.6 or later (`just tofu-install` installs 1.12.6), `jq`, `just`,
  `curl`, `helm`, `crane`, and a Rust toolchain.
- Read access to an OCI pack registry in the tenant, and a login to it that
  `crane` can use (`crane auth login` or `docker login`). The Helm registry must
  serve charts with no credentials. `Bitnami` does. For an ECR registry:

  ```bash
  AWS_PROFILE=<profile> aws ecr get-login-password --region <region> \
    | crane auth login <account>.dkr.ecr.<region>.amazonaws.com -u AWS --password-stdin
  ```

  The profile needs `ecr:GetAuthorizationToken`, and the token expires, so a run
  after some hours needs the login again. `crane auth login` reports success
  even when it reads an empty password, so make sure of the login with `crane
  digest <registry>/<repository>:<tag>`.
- The AWS CLI with read access to the account, only for `aws-inventory.sh`.
- An AWS profile that can create an ECR repository and an IAM user in the
  account, named in `terraform.tfvars` as `aws_profile`. Every apply needs it,
  because the example's own pack registry lives there. `registry.sh` needs
  `oras` as well, and logs in to the repository with that profile.
- The AWS managed policy `AmazonEBSCSIDriverPolicy` on the worker node role
  `nodes.cluster-api-provider-aws.sigs.k8s.io`. Without it, the EBS CSI
  controller fails on each cluster with `UnauthorizedOperation` on
  `ec2:DescribeAvailabilityZones`. No strata check needs volumes, and Palette
  still reports such a cluster `Running`. The runs before 2026-09-17 01:35 UTC
  ran without the policy.

For a self-hosted Palette, set `PALETTE_API` in the environment or in
`~/.palette.env`. Strata reads it, and `tf.sh` and `tenant.sh` pass it on.

## Steps

### 1. Find the names in your tenant

```text
$ examples/live/tenant.sh names us-east-2
projects (palette_project):
  platform  uid 64f20000000000000000a0f1
  strata-test  uid 64f20000000000000000a0d1
AWS cloud accounts (cloud_account_name), with zones (az) and key pairs (ssh_key_name) in us-east-2:
  aws-main
    zones:     us-east-2a, us-east-2b, us-east-2c
    key pairs: platform-key, bastion-key
Helm registries (helm_registry_name):
  Bitnami
  Public Spectro Helm Repo
OCI pack registries (oci_registry_name):
  Palette Registry  <spectro account>.dkr.ecr.us-east-1.amazonaws.com
  Palette Community Registry  <spectro account>.dkr.ecr.us-east-1.amazonaws.com
  team-packs  <your account>.dkr.ecr.us-east-2.amazonaws.com
```

The names and the uids above are examples. This book holds no name or uid of a
real tenant.

Put the names in `terraform.tfvars`:

```bash
cp examples/live/terraform/terraform.tfvars.example examples/live/terraform/terraform.tfvars
$EDITOR examples/live/terraform/terraform.tfvars
```

| Variable | Value |
| --- | --- |
| `palette_project` | a project name |
| `cloud_account_name` | an AWS cloud account name |
| `region`, `az` | the region and a zone |
| `ssh_key_name` | a key pair |
| `oci_registry_name` | an OCI pack registry that you can log in to |
| `oci_pack`, `oci_pack_version` | an add-on pack in that registry, and its version |
| `helm_registry_name`, `helm_chart`, `helm_chart_version` | optional; the default is `nginx` 25.1.5 from `Bitnami` |

The file is not tracked. It holds names only. The API key never goes in it.

### 2. Build it

```bash
just tofu-install     # once
just live-up          # billable until live-down
```

`tf.sh` gives the key to OpenTofu from the environment or `~/.palette.env`, and
never prints it.

For the profiles alone, with no cluster and no cost, `just live-profiles` runs
the apply with `-var cluster=false`. It gives a project that `list`, `compare`,
`candidate`, `consumers`, and `verify` read in seconds. The cluster checks need
`just live-up`. `just live-down` removes either.

### 3. Run the checks

Log in to the OCI pack registry first, so `crane` can read the digest of the
pack. For a registry in AWS:

```bash
aws ecr get-login-password --region <region> | crane auth login -u AWS --password-stdin <registry endpoint>
just live-verify
```

`verify.sh` reads the uids from the OpenTofu outputs, runs each check, and
compares each exit code with the expected one. It writes each output, the raw API
responses, and `summary.md` to `examples/live/runs/<UTC time>/`. That directory
is not tracked, because raw responses can hold variable values. The script exits
`0` only when each check passes.

### 4. Edit, move, and rename

The scripts write to the tenant, so run them one at a time. Each starts from the
baseline and restores it, so they run in any order:

```bash
examples/live/in-place.sh off                                                  # #27, legacy provider behavior
OFF_RUN=examples/live/runs/<time>/in-place/off examples/live/in-place.sh on    # #27, with the flag
examples/live/registry-move.sh                                                 # #24
AWS_PROFILE=<profile> examples/live/tag-only.sh                                # #19
just live-drift-report                                                         # #25
just live-packs                                                                # #55, read-only
just live-merge-gate                                                           # #57, needs just live-profiles
just live-registry                                                             # #66, needs just live-profiles, oras, helm, and aws
just live-template                                                             # #66, needs just live-profiles and no cluster; ends with live-down
just live-cluster                                                              # #66, needs just live-up; ends with the cluster gone
```

`just live-session` runs `verify.sh`, `in-place.sh off`, `in-place.sh on`, and
`registry-move.sh` in that order, under one `runs/<UTC time>/` directory, and
stops at the first failure.

- `in-place.sh` creates cluster `barney`: about 20 minutes. It deletes it at the
  end: 10 to 30 minutes. It waits for `Running` before the delete, because
  Palette does not complete the deletion of a cluster that is still
  provisioning.
- `registry-move.sh` runs no cluster, and takes a few minutes.
- Each script writes `summary.md` and each output under its run directory. To
  continue an interrupted `in-place.sh`:
  `OUT=<that directory> examples/live/in-place.sh <off|on> <stage>`.

### 5. Remove it

Keep the uids first. The outputs go with the state.

```bash
C=$(examples/live/tf.sh output -raw cluster_uid)
export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
just live-down
examples/live/tenant.sh cluster "$C"                  # exit 0: Deleted
AWS_PROFILE=<profile> examples/live/aws-inventory.sh   # exit 0: nothing left
```

`live-down` also deletes each version of each profile name that the example
uses (the `profile_names` output), by exact name. With the flag on,
`skip_destroy` keeps the versions that a bump replaced out of the state, so a
destroy alone leaves them in Palette.

The destroy asks for approval. A run with no person at the terminal answers it
with `yes yes | just live-down`; the second attempt, after a refused read,
reads the answer too.

> **Do not take "Destroy complete" as the confirmation.** During a gateway
> outage, Palette answers a plain-text 404, and the provider can take it for a
> finished deletion while the cluster still runs. A deleted cluster has the
> state `Deleted`. `tenant.sh cluster` exits `2` on a 404 that is not the JSON
> `ResourceNotFound` of Palette.

## When a check fails

`verify.sh` continues after a failed check, so one run shows each result. In
`runs/<UTC time>/`:

- `summary.md` has the table of results and the digest table.
- `<check>.out` and `<check>.err` hold the output of each command. A failed
  comparison names the layer and the value in its `.out`.
- `raw/cluster`, `raw/addon`, `raw/release`, `raw/next`, and `raw/registries`
  hold the API responses that strata hashed. Compare them with the models of the
  Palette SDK when a shape looks wrong.

Run `just live-verify` again after a fix. Each run gets its own directory. The
cluster does not need to be `Running`: the checks read the record of the cluster
in Palette, which exists from the start of the creation.

| Symptom | Cause |
| --- | --- |
| `live-up` fails with "A minimum allocation of '4.1' CPU cores is required across all worker nodes" | The workers have 4 cores or fewer in total. The default of two `t3.xlarge` workers has more. |
| `snapshot-candidate` fails with `pack-not-found` that names a tag | A strata build from before the tag fix. The tag of the profile is the version. |
| A flag-on bump fails with `DeletionResourceInUseError: Unable to delete the resource as fred spectrocluster(s) in-use` | The state held `skip_destroy = false`, and the provider deletes a replaced version with the value in its state (provider 0.29.7, and `resourceClusterProfileDelete` in 0.30.1). Each script now runs `hold_versions` right after it checks the baseline. |
| A script stops with `target/release/strata is older than <file>; run just build` | The sources changed after the last release build. A script that you run directly does not build first. Without this check, the `drift-report.sh off` run of 2026-09-17 used a binary with no `verify --summary`, and failed 18 checks. Run `just build`, then run the script again. |
| `consumer-is-the-cluster` fails with state `unknown` | The key cannot read the cluster in its project. `consumers-release.out` prints the error next to the cluster. |
| `only-the-edited-value` fails with two changed values | Somebody edited 1.0.1 in place and did not undo it. `baseline.sh` names it. |
| A script stops with "not at its baseline" | An earlier run did not finish, or a person changed the example. `baseline-before.txt` names each difference. `just live-down && just live-up` builds it again. |
| `registry-digest-oci` fails with "crane digest ... failed" or HTTP 401 | No login to the OCI registry, or an expired one. An AWS registry login lasts 12 hours. A login also stores an empty credential, and reports a success, when the token request before it failed: run `aws ecr get-login-password` alone first, and use a profile that has `ecr:GetAuthorizationToken`. |
| `ebs-csi-controller` pods in `CrashLoopBackOff`, with `UnauthorizedOperation` on `ec2:DescribeAvailabilityZones` | The worker node role does not have `AmazonEBSCSIDriverPolicy`. See Prerequisites. |
| A cluster stays `Provisioning` with `not done: WorkerNodeAdditionDone: provision worker nodes`, although its machines are `Running` and its nodes `Ready` | Palette did not check the workers again after a node that was not yet in the cluster failed its first check (`fred`, 2026-09-17). A restart of the Palette controller in the cluster did not help. Its deletion ran normally: delete it, and send its diagnostics to Palette support. |

## When a teardown is slow or stuck

These rules come from the teardowns of this kind of cluster:

- **Save what holds the cluster first.** Run `examples/live/diagnose.sh` (with
  `AWS_PROFILE` for the AWS inventory), or `tenant.sh diagnose <uid> <dir>` for
  one cluster. When the cluster is gone, the evidence is gone.
- **Do not delete a cluster that is still provisioning**, unless its machines are
  `Running` and its nodes `Ready`. Palette then does not complete the deletion,
  and each cloud resource stays.
- **A deletion continues in Palette after OpenTofu stops.** Check with
  `tenant.sh cluster`, and do not start again. While the state is not `Running`
  or `Deleted`, it prints the conditions that are not done and the three latest
  events.
- **Judge the progress by `aws-inventory.sh`, not by the time.** Only 0 on each
  line means that AWS is empty. The instances, the NAT gateway, and the load
  balancer go first. The security groups, route tables, subnets, internet
  gateway, and VPC go last, some after a retry. Before 2026-09-17,
  `aws-inventory.sh` counted fewer kinds of resource, so a healthy deletion
  looked stuck. Palette then logs the event
  `Cluster <name> has been deleted`, but the state stays `Deleting` for several
  minutes, and OpenTofu waits for the state. A deletion seen end to end took
  19 minutes: the VPC went first, the event came 11 minutes after the delete
  with the inventory at 0, and the state became `Deleted` 8 minutes after that.
  No step was manual.
- **If the state stays `Deleting` and the inventory does not change** for
  several readings, read the events of the cluster in the Palette console. The
  message usually names what holds the deletion. A cluster with Pause Agent
  Upgrades on can stay in `Deleting`: disable that option and delete again.
- **Force a delete only when `aws-inventory.sh` exits 0.** A forced delete
  removes the records of Palette and nothing in AWS. An earlier force leaves
  resources that cost money with nothing to track them. Palette refuses a forced
  delete for about 15 minutes after the deletion starts, and can answer 500
  before it accepts one. A forced delete in that gap does no harm once the
  inventory is empty, and it saves a few minutes at most. Use the Palette
  console, or the API:

  ```bash
  set -a; . ~/.palette.env; set +a     # when the key is in that file
  curl -X DELETE -H "ApiKey: $PALETTE_API_KEY" -H "ProjectUid: $PALETTE_PROJECT_UID" \
    "${PALETTE_API:-https://api.spectrocloud.com}/v1/spectroclusters/$C?forceDelete=true"
  ```

## The checks of verify.sh

| Check | Expected exit | What it shows |
| --- | --- | --- |
| `snapshot-cluster` | 0 | strata reads what a live cluster runs |
| `cluster-runs-release` | 0 | the cluster reports the release profile version |
| `cluster-runs-two-profiles` | 0 | #23: the cluster runs the release profile and the add-on profile. Palette lists the two in either order. |
| `snapshot-release` | 0 | record the version that the cluster runs |
| `compare-release-cluster` | 0 | #1: a cluster that runs the recorded version is `IDENTICAL` |
| `snapshot-addon` | 0 | record the add-on profile version that the cluster runs |
| `compare-addon-cluster` | 0 | #23: the add-on record is `IDENTICAL` to the add-on selection of the cluster |
| `addon-has-a-variable` | 0 | #66: the add-on profile declares one variable, `purpose` |
| `cluster-variables-endpoint` | 0 | #66: `GET /v1/spectroclusters/<uid>/variables` serves the variable of the add-on profile |
| `snapshot-addon-copy` | 0 | #66: record the copy of the add-on profile that the cluster holds |
| `addon-copy-variables-hash` | 0 | #66: the copy has the variables hash of the profile, so the endpoint repeats the definition fields |
| `snapshot-next` | 0 | record the newer version |
| `compare-next-cluster` | 1 | #1: against a newer record, the cluster shows the change |
| `only-the-edited-value` | 0 | #1: the only change is `csi-aws-ebs` `reclaimPolicy` |
| `consumers-release` | 1 | #12: the release version has a consumer |
| `consumer-is-the-cluster` | 0 | #12: listed with its name, project, and state |
| `consumers-next` | 0 | #12: no cluster runs the newer version |
| `candidate-file` | 0 | #21: write the record of the newer version as a candidate file |
| `snapshot-candidate` | 0 | #4: snapshot the candidate through registry lookups |
| `candidate-same-release` | 0 | #4: the root of the candidate equals the published root |
| `verify-release` | 0 | `verify` passes on a published version that nobody changed |
| `verify-cluster-record` | 0 | `verify` passes on a cluster record |
| `snapshot-registries` | 0 | record the profile with a Helm pack and an OCI pack |
| `registries-kinds` | 0 | #2: the profile holds one helm pack and one oci pack |
| `registry-digest-helm` | 0 | #2: read the digest of the helm pack from the registry itself |
| `registry-digest-oci` | 0 | #2: read the digest of the oci pack from the registry itself |

`strata candidate next.json -o candidate/profile.yaml --files-dir
candidate/files` writes the candidate file from a record. Its tags matter:
Palette stores the tag that a profile was authored with, and the provider pins
the version, `24.04`, where the registry names that Ubuntu version `LTS__24.x`.
