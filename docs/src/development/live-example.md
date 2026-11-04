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
| `pack-states.sh` | Pack versions that Spectro Cloud deprecated, disabled, or deleted: what `packs` and `snapshot` report, and which profiles Palette refuses (#68). It needs no cluster. |
| `validate-packs.sh` | What Palette answers when `snapshot --validate` asks whether it takes the packs of a candidate, and of a version (#152). Stage 4: the layer that Palette gives a pack whose profile gives another layer than its registry (#210, #254). Stage 5: the name, the version, and the names of the packs and the manifests of a candidate (#273). It needs no cluster and no profile. |
| `validate-route.sh` | Whether the validate route of a cluster refuses the fault that a rule of the preflight catalog copies (#243). It asks the route with one fault of a body for each case. It needs the key and a body of a create for each engine, and no cluster. |
| `route-bodies.sh` | The case bodies of `validate-route.sh` that depend on a profile, on AWS (#408). It writes `aws-clean.json` and the bodies for `profile-cloud-mismatch` and `pack-cloud-support`, and it skips `pack-state-disabled` and `pack-state-deleted`, because Palette refuses their profiles. A body names its profile, and Palette answers 404 for a profile that is gone. So the script runs `validate-route.sh` on the bodies itself, while the profiles are there (#413). Then it deletes each profile and each case body, also when a step fails. It needs the release profile on AWS and the key, and no cluster. |
| `export.sh` | The export of each version of the example, read with `snapshot --from-candidate`, and what the export route answers (#156). It needs the profiles, and no cluster. |
| `registry.sh` | The example's own OCI pack registry: a push, a push again of one version with other bytes, and a new version, and what Palette does after each (#66); a tag that the registry deletes while a profile pins it (#108). It needs the profiles, and no cluster. |
| `template.sh` | The lock of a cluster template on the versions it links, the repave answer for a cluster in the template (#276), and the order in which a cluster installs the packs of two add-on profiles (#66). It needs the profiles and no cluster, and it ends with the removal of the example. |
| `cluster.sh` | A variable value that a cluster changes, and other answers that only a cluster gives (#66). It needs cluster A, and it ends with the cluster gone. |
| `order.sh` | What a reorder of the packs of a published version is to Palette (#66). It needs cluster A, and it leaves the versions edited: run `just live-down` after it. |
| `who.sh` | What `verify --who` reads from the audit log, for an edit through the API and for an update that a cluster accepts (#69). Stage 1 needs the profiles; stage 2 needs cluster A. Stage 3 reads with a key that cannot read the audit log (#664), and needs `STRATA_LIVE_NO_AUDIT_KEY`. |
| `draft.sh` | A draft of the add-on version: a `PUT` with no publish, what `<uid>@draft` records, what each route of Palette gives while the draft waits, and the publish (#155). It needs the profiles, and no cluster. |
| `scope.sh` | A profile of the tenant with the name and the version of the release profile: what the listing, the search, and the document of Palette say, and what `list` and `snapshot` do with the ref that two scopes answer (#113). It needs the profiles, and no cluster. |
| `release.sh` | The route that gives the release of Palette: it answers a call with no key, and refuses a call with the key (#146). It needs the key and nothing else. |
| `pending.sh` | When Palette says that an update waits for a cluster, what a record of `cluster:<uid>@pending` holds (#70, #78), and what `compare --repave` with it asks Palette (#276). It needs cluster A. |
| `macros.sh` | What a new value of a project macro does to a version, to the copy of a cluster, and to a record (#72). It needs cluster A. |
| `image-digest.sh` | What cluster A pulls when a digest sits in the image list of a pack alone, and when it sits in the manifest that the workload reads (#630). It needs cluster A. |
| `macro-repave.sh` | Whether a new macro value in an infrastructure layer replaces the nodes of a cluster (#72). It needs cluster A, and it leaves a repave that waits for approval: run `just live-down` after it. |
| `repave.sh` | `compare --repave`: the answer of Palette beside the level of the impact table (#73), and `compare --validate` (#134). Stage 8 asks about a profile variable in a repave key of a version that the cluster does not run (#132). It needs cluster A. |
| `registry-move-nodes.sh` | Whether a move of the OS pack to another registry, at the same version, replaces the nodes of a cluster, and what the impact table and Palette say (#135). It needs cluster A, and it can repave it two times. |
| `merge-gate.sh` | `scripts/ci/merge-gate.sh` over records of the live profiles, in a git repository that the script builds (#57). It needs the profiles, and no cluster. |
| `knurl.sh` | [The knurl adapter](../automation/knurl.md): each method of the port against the strata answer for the same real object. Read-only. It needs the profiles; the instance checks need the cluster. |
| `registry-kinds.sh` | The digest of each registry kind, and a push again of one version into each (#75); a Helm registry with synchronization off, the case with no digest (#71). It needs no cluster. |
| `tiers.sh` | The tiers of a cluster record, and `compare` of two cluster records beyond the release (#76). It builds cluster B for a while. Stage 5 edits a role binding of cluster A, and needs no cluster B (#106). Stage 6 reads the role bindings with a second key, and runs only as `tiers.sh 6` (#276). Stage 7 sets an override on a machine pool of cluster A, and runs only as `tiers.sh 7` (#515). |
| `edge-host.sh` | A record of an edge host, `consumers` of a profile bound to one, and what an edit in place does to an edge host (#78). It registers edge hosts with no device. |
| `cluster-group.sh` | A record of a cluster group, and `compare` of the group with its virtual cluster (#78); a change of the CPU limit of the group (#110). It builds a virtual cluster in cluster A. |
| `app-profile.sh` | A record of an app profile, and a plain update of it (#78); two app profiles whose tiers differ in the install order (#112). It needs no cluster. |
| `fleet.sh` | `list clusters --tag` and `--filter`, `list filters`, and a record of each cluster of a set (#80). It labels cluster A through the API. |
| `upstream.sh` | `strata behind` against the registries, and `upstream-report.sh` around a version that arrives in the example's own registry (#79). It needs no cluster. |
| `loop.sh` | [One change, start to end](../walkthrough.md), end to end: a change, the gates, the publish, the questions to Palette about a move, `preflight` on the body of a create, the nightly jobs, a value overridden on a cluster, an edit in place, an incident, and a retire, with the records in a git repository that it builds (#81, #385). It needs cluster A, and undoes each change. |
| `observer-kind.sh` | [The observer](./observer-deploy.md) on a kind cluster: its manifests, the records that `observer-records.sh` writes, the pod, and the page for a user with only the reader role (#644). It reads cluster A and the two profiles of the release from the tenant, and writes nothing to it. Stage 7 (#646) starts a registry and a mirror for `vendor.example`, and applies the component that reads the pods. It needs docker, kind, and kubectl, and it deletes the kind cluster and the registry. |
| `observer.sh` | [The observer](./observer-deploy.md) on cluster A, in two ways: a plain `kubectl apply` of the rendered manifests, and an add-on profile in Palette with the manifests as one manifest layer (#656). It checks the pod, the answer of Palette to a workload cluster, who can open the page, an edit in place and its finding, the requests of the cycles, a wrong key, a restart, and the removal. It needs cluster A, `kubectl`, `gh`, and `go`. It makes a namespace and a profile, and it removes both. |
| `observer-images.sh` | The digest that each pod of cluster A runs (#646): the observer with the component that reads the pods, and a throwaway tag on `ttl.sh` that moves from one image to another. It checks the first digest, a split, a move, a container that never started, a crash loop, and the agents of Palette. It needs cluster A, docker, kubectl, and crane. It edits the values of an add-on in place, and the undo restores them. |
| `observer-role.sh` | The least role of the key of the observer (#645): each request of a content cycle and of a versions cycle, with the key of the example and with a second key, and the commands of the cycles. It only reads. It needs cluster A and a second key: a maintainer makes the user. |
| `eks-nodegroups.sh` | The EKS node groups of cluster A, from AWS; `--delete` deletes those of the example, a step of the recovery of [An EKS cluster](#an-eks-cluster). |
| `eks.sh` | On an EKS cluster: the release and its packs, the shape tier of a managed node group, the repave answer of Palette for each layer and for a Kubernetes bump, and a change of the node group (#89). It needs `cloud = "eks"` and cluster A. |
| `gke.sh` | On a GKE cluster: the release and its packs, the shape tier of a node pool, the repave answer of Palette for a Kubernetes bump, and, with `GKE_POOL_CHANGE=1`, a change of the node pool (#90). It needs `cloud = "gke"` and cluster A. |
| `version-states.sh` | One state per version with no feature preview flag, a destroy that a cluster in use refuses, a version that is gone, and the version strings that Palette stores (#81). It builds its own profile in `terraform-version/`, and needs cluster A. |

The example runs on AWS or GCP instances, or on EKS or GKE: the variable `cloud` in
`terraform.tfvars` selects one. Strata makes no rule about which cloud runs the
cluster. The profiles and the checks are the same on each cloud.
`terraform/cloud-aws.tf`, `terraform/cloud-gcp.tf`, `terraform/cloud-eks.tf`, and
`terraform/cloud-gke.tf` hold what changes: the cloud account, the cluster resource, and the names and
versions of the infrastructure packs. For another cloud, add a file of the same
shape. EKS and GKE are managed engines. EKS has its own cloud account: see
[An EKS cluster](#an-eks-cluster) and [A GKE cluster](#a-gke-cluster). The scripts read the pack names from the outputs
`os_pack_name` and `csi_pack_name`, and hold none.

## What it builds

`terraform/` creates these objects in one Palette project:

| Resource | What it is |
| --- | --- |
| profile `strata-live-profile` 1.0.0 | Ubuntu 24.04, Kubernetes 1.36.2, Cilium 1.20.1, EBS CSI 1.65.0, default values. Cluster A runs it. |
| profile `strata-live-profile` 1.0.1 | The same, with the storage class `reclaimPolicy` `Retain` in place of `Delete` in the CSI pack. The GKE CSI pack has values of comments alone, so with `cloud = "gke"` 1.0.1 adds the key `strataNext` in its place. The output `csi_next_key` names the key. No cluster runs it. |
| profile `strata-live-addon` 1.0.0 | An add-on profile with one manifest, the ConfigMap `strata-live-config` in `default`, and one variable, `purpose`, with a default. Cluster A runs it after the release profile. |
| cluster `fred` (cluster A) | One control plane node and two workers, `t3.xlarge`, one availability zone. With `cloud = "gke"`, a GKE cluster with one node pool of three `e2-standard-4` nodes, on the GCP cloud account and project of `cloud = "gcp"`. With `cloud = "eks"`, an EKS cluster with one managed node group of two `t3.xlarge` nodes in two zones, on the cloud account of [An EKS cluster](#an-eks-cluster). |
| profile `strata-live-registries` 1.0.0 | An add-on profile with one chart from a Helm registry (default `nginx` 25.1.5 from `Bitnami`), and one pack from the OCI pack registry that `terraform.tfvars` names. No cluster runs it. |
| registry `strata-live-registry` | The example's own OCI pack registry: two ECR repositories, `strata-live/spectro-packs/archive/strata-live-pack` and `.../strata-live-dep`, in the AWS account that `aws_profile` names, a user that reads them, and a Palette registry over them. `registry.sh` pushes into it; nothing else does. |
| profile `strata-live-owned` 1.0.0 | An add-on profile with the one pack of that registry. `registry.sh` makes it after the first push, and removes it at its end. |
| profile `strata-live-dep` 1.0.0 | An add-on profile with `strata-live-pack` 1.1.0, which requires `strata-live-dep`, and that pack. `registry.sh` makes it after the pushes, and removes it at its end. |
| registry `strata-live-helm-registry` | The same repository host, read by Palette as an OCI Helm registry, with one chart repository. `registry.sh` pushes into it. |
| profile `strata-live-owned-helm` 1.0.0 | An add-on profile with the one chart of that registry. `registry.sh` makes it after the first push, and removes it at its end. |
| policy `strata-live-maintenance`, template `strata-live-template` | A maintenance policy, and a cluster template that links the release version and the add-on version. `template.sh` makes them before it builds a cluster, and removes them at its end. The variable `cluster_in_template` builds the clusters in the template, on AWS and GCP. A cluster in the template needs a time zone, and the variable `cluster_timezone` gives it. |
| profile `strata-live-addon2` 1.0.0 | A second add-on profile with one manifest. `template.sh` runs it on the cluster beside the first, for the install order of add-on packs. |

The pack versions are those of `examples/profiles/aws-stack@1.1.0.json`.

`cluster-group.sh` sets `cluster_group`, which adds a cluster group
`strata-live-group` on cluster A that pins the add-on version, and a virtual
cluster `strata-live-member` in it. The virtual cluster runs in cluster A, so it
adds no machine. The script removes both at its end.

`in-place.sh` adds cluster `barney` (cluster B), with the same two profiles, for
the length of its run. The two cluster names share no part, so no match by
prefix or substring finds both. `registry-move.sh` adds the profile
`strata-live-move` 1.0.0 and 1.0.1 with the release packs, and deletes both at
its end.

> **The cluster costs money while it exists.** In the recorded AWS run, it had four
> EC2 instances (three `t3.xlarge` nodes and a `t3.micro`), a NAT gateway, an
> elastic IP, a classic load balancer, a VPC, and four volumes. Creation took 15
> to 19 minutes. A deletion took 9 to 29 minutes. Remove it when the checks are
> done, and make sure that it is gone.

## Files

| File | Function |
| --- | --- |
| `terraform/` | The profiles and the clusters, and the variables that the scripts set. `main.tf` holds the profiles; `cloud-aws.tf`, `cloud-gcp.tf`, and `cloud-eks.tf` hold the cloud account, the clusters, and the infrastructure packs of each cloud. `iam/eks-controllers.json` is the EKS policy of Palette, for the role of `cloud-eks.tf`. |
| `tofu-install.sh` | Installs the pinned OpenTofu. `just tofu-install` runs it, and `just tofu-uninstall` removes it. |
| `tf.sh` | Runs `tofu` with the key and the Palette address that strata uses. |
| `contract-profiles.sh` | The standing cluster profiles of the contract run (#531). It applies `terraform/` with the prefix `strata-contract`, no cluster, and `own_registries` off, in the state file `terraform/contract.tfstate`. See [The profiles of the run](./facts.md#the-profiles-of-the-run). |
| `contract.sh` | The contract run (#390). It asks a real tenant the requests of the facts of Palette, and compares each answer with the stand-in. It sends GET requests and the read-only calls of the clouds, and builds nothing. `just live-contract` runs it, and the weekly timer runs it too. See [The weekly contract run](./facts.md#the-weekly-contract-run). |
| `contract-report.sh` | The issues of a contract run (#390): one for each fact that differs, one for each unknown value, and one for each new release. `just contract-report` runs it. It needs a `gh` login, and `--dry-run` calls no host. See [The issues](./facts.md#the-issues). |
| `report-send.sh` | The send of the report (#397). Strata posts the report of one unknown value to the endpoint, and the script checks that one issue of the label `unknown-value` holds it. `just live-report-send` runs it. It reads no tenant. See [The endpoint of the report](./report-endpoint.md). |
| `common.sh` | The functions `die` and `usage`. `lib.sh`, `contract.sh`, `contract-profiles.sh`, `api.sh`, `tenant.sh`, and `tofu-install.sh` source it. So do `timer.sh` and `deploy.sh` of `deploy/`. It runs nothing and checks nothing, so a script that does not run strata can source it alone. |
| `github.sh` | What the scripts read about GitHub. `remote_repo` gives the project of the `origin` remote of a checkout. The jq function `first_line` gives the first line of the body of an issue, which holds its marker. `contract-report.sh`, `upstream.sh`, `drift-report.sh`, and `report-send.sh` source it. The scripts in `scripts/ci/` stand alone, so they do not. |
| `lib.sh` | What the scripts share: the heading of a stage, the checks and their summary, and the edits of the lab. It also holds the reads of `terraform.tfvars`, the applies, the wait for a cluster, and the baseline. For the observer, it holds the key, the address of Palette, the ConfigMap, and the test of a pod that cannot connect. It stops on a binary of strata that is older than the sources. With a `jq` older than 1.7, it makes `jq -e` exit with `4` on an empty input. So a check on the output of a failed command fails. |
| `baseline.sh` | Checks that the example is at its baseline. Each script needs it before it starts and after it ends. |
| `tenant.sh` | Read-only lookups: the names for `terraform.tfvars`, the state of a cluster and what holds it, the cluster listing itself, diagnostics, the repave check of Palette, and a pack digest from its Helm or OCI registry. |
| `diagnose.sh` | Saves diagnostics for each cluster in the state. The scripts and `just live-up` and `just live-down` run it when an apply fails or a wait stops. |
| `verify.sh` | Runs each read-only check, and writes `runs/<UTC time>/`. |
| `in-place.sh` | The edit in place test (#27). |
| `registry-move.sh` | The registry move test (#24). |
| `tag-only.sh` | The tag name test (#19). |
| `drift-report.sh` | The drift report test (#25). It opens and closes issues in this repository with the label `strata-live-drift`. |
| `count-requests/` | A small Go program that sits between `strata` and Palette. It runs a command, sends each request on, and writes one line for each request to a file, with the method and the path. `observer.sh` uses it to count the requests of a cycle of the observer. `just lint` and `just test` check it. |
| `api.sh` | The writes outside OpenTofu: an edit in place, a push of a profile to a cluster, the delete of profile versions, the delete of a cluster, a probe profile that it deletes at once, an edge host with no device, an app profile, and an add-on profile with one manifest layer (`api.sh manifest-profile`). `api.sh post` sends a search that Palette takes as a `POST`, or a question to a validate route. `api.sh export` reads the export of a profile version. `api.sh packs-where` reads the pack listing one page at a time, and prints the first pack that a `jq` condition accepts. `api.sh get-status` prints the status of an answer and its body, and exits `0` for a refusal. |
| `packs.sh` | The pack ref test (#55). |
| `pack-states.sh` | The pack state test (#68). |
| `validate-packs.sh` | The profile validate test (#152). |
| `validate-route.sh` | The cluster validate route test (#243). |
| `route-bodies.sh` | The route bodies test (#408, #413). |
| `export.sh` | The profile export test (#156). |
| `merge-gate.sh` | The merge gate test (#57). |
| `knurl.sh` | The knurl adapter test. |
| `registry.sh` | The registry test (#66): pushes into the example's own OCI pack registry and OCI Helm registry, and reads what Palette does with each push. |
| `pack/`, `chart/` | The two packs and the one chart that `registry.sh` pushes. |
| `registry-kinds.sh` | The registry kinds test (#75). |
| `zarf/` | The Zarf package that `registry-kinds.sh` publishes. |
| `terraform/registry-kinds.tf`, `terraform/registry-host.sh.tftpl` | The registry host of `registry-kinds.sh`: one EC2 instance that serves a Helm repository, one with credentials, an OCI registry with basic auth, and a Spectro pack registry, behind Caddy with a certificate for each `<service>.<IP>.sslip.io`; and a Palette registry over each. Off unless `registry_kinds` is set. |
| `terraform-version/` | One version of an add-on profile in a state of its own, with no feature preview flag: `version-states.sh` applies it once for each version, each time in its own workspace. |
| `template.sh` | The cluster template test (#66): the lock on a linked version, a cluster outside the template, a cluster in the template with the two repave routes (#276), and the install order of add-on packs. It ends with the removal of the example. |
| `cluster.sh` | The cluster test (#66): a change of the value the cluster gives a variable, and a deletion through the API. It ends with the cluster gone. |
| `order.sh` | The order test (#66): a reorder of the packs of two published versions, and of the profiles of the cluster, through the API. It leaves the versions edited; `live-down` follows it. |
| `who.sh` | The audit log test (#69): who made an edit through the API, and an update that a cluster accepted, as `verify --who` reads them. |
| `draft.sh` | The draft test (#155): a draft of the add-on version, `<uid>@draft`, and the publish. |
| `pending.sh` | The pending update test (#70, #78): when Palette says that an update waits for cluster A, what a record, the listing, and `compare` say, and what `cluster:<uid>@pending` holds; `consumers --list-only`. |
| `macros.sh` | The macro test (#72): what a project macro does to a version, to the copy that cluster A runs, and to a record, when the macro gets another value. |
| `image-digest.sh` | The image digest test (#630): a digest in the image list of a pack alone, and a digest in the manifest that the workload reads, and the image that cluster A runs for each. |
| `macro-repave.sh` | The macro repave test (#72): a macro in a field that repaves cluster A, and what a change of its value alone does. It ends with a repave that waits for approval; `live-down` follows it. |
| `repave.sh` | The repave test (#73): `compare --repave` asks Palette if a change repaves cluster A, and prints its answer beside the impact level. `compare --validate` asks Palette if it accepts the packs (#134). |
| `registry-move-nodes.sh` | The registry move of a running cluster (#135): the OS pack of the release version from `move_registry_name`, and what cluster A does when it takes it. |
| `edge-host.sh` | The edge host test (#78). |
| `cluster-group.sh`, `terraform/cluster-group.tf` | The cluster group test (#78), and the group and the virtual cluster that it builds. Off unless `cluster_group` is set. |
| `app-profile.sh` | The app profile test (#78). |
| `fleet.sh` | The fleet test (#80). |
| `upstream.sh` | The upstream test (#79). It opens and closes issues of this repository with the label `strata-live-behind`. |
| `inventory.sh` | What clusters A and B still have in the cloud that `terraform.tfvars` names, to confirm a teardown. `just live-inventory` runs it. It reads the variables and not the outputs, so it works after `live-down`. |
| `aws-inventory.sh` | What one cluster still has in AWS. `inventory.sh` runs it on AWS. |
| `gcp-inventory.sh` | What one cluster still has in a GCP project. `inventory.sh` runs it on GCP. |

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

`verify.sh`, `in-place.sh`, `registry-move.sh`, `tag-only.sh`, and
`drift-report.sh` check the baseline before they start, and leave the example as
they found it. `packs.sh` and `knurl.sh` read the profiles and change nothing.
`pack-states.sh` asks for three profiles, and deletes the one that Palette
creates at once. `validate-packs.sh` asks validate routes, which change nothing.
Its stage 4 makes one profile, and deletes it.
`export.sh` reads the export of each version, and changes nothing.
`merge-gate.sh` makes one edit in place, and puts it back.
`who.sh` makes one edit in place at a time, and puts each back.
`draft.sh` makes one draft, publishes it, and puts the edit back.
`pending.sh` checks the baseline before it starts, and puts each change back.
`macros.sh` does the same, and deletes the macro it makes.
`image-digest.sh` checks the baseline before it starts, and puts each change back.
`repave.sh` checks the baseline before it starts, puts each change back, and takes no update on cluster A.
`registry-move-nodes.sh` checks the baseline, and puts the release version back, unless a repave waits for approval.
`registry.sh` leaves the pushed versions in its own registry.
`template.sh` ends with the example removed, `cluster.sh` ends with the cluster
gone, and `order.sh` leaves two versions edited: run `just live-down` after
each.

| Script | Changes | Holds |
| --- | --- | --- |
| `verify.sh` | nothing; it only reads | the baseline |
| `in-place.sh off` and `on` | the `immutable-clusterprofiles` flag, in each plan and apply | the edit (`api.sh edit` in both runs), the clusters, the steps and their order, the baseline |
| `registry-move.sh` | the CSI registry of the move profile, then its values | the flag (on), the other packs, the baseline |
| `drift-report.sh off` and `on` | the flag, in the plan and in the bump | the edit (`api.sh edit` in both runs), the records, the steps and their order, the issue label, the baseline |
| `tag-only.sh` | the tag name of the OS pack, and which of two such versions cluster A runs | the pack uid, each other pack, the values, the flag (off), the baseline |
| `packs.sh` | nothing; it only reads | the baseline |
| `pack-states.sh` | nothing that stays: a probe profile, deleted at once | the baseline |
| `validate-packs.sh` | nothing that stays: stage 4 makes one profile, and deletes it | the baseline |
| `validate-route.sh` | nothing; it only asks a validate route | the baseline |
| `route-bodies.sh` | nothing that stays: one add-on profile for each case, deleted after `validate-route.sh` ran on the bodies, and on a failure | the baseline |
| `export.sh` | nothing; it only reads | the baseline |
| `release.sh` | nothing; it only reads | the baseline |
| `merge-gate.sh` | the add-on version, which stage 4 edits in place and stage 5 puts back | the profiles, the clusters, the baseline |
| `knurl.sh` | nothing; it only reads | the baseline |
| `registry.sh` | the images of the example's own registries, a tag that it deletes and pushes again, and the profiles that pin them | the shared registries, the clusters, the release and add-on versions |
| `registry-kinds.sh` | the registry host and a Palette registry over each kind, then pushes of `1.0.0` into each, and one profile of stage 2; it removes each at its end | the profiles, the clusters, the baseline |
| `tiers.sh` | cluster B, then its timezone and its labels through the API; it removes B. Stage 5: a role binding of cluster A, through the API, and back. Stage 6: nothing; it only reads. Stage 7: both overrides of the worker pool of cluster A, through the API, and back | the profiles, cluster A, the baseline |
| `template.sh` | a template over the release and add-on versions; a cluster in the template, then removed; version 1.0.1, in place and put back; then the release version, in place | the pack versions; it ends with the example removed |
| `cluster.sh` | the value cluster A gives one variable; then the cluster, deleted through the API | the profiles |
| `order.sh` | the pack order of two published versions, and the profile order of cluster A | the packs, the values, the manifests; it leaves the versions edited |
| `who.sh` | the add-on version, edited in place and put back; with a cluster, the update that cluster A accepts, and its undo | the profiles, the clusters |
| `draft.sh` | the manifest of the add-on version, in a draft and then published; it puts the edit back | the profiles, the clusters |
| `pending.sh` | a manifest of the add-on version, a value and a kubeadm command of the release version, and the add-on version number, each in place and put back; the updates that cluster A accepts | the clusters, the flag (off), the baseline |
| `macros.sh` | a project macro, and the manifest of the add-on version that reads it; the update that cluster A accepts; all put back | the clusters, the flag (off), the baseline |
| `image-digest.sh` | the values and the manifest of the add-on version, in place and put back; a Deployment of one pod, and the updates that cluster A accepts | the clusters, the flag (off), the baseline |
| `macro-repave.sh` | a project macro in the Kubernetes pack of the release version, and then its value alone; it leaves a repave that waits for approval | the flag (off); it ends with the removal of the example |
| `repave.sh` | the Kubernetes pack of the release version, and a manifest of the add-on version, each in place and put back; a profile variable on version 1.0.1, through OpenTofu, and back; no update is accepted | the clusters, the flag (off), the baseline |
| `registry-move-nodes.sh` | the registry of the OS pack of the release version, in place, and the update that cluster A takes; both put back | the version and the values of each pack, the flag (off) |
| `edge-host.sh` | two edge hosts with no device, each bound to a version; a manifest of the add-on version and a value of the release version, each in place and put back; it deletes the hosts | the profiles, the clusters, the baseline |
| `cluster-group.sh` | a cluster group on cluster A and a virtual cluster in it; a manifest of the add-on version, in place and put back; the CPU limit of the group, and back; it removes the group | the profiles, cluster A, the baseline |
| `app-profile.sh` | an app profile, a plain update of it, and its delete; two app profiles with install orders, and their delete | the baseline |
| `fleet.sh` | a label of cluster A and a saved tag filter, through the API, each put back | the profiles, cluster A, the baseline |
| `upstream.sh` | three versions of the example's own pack, pushed and then removed; issues of this repository | the profiles, the baseline |

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

- A Palette tenant with a project, and an AWS cloud account in it. The example's
  own pack registry is in AWS for each cloud.
- An EC2 key pair in the region. The provider requires one.
- For `cloud = "gcp"`: a GCP cloud account in the tenant, and a GCP project that
  holds the example alone, with the Compute Engine API on. The service account
  of the cloud account needs `compute.admin`, `container.admin`,
  `iam.serviceAccountTokenCreator`, `iam.serviceAccountUser`, and
  `storage.admin` on the project. `tenant.sh names` lists the projects that each
  account can use. `gcp-inventory.sh` needs `gcloud` with read access to the
  project.
- For `cloud = "gcp"`, `roles/compute.admin` on the project for its Compute
  Engine default service account, `<project number>-compute@developer.gserviceaccount.com`.
  The nodes run as that account, and the GCE cloud controller in the cluster
  calls the Compute API with it. An organization that enforces
  `iam.automaticIamGrantsForDefaultServiceAccounts` gives it no role. Without
  one, the cloud controller exits, the node keeps the taint
  `node.cloudprovider.kubernetes.io/uninitialized`, no pod starts, and the
  cluster stays `Provisioning`.
- For `cloud = "gke"`: the GCP cloud account and project of `cloud = "gcp"`,
  with the Kubernetes Engine API on as well
  (`gcloud services enable container.googleapis.com --project <project>`).
  Without it, the cluster does not build. GCP also refuses a new GKE cluster
  that is in no release channel. Palette sets no channel by default (known
  issue PCP-7528), and gives a cluster-level CAPI override for it:
  `cloud-gke.tf` sets the channel `regular` through
  `override_cluster_api_config`. Without it, GCP refuses each `CreateCluster`
  call, and the cluster stays `Provisioning` with its control plane shown as
  done. In both cases the events of the cluster name the refusal
  (`GET /v1/events/components/spectrocluster/<uid>`), and the status of the
  cluster stays `Provisioning` with no error field.
  When the create takes longer than its timeout, the provider gives a warning
  and ends the apply with success, so `just live-up` can end while the cluster
  still builds: `gke.sh` waits for `Running`.
- For `cloud = "eks"`: the six core policies of Palette as customer managed
  policies in the AWS account, by the names that `palette_core_policies` holds,
  and an `aws_profile` that can create an IAM role and a policy. See
  [An EKS cluster](#an-eks-cluster).
- A Palette API key in `PALETTE_API_KEY` or `~/.palette.env`.
- For `tiers.sh 6` only: a second key in `STRATA_LIVE_LIMITED_KEY`, the key of an active user with no role in the project. It cannot read `config/rbacs` of a cluster. The script never prints it.
- For `observer-role.sh` only: the key of a user with the role under test in `OBSERVER_ROLE_KEY`, and the name of the role in `OBSERVER_ROLE`. The script never prints the key.
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
- The AWS CLI with read access to the account, for `aws-inventory.sh`.
- An AWS profile that can create an ECR repository and an IAM user in the
  account, named in `terraform.tfvars` as `aws_profile`. Every apply needs it,
  because the example's own pack registry lives there. `registry.sh` needs
  `oras` as well, and logs in to the repository with that profile.
- For `registry-kinds.sh`: `oras`, `helm`, `zarf`, and the `spectro` CLI of
  Spectro Cloud ([Spectro Cloud CLI](https://docs.spectrocloud.com/registries-and-packs/spectro-cli-reference/)).
  The host needs no login of a person: the script reads its password from the
  OpenTofu outputs.
- On AWS, the AWS managed policy `AmazonEBSCSIDriverPolicy` on the worker node role
  `nodes.cluster-api-provider-aws.sigs.k8s.io`. Without it, the EBS CSI
  controller fails on each cluster with `UnauthorizedOperation` on
  `ec2:DescribeAvailabilityZones`. No strata check needs volumes, and such a
  cluster reaches `Running` all the same.

For a self-hosted Palette, set `PALETTE_API` in the environment or in
`~/.palette.env`. Strata reads it, and `tf.sh` and `tenant.sh` pass it on.

## An EKS cluster

With `cloud = "eks"`, cluster A is an EKS cluster: Amazon runs the control plane,
and one managed node group of two nodes runs the workers. The release profile is
an EKS profile: `amazon-linux-eks`, `kubernetes-eks`, `cni-aws-vpc-eks`, and
`csi-aws-ebs`.

EKS needs one more policy than a cluster on instances: the EKS policy of Palette
([Additional IAM policies](https://docs.spectrocloud.com/clusters/public-cloud/aws/required-iam-policies/additional-iam-policies-specific-use-cases/)).
The example does not add it to the role of `cloud_account_name`, which other
work can use. It makes its own cloud account instead, in `cloud-eks.tf`:

| Object | What it is |
| --- | --- |
| IAM role `strata-live-eks-cloud` | Trusts the AWS account of Palette, with the external id of the tenant, as the console asks for a cloud account with STS ([Add an AWS account](https://docs.spectrocloud.com/clusters/public-cloud/aws/add-aws-accounts/)). `GET /v1/clouds/aws/account/sts` gives both, and OpenTofu reads them through `api.sh`. |
| The core policies | The six customer managed policies of `palette_core_policies`, which the account holds already, attached to the role by name. |
| IAM policy `strata-live-PaletteControllersEKSPolicy` | The EKS policy of the public page, from `terraform/iam/eks-controllers.json`, attached to the role. |
| cloud account `strata-live-eks` | A cloud account of the project, of type `sts`, over the role. Cluster A uses it. |

`just live-up` makes each one, and `just live-down` removes each one. The
external id is sensitive, and OpenTofu keeps it in the local state alone.

A change of the node pool of an EKS cluster, step 4 of `eks.sh` with
`EKS_POOL_CHANGE=1`, leaves a node group outside the machine pools of Palette,
and the delete of the cluster then does not end by itself. The recovery, after
`just live-down` stops at the cluster:

1. `examples/live/eks-nodegroups.sh --delete`.
2. `examples/live/aws-cleanup.sh <cluster name> <region> --yes`: the EKS
   cluster, and then the VPC of the cluster with its NAT gateways, elastic IPs,
   internet gateway, subnets, route tables, and security groups, each found by
   the tag `sigs.k8s.io/cluster-api-provider-aws/cluster/<cluster name>`.
   Without `--yes` it lists what it would delete and deletes nothing.
3. `examples/live/aws-inventory.sh <cluster name> <region>` reads 0 for each kind.
4. `examples/live/api.sh delete-cluster <uid> --force`, and `just live-down`
   again.

A delete waits the same way after a delete of a cluster that is still
`Provisioning` (`delete-while-provisioning`), with no pool change: the
preflight plan met it on eks, and the same four steps end it. A second plain
delete, which the findings report on the pivot webhook gives as the step that
ends the wait, does not end it when the delete reads a control plane object
that is gone as an error; step 4 does.

For the questions of `eks.sh`, set `eks_k8s_bump` to a Kubernetes version of the
EKS pack, such as `1.36`: the example then also makes the profile version 1.0.9,
the release with that version alone.

## A GKE cluster

With `cloud = "gke"`, cluster A is a GKE cluster: Google runs the control plane,
and one node pool of three nodes runs the workers. The release profile is a GKE
profile: `cos-gke`, `kubernetes-gke`, `cni-vpc-native-gke`, and
`gke-csi-gcp-driver`. The cluster uses the GCP cloud account and project of
`cloud = "gcp"`, and needs no more access than the Kubernetes Engine API (see
[Prerequisites](#prerequisites)).

The four GKE packs have values of comments alone. So version 1.0.1 adds the key
`strataNext` to the values of the CSI pack, in place of the edit of
`reclaimPolicy` on the other clouds.

A change of the node pool of a GKE cluster, step 4 of `gke.sh` with
`GKE_POOL_CHANGE=1`, deletes the GKE node pool, and Palette makes no new one.
Palette gives the new machine pool the GKE name of the old one, `worker-pool`.
The delete of the old machine pool removes the GKE node pool, and the
controllers that make the new pool ran on its nodes. The cluster keeps no node.
The delete of the cluster then removes the GKE cluster, and stays `Deleting`,
so the recovery below ends it. The recovery, after `just live-down` stops at
the cluster:

1. `examples/live/gcp-inventory.sh <project> <cluster name> --gke` reads 0 for
   each kind. If a GKE cluster is left,
   `gcloud container clusters delete <cluster name> --region <region>` removes it.
2. `examples/live/api.sh delete-cluster <uid> --force`, and `just live-down`
   again.

For the questions of `gke.sh`, set `gke_k8s_bump` to a Kubernetes version of the
GKE pack, such as `1.35`: the example then also makes the profile version 1.0.9,
the release with that version alone.

## The preflight checks

Two scripts run the three tiers of `strata preflight` around each build.

**`preflight.sh`** checks the lab itself, before a recipe spends anything: the
tools on the PATH, the key, the credentials of the cloud CLI, the APIs of the
GCP project, the six core policies of an EKS run, the login of the OCI
registry, and the leftovers of a run before, with the inventories. `just
live-up` and `just live-profiles` run it first and stop at a failed check.
`just live-preflight` runs it alone. These are checks of the lab, not of
strata: the rows of the catalog with `engines: [lab]` name each one.

**`preflight-plan.sh ENGINE`** is the test plan of #91 for one engine, with
no cluster. It fills the entity of `examples/live/preflight/ENGINE.json` from
the tenant, then runs two parts. In the first, one edit of the entity makes each
fault the lab can make without a build, such as a GKE pool of 4, one zone on
EKS, a GKE cluster with no release channel, or one worker of four cores, and
`strata preflight --rule ID` must fail on it. In the second, the clean entity
runs through every tier, and each rule of severity `fail` that ran must pass.
The script appends the run to the ledger, `preflight/evidence/runs.yaml`, as
`caught`, `missed`, `passed`, or `false-fail` for each rule, and
`strata preflight --promotion` reads it. `--build` adds the build of cluster A
through the provider, with the watch on it from the moment Palette lists it,
until it is `Running` or a stuck state; then `strata preflight cluster:UID` on
the cluster as Palette holds it, which is the entity the provider sent, and on
a change of the instance type of its worker pool, which the rules on a change
must fail. `--build --down` tears the example down after, and reads the
inventory of the cloud; without `--down` the cluster stays, and `just
live-down` follows. The watch of the build streams to the terminal.

**`watch.sh`** is the watch a person sees. `just live-watch --build --down`
makes cluster B beside cluster A, and from the moment Palette lists it runs
`strata preflight cluster:UID --watch --verbose` on the terminal: one line for
each rule at each reading, every 30 seconds, until the cluster is `Running` or
a stuck state; then it removes cluster B. Cluster A stays as it is. Without
`--build` it waits, by name, for a build started elsewhere. The report of the
watch goes to `runs/<UTC time>/watch/out/`, or under `--out DIR`; `--every`
and `--limit` go to the watch.

```bash
just live-profiles                # or just live-up; the profiles must be there
just live-watch --build --down    # cluster B, watched, then removed: about 25 minutes
```

[Preflight: one cluster, end to end](./preflight-walkthrough.md) is the same
story typed one line at a time: check a body, break it, build it, watch it,
inspect it, remove it.

```bash
just live-down                                                # the profiles of one cloud at a time
examples/live/tf.sh apply -var cluster=false -var cloud=gke   # the profiles of the engine
just live-preflight-plan gke
just live-down
```

The profiles of each cloud share one name and one version, so an apply with
another cloud before a destroy is refused with `ClusterProfileAlreadyExists`:
OpenTofu creates the new profile before it destroys the old one. Run
`just live-down` between two clouds.

`PALETTE_RELEASE` names the Palette release for the ledger, when known: a
tenant key cannot read it. The runs are recorded in
[Live runs](./live-runs.md#preflight-plansh).

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
| `cloud` | optional; `aws` (the default), `gcp`, `eks`, or `gke`. Change it only after `just live-down` |
| `cloud_account_name` | an AWS cloud account name |
| `region`, `az` | the region and a zone |
| `ssh_key_name` | a key pair |
| `oci_registry_name` | an OCI pack registry that you can log in to |
| `aws_profile` | an AWS profile that can create an ECR repository and an IAM user in the account. Every apply needs a value. The module reads the profile unless `own_registries` is `false` and the cloud is not `eks` |
| `oci_pack`, `oci_pack_version` | an add-on pack in that registry, and its version |
| `own_registries` | optional; `true` by default. With `false`, the module makes no ECR repository, no IAM user, no Palette registry of its own, and no profile that needs one. Only `just contract-profiles` sets it |
| `helm_registry_name`, `helm_chart`, `helm_chart_version` | optional; the default is `nginx` 25.1.5 from `Bitnami` |
| `gcp_cloud_account_name`, `gcp_project` | for `cloud = "gcp"`: a GCP cloud account name, and a project it can use |
| `gcp_region`, `gcp_zone`, `gcp_instance_type` | optional; the defaults are `us-east1`, `us-east1-b`, and `e2-standard-4` |
| `eks_azs`, `eks_instance_type`, `eks_worker_count` | optional, for `cloud = "eks"`; the defaults are the zones `a` and `b` of `region`, `t3.xlarge`, and 2 |
| `eks_k8s_bump` | optional, for `cloud = "eks"`: a Kubernetes version for the profile version 1.0.9, the release with that version alone, which `eks.sh` asks Palette about |
| `gke_instance_type`, `gke_worker_count` | optional, for `cloud = "gke"`; the defaults are `e2-standard-4` and 3. Palette refuses a GKE worker pool whose size is not a multiple of 3 ("Node pool size should be multiple of 3") |
| `gke_k8s_bump` | optional, for `cloud = "gke"`: a Kubernetes version for the profile version 1.0.9, which `gke.sh` asks Palette about |
| `palette_core_policies` | optional, for `cloud = "eks"`: the names of the core policies of Palette in the account |
| `extra_addon_profile_uids` | optional; more add-on profile versions for the clusters to run, by uid, such as the profiles that `api.sh pin-profile` makes. The default is none |
| `cluster_timezone` | optional; the time zone of a cluster in the cluster template, as a name of the IANA time zone database. The default is `America/Chicago`. `template.sh` builds such a cluster, and Palette refuses a cluster in a template that has none: see [A cluster in a cluster template needs a time zone](../internals/palette/profiles.md#a-cluster-in-a-cluster-template-needs-a-time-zone) |

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

The scripts write to the tenant, so run them one at a time. Most start from the
baseline and restore it, so they run in any order. `template.sh`, `cluster.sh`,
`order.sh`, and `macro-repave.sh` do not: each one says what it leaves, so run
it last, and then `just live-down`.

```bash
examples/live/in-place.sh off                                                  # #27, legacy provider behavior
OFF_RUN=examples/live/runs/<time>/in-place/off examples/live/in-place.sh on    # #27, with the flag
examples/live/registry-move.sh                                                 # #24
examples/live/tag-only.sh                                                      # #19
just live-drift-report                                                         # #25
just live-packs                                                                # #55, read-only
just live-merge-gate                                                           # #57, needs just live-profiles
just live-registry                                                             # #66, needs just live-profiles, oras, helm, and aws
just live-registry-kinds                                                       # #75, needs just live-profiles, oras, helm, zarf, and spectro
just live-tiers                                                                # #76, needs just live-up; builds and removes cluster B
just live-tiers-limited                                                        # #276, needs just live-up and STRATA_LIVE_LIMITED_KEY; reads only
just live-tiers-pool                                                           # #515, needs just live-up; sets an override on a pool of cluster A, and puts the pool back
just live-template                                                             # #66, #276, needs just live-profiles and no cluster; builds two clusters; ends with live-down
just live-cluster                                                              # #66, needs just live-up; ends with the cluster gone
just live-order                                                                # #66, needs just live-up; leaves the versions edited, so live-down follows
just live-pack-states                                                          # #68, needs no cluster
just live-knurl                                                                # #66, read-only; the instance checks need just live-up
just live-who                                                                  # #69, stage 2 needs just live-up
just live-draft                                                                # #155, needs just live-profiles
just live-release                                                              # #146, needs only the key
just live-contract                                                             # #390, #391, #393, #576, needs the key and just examples; reads only, also the calls of the clouds; --with-cluster in a window with a cluster
just live-report-send URL                                                      # #397, needs the address of the endpoint and a `gh` login; reads no tenant
just live-validate-route BODIES                                                # #243, needs the key and a directory of bodies; no cluster
just live-route-bodies BODIES                                                  # #408, #413, needs just live-profiles on AWS; tries four add-on profiles, runs validate-route.sh on those that Palette takes, deletes them
just live-pending                                                              # #70, #276, needs just live-up
just live-macros                                                               # #72, needs just live-up
just live-image-digest                                                        # #630, needs just live-up, kubectl, and crane
just live-macro-repave                                                         # #72, needs just live-up; leaves a repave that waits, so live-down follows
just live-repave                                                               # #73, needs just live-up
just live-edge-host                                                            # #78, needs just live-up; registers edge hosts with no device
just live-cluster-group                                                        # #78, needs just live-up; builds a virtual cluster in cluster A
just live-app-profile                                                          # #78, needs no cluster
just live-fleet                                                                # #80, needs just live-up
just live-upstream                                                             # #79, needs just live-profiles
just live-loop                                                                 # #81, needs just live-up
just live-observer-kind                                                        # #644, needs just live-up, docker, kind, and kubectl; makes a kind cluster
just live-observer                                                             # #656, needs just live-up, kubectl, gh, and go; makes a namespace and a profile in cluster A, and removes both
just live-observer-images                                                       # #646, needs just live-up, docker, kubectl, and crane; makes two namespaces, a role, and images on ttl.sh
just live-observer-role                                                         # #645, needs just live-up, OBSERVER_ROLE, and OBSERVER_ROLE_KEY; only reads
just live-version-states                                                       # #81, needs just live-up
just live-eks                                                                  # #89, needs cloud = "eks" and just live-up
```

`just live-session` runs `verify.sh`, `in-place.sh off`, `in-place.sh on`, and
`registry-move.sh` in that order, under one `runs/<UTC time>/` directory, and
stops at the first failure.

- `in-place.sh` creates cluster `barney`: about 20 minutes. It deletes it at the
  end: 10 to 30 minutes. It waits for `Running` before the delete, because a
  delete of a cluster that is still provisioning stays in `Deleting`.
- `registry-move.sh` runs no cluster, and takes a few minutes.
- Each script writes `summary.md` and each output under its run directory. To
  continue an interrupted `in-place.sh`:
  `OUT=<that directory> examples/live/in-place.sh <off|on> <stage>`.

### 5. Remove it

Keep the uids first. The outputs go with the state, and after that
`tf.sh output -raw` prints the warning "No outputs found" on standard output: a
shell that exports it as `PALETTE_PROJECT_UID` sends that text as the project
header, and every request answers a plain-text `400 Bad Request`. When the
outputs are gone, take the project uid from `~/.palette.env`.

```bash
C=$(examples/live/tf.sh output -raw cluster_uid)
export PALETTE_PROJECT_UID=$(examples/live/tf.sh output -raw project_uid)
just live-down
examples/live/tenant.sh cluster "$C"                  # exit 0: Deleted
just live-inventory                                   # exit 0: nothing left
```

`live-down` also deletes each version of each profile name that the example
uses (the `profile_names` output), by exact name. With the flag on,
`skip_destroy` keeps the versions that a bump replaced out of the state, so a
destroy alone leaves them in Palette.

The destroy asks for approval. A run with no person at the terminal answers it
with `yes yes | just live-down`; the second attempt, after a refused read,
reads the answer too.

> **"Destroy complete" is not the confirmation.** During a gateway
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
| A script stops with `target/release/strata is older than <file>; run just build` | The sources changed after the last release build. A script that you run directly does not build first. Without this check, a run can use a binary that lacks an option the script needs, and fail every check that uses it. Run `just build`, then run the script again. |
| `consumer-is-the-cluster` fails with state `unknown` | The key cannot read the cluster in its project. `consumers-release.out` prints the error next to the cluster. |
| `only-the-edited-value` fails with two changed values | Somebody edited 1.0.1 in place and did not undo it. `baseline.sh` names it. |
| A script stops with "not at its baseline" | An earlier run did not finish, or a person changed the example. `baseline-before.txt` names each difference. `just live-down && just live-up` builds it again. |
| `registry-digest-oci` fails with "crane digest ... failed" or HTTP 401 | No login to the OCI registry, or an expired one. An AWS registry login lasts 12 hours. A login also stores an empty credential, and reports a success, when the token request before it failed: run `aws ecr get-login-password` alone first, and use a profile that has `ecr:GetAuthorizationToken`. |
| `ebs-csi-controller` pods in `CrashLoopBackOff`, with `UnauthorizedOperation` on `ec2:DescribeAvailabilityZones` | The worker node role does not have `AmazonEBSCSIDriverPolicy`. See Prerequisites. |
| A GCP cluster stays `Provisioning` with no instance, and the events say `Required 'compute.images.useReadOnly' permission for 'projects/spectro-image-build/global/images/<image>'` | No GCP image is shared for that pair of OS and Kubernetes versions. Use a version whose image `gcloud compute images describe <image> --project spectro-image-build` reads; `cloud-gcp.tf` sets one. |
| A GCP cluster stays `Provisioning` with its control plane instance running; `cloud-controller-manager` is in `CrashLoopBackOff`, and every other pod is `Pending` | The Compute Engine default service account has no role on the project. See Prerequisites. The cluster continues when it gets one. |
| A cluster stays `Provisioning` with `not done: WorkerNodeAdditionDone: provision worker nodes`, although its machines are `Running` and its nodes `Ready` | The condition of the workers was not read again. A restart of the Palette controller in the cluster does not change it. The deletion of such a cluster runs as usual: delete it, and send its diagnostics to Spectro Cloud support. |

## When a teardown is slow or stuck

These rules come from the teardowns of this kind of cluster:

- **Save what holds the cluster first.** Run `examples/live/diagnose.sh` (with
  `AWS_PROFILE` or `gcloud` for the inventory), or `tenant.sh diagnose <uid> <dir>` for
  one cluster. When the cluster is gone, the evidence is gone.
- **Do not delete a cluster that is still provisioning**, unless its machines are
  `Running` and its nodes `Ready`. A delete before that stays in `Deleting`,
  and each cloud resource stays.
- **A deletion continues in Palette after OpenTofu stops.** Check with
  `tenant.sh cluster`, and do not start again. While the state is not `Running`
  or `Deleted`, it prints the conditions that are not done and the three latest
  events.
- **Judge the progress by `just live-inventory`, not by the time.** Only 0 on each
  line means that the cloud is empty. On AWS, the instances, the NAT gateway,
  and the load balancer go first. The security groups, route tables, subnets, internet
  gateway, and VPC go last, some after a retry. Palette logs the event
  `Cluster <name> has been deleted` before the state changes: the state stays
  `Deleting` for several minutes after that event, and OpenTofu waits for the
  state. So an inventory at 0 with the state still `Deleting` is a deletion that
  runs, and not one that is stuck.
- **If the state stays `Deleting` and the inventory does not change** for
  several readings, read the events of the cluster in the Palette console. The
  message usually names what holds the deletion. A cluster with Pause Agent
  Upgrades on can stay in `Deleting`: disable that option and delete again.
- **Force a delete only when `just live-inventory` exits 0.** A forced delete
  removes the records of Palette and nothing in the cloud. An earlier force leaves
  resources that cost money with nothing to track them. Palette refuses a forced
  delete for about 15 minutes after the deletion starts, and can answer 500
  before it accepts one. A forced delete in that window does no harm once the
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
| `consumers-line-names-the-cluster` | 0 | #12: the comparison of a record against its cluster names that cluster among the consumers of the version |
| `snapshot-addon` | 0 | record the add-on profile version that the cluster runs |
| `compare-addon-cluster` | 0 | #23: the add-on record is `IDENTICAL` to the add-on selection of the cluster |
| `addon-has-a-variable` | 0 | #66: the add-on profile declares one variable, `purpose` |
| `cluster-variables-endpoint` | 0 | #66: `GET /v1/spectroclusters/<uid>/variables` serves the variable of the add-on profile |
| `snapshot-addon-copy` | 0 | #66: record the copy of the add-on profile that the cluster holds |
| `addon-copy-variables-hash` | 0 | #66: the copy has the variables hash of the profile, so the endpoint repeats the definition fields |
| `release-variables`, `release-variables-empty` | 0 | #190: the variables route of the release version, which declares no variables, gives `{"variables":[]}` |
| `missing-profile-variables` | 2 | #190: the variables route of a uid that Palette does not have fails |
| `missing-profile-not-found` | 0 | #190: that failure is HTTP 404 with the code `ResourceNotFound` |
| `snapshot-next` | 0 | record the newer version |
| `compare-next-cluster` | 1 | #1: against a newer record, the cluster shows the change |
| `compare-next-cluster-json` | 1 | #1: the same comparison as JSON |
| `only-the-edited-value` | 0 | #1: the only change is the key `csi_next_key` of the CSI pack |
| `consumers-release` | 1 | #12: the release version has a consumer |
| `consumers-release-json` | 1 | #12: the same listing as JSON |
| `consumer-is-the-cluster` | 0 | #12: listed with its name, project, and state |
| `consumers-next` | 0 | #12: no cluster runs the newer version |
| `list-applied`, `list-applied-json` | 0 | #63: `list --applied` names the profiles each cluster runs, as text and as JSON |
| `applied-is-what-the-cluster-runs` | 0 | #63: the listed profiles are those the cluster record names |
| `cluster-listing` | 0 | #63: read the cluster listing that strata lists clusters from |
| `listing-profile-fields` | 0 | #63: the fields of the cluster listing whose name holds `profile` |
| `candidate-file` | 0 | #21: write the record of the newer version as a candidate file |
| `snapshot-candidate` | 0 | #4: snapshot the candidate through registry lookups |
| `candidate-same-release` | 0 | #4: the root of the candidate equals the published root |
| `verify-release` | 0 | `verify` passes on a published version that nobody changed |
| `verify-cluster-record` | 0 | `verify` passes on a cluster record |
| `snapshot-registries` | 0 | record the profile with a Helm pack and an OCI pack |
| `registries-kinds` | 0 | #2: the profile holds one helm pack and one oci pack |
| `registries-provider-type` | 0 | #75: the oci pack records `registryProviderType` `pack`; the helm pack records none |
| `registry-digest-helm` | 0 | #2: read the digest of the helm pack from the registry itself |
| `registry-digest-oci` | 0 | #2: read the digest of the oci pack from the registry itself |

`strata candidate next.json -o candidate/profile.yaml --files-dir
candidate/files` writes the candidate file from a record. Its tags matter:
Palette stores the tag that a profile was authored with, and the provider pins
the version, `24.04`, where the registry names that Ubuntu version `LTS__24.x`.
