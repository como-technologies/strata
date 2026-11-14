# The validate routes

Palette has routes that check a cluster before a create. The console calls them
in its create wizard, and a create through the API goes to the create route.
This page states what each route checks, and who calls it. The checks come
from the code of Palette. Each fact that names a live run comes from that run.

`strata preflight` calls each route that takes the body of a create. One more
route checks the packs of a cluster before an update of its profiles:
`compare --validate` calls it for a version that the cluster does not run. Two
routes check the packs of a profile before a create or an update:
`snapshot --validate` calls them. Two more routes check the name, the version,
and the names of the packs and the manifests of a new profile:
`snapshot --from-candidate --validate` calls them too. `behind` calls the route
of an update, to ask about the bump of a pin. `compare --validate` calls it for
a version that the cluster runs.

## POST /v1/spectroclusters/{cloudType}/validate

The body is the body of the create. The route exists for `aws`, `eks`, `gcp`,
`gke`, `azure`, `aks`, `maas`, `edge-native`, and `vsphere`; `gke` takes the
GCP entity, and `aks` the Azure entity.

The checks, in order:

1. The fields of the entity: a nil spec, the timezone, the cluster API
   override, and for each pool a size over 0, min and max that agree, and a
   size inside them. For a GKE worker pool, a size that is not a multiple of 3
   is refused here, as a 400 `InvalidParam` "Node pool size should be multiple
   of 3". For AWS with a `vpcId`, each pool names a subnet: see below.
2. The name: valid, and not in use. The cluster limit of the tenant: a trial
   tenant has a limit on its active clusters, and the route refuses a create
   over it with `FreemiumActiveClusterLimitExceeded`. The limit holds for a
   trial tenant alone. `GET /v1/tenants/{tenantUid}/freemiumUsage` gives the
   usage. `palette-validate` reports the refusal.
3. The cloud: for AWS, the zones of each worker pool inside the zones of the
   control plane, and with dynamic placement one zone or more for each worker
   pool. For EKS, when encryption is on, that the KMS key is in the list of
   keys of the account: see [The property routes](#the-property-routes). For
   edge-native and vSphere, the SSH key.
4. The profiles: an infrastructure profile is there, no pack is in two
   profiles, the packs and their variables, the state of each pack, and the
   FIPS mode. The errors go in `profiles[].packs.results[].errors[]`. The route
   refuses the whole entity when a profile is of another cloud type than the
   route, and not of `all`: see below.
5. The machines: each pool against the CPU and memory the packs need. The
   errors go in `machinePools.results[].errors[]`: one worker of four cores
   with the packs of the live example is `MachinePoolWithMinCpuRequired`,
   "A minimum allocation of '4.1' CPU cores is required across all worker
   nodes. Current allocation : '4' CPU cores", in under 4 seconds. The EKS
   route answers `200` with no error for the same pool: the floor does not
   apply to a managed node group there, or the route does not check it. When a
   profile has an error, this step does not run, and `machinePools.results`
   is empty.
6. For AWS with a VPC: the pod and service CIDRs of the Kubernetes pack against
   the CIDR of the VPC. With dynamic placement, no CIDR check runs.

The answer is `200` with the errors of the packs and the pools, or a 4xx
`V1Error` with `code` and `message` for a refusal of the whole entity. The
route calls no cloud.

A refusal of the whole entity can also be HTTP `500`. A live run shows it for a
body with an add-on profile of another cloud type. The answer is HTTP `500`, the
code `ClusterProfilesNotSupported`, and the message
`Cluster profiles '<name>' are not supported with 'aws' environment`. A profile of
the cloud type `all` fits each cloud. Strata does not send this refusal again.
`palette-validate` reads it as the answer of the route, with the verdict
`failed` and the message of Palette: see
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).

A live run shows one more refusal with HTTP `500`: an AWS body with a `vpcId`
and a machine pool with no subnet. The code is `RequiredParam`, and the message
is `Parameter 'Subnets' is required`. The route gives it before the check of
the CIDRs. The code of Palette shows that the route gives `RequiredParam` for no
other cause. Strata does not send this refusal again. `palette-validate` reads
it as the answer of the route, with the verdict `failed` and the message of
Palette.

The route also refuses the whole entity with HTTP `500` and one of these codes.
The code of Palette shows it; no live run shows it yet.

- `InvalidDropDownVariable`: a value of a dropdown variable that is not the
  label of an option.
- `InvalidVariable`: a value that the format or the pattern of its variable
  refuses, or a required variable with no value.
- `K8SCidrOverlap`: for AWS with a VPC, a pod CIDR or a service CIDR that
  overlaps the CIDR of the VPC.
- `CidrParseError`: for AWS with a VPC, a pod CIDR or a service CIDR that is not
  a CIDR.

The route refuses more faults of the body with HTTP `500`, before or after the
checks above. The faults are in the machine pools, the profiles, the packs, the
variables, the instance types, and the labels and the SSH keys of the cloud. The
route refuses four faults with HTTP `403`, such as two system profiles.
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again)
lists each code. The code of Palette shows them; no live run shows them yet.
`palette-validate` reads each one as the answer of the route, with the verdict
`failed` and the message of Palette.

The check `cidr-overlap-status` of
[`route-bodies.sh`](../../development/live-runs.md#route-bodiessh) asks the
route about a pod CIDR that overlaps. Its body names a subnet for each pool, so
that the route reaches the check of the CIDRs. Strata does not send these refusals
again either. `palette-validate` reads each one as the answer of the route, with
the verdict `failed` and the message of Palette.

The same live run shows what the route does not check. It takes a body with an
add-on profile that holds a pack version that does not list the cloud type, and
`palette-validate` passes. Palette refuses such a pack at the create. So no
route answers this check before the create, and `pack-cloud-support` is a gap
that preflight fills: see
[The gaps that preflight fills](../../reference/preflight-rules.md#the-gaps-that-preflight-fills).

A live run on an EKS body shows the check of the AMI type of a pool. The route
takes the clean body. It refuses the type `AL2_x86_64` with
`UnsupportedAmiType`, and a type that is not known, such as `BOTTLEROCKET_x86_64`,
with `IsValidValue`. `palette-validate` reports each refusal with the message of
Palette. So the rule `eks-ami-type` left the catalog: see
[The checks that the route answers](../../reference/preflight-rules.md#the-checks-that-the-route-answers).

For a disabled pack and a deleted pack, see [Pack states](./packs.md#pack-states).

**Who calls it.** The console, in its create wizard. The Terraform provider,
palette-cli, and the SDK for cloud clusters do not. The create path of the API
runs the checks of the name and the limits, not those of the packs and the
machines.

### The permission of the route

The route checks the format of the body first. A body with no required field,
such as `spec.cloudConfig`, gets HTTP `422` and a number as the code, before the
check of the role. Then the route checks the fields of step 1 of the checks
above. Then it checks the role of the key, before it checks the profiles and the
rest of the content of the body. The code of Palette shows the order; no live
run shows it yet.

The route needs `cluster.create` in the project or in the tenant, also for a
body that creates nothing. So a key with a viewer role cannot ask the route. A
key whose role does not give `cluster.create` gets HTTP `403` and the code
`AccessDenied`:

```text
cluster.create operation is not permitted, required permissions 'cluster.create' in the scope(s) project or tenant or system
```

The order of the scopes in the message can change from one answer to the next.
The code of Palette shows it, and a field test with a read-only key got the
refusal. No live run of the example shows it yet: the check
`preflight-role-validate` of [observer-role.sh](../../development/live-runs.md#observer-rolesh)
asks the route with the key of a role. [The role that a key
needs](./clusters.md#the-role-that-a-key-needs) names the roles that give the
permission.

The route then reads each profile of the body. A role that does not give
`clusterProfile.get` gets HTTP `500` and the code `OperationForbidden`.

### The variables of the body

The route reads `variables[]` of each profile of the body. Each variable must
be a variable of the profile, and its value must pass the schema of the
variable. Palette then puts each value into the values of the packs, for the
checks of the cloud config. One such check compares the CIDRs of the Kubernetes
pack with the CIDR of the VPC.

When the schema of a variable does not take its value, the route answers HTTP
`500` and `InvalidVariable`, or `InvalidDropDownVariable` for a dropdown
variable. The code of Palette shows it; no live run shows it yet.

A body with no `variables[]` gives the route no value to check. A value of a
pack that reads `{{.spectro.var.NAME}}` then reaches the checks of the cloud
config unresolved. Palette masks the value of a sensitive variable in its
answer. See [A cluster as Palette holds it](../../commands/preflight/tiers.md#a-cluster-as-palette-holds-it)
for the variables that `strata preflight cluster:UID` sends.

Some refusals quote a value after Palette puts it in place:

- `K8SCidrOverlap` quotes the CIDR of the Kubernetes pack that overlaps the
  VPC: `Kubernetes <field> '<CIDR>' is overlapping with vpc '<VPC>' cidr block
  '<CIDR of the VPC>'`.
- `CidrParseError` quotes a value that is not a CIDR two times:
  `CIDR '<value> parse error; caused by: invalid CIDR address: <value>`.
- `InvalidDropDownVariable` quotes a label that is not an option of a dropdown
  variable: `... is invalid: label '<value>' is not a valid option`.

When the text comes from a variable, the refusal holds the value of the
variable. So the report of `strata preflight` writes a mark in place of each
value: see [The values in a message](../../commands/preflight/report.md#the-values-in-a-message).
The case `cidr-variable` of
[`validate-route.sh`](../../development/live-runs.md#validate-routesh) sends a
body whose pod CIDR comes from the variable `pod_cidr` of a profile. Its checks
make sure that the three reports hold the mark of the variable, and not its
value. No live run shows it yet.

## POST /v1/spectroclusters/{uid}/validate/packs

This route checks a cluster before an update of its profiles. The body is a
`v1SpectroClusterPacksEntity`, the body of `validate/repave`: `profiles[]`,
each with a `uid` and the `packValues` to send. The route treats the profiles
of the body as all the profiles that the cluster will run. `compare --validate`
sends it.

The checks, in order:

1. The cluster exists, and the values of each pack of the body are YAML.
2. For each profile of the body, Palette builds the target. The target is the
   copy that the cluster holds of that uid, or the published profile for a new
   uid. Palette puts the sent values over it. The tag of a copy stays: see
   [The sent tag](#the-sent-tag).
3. The metadata of the packs. Palette checks these items:
   - two dashboard packs that cannot go together,
   - the names of the packs and the manifests,
   - one pack or one manifest two times in a profile,
   - one pack name in two profiles of one workload scope: the system scope or
     the app scope,
   - two versions of one pack across the profiles.
4. The integration packs. Palette refuses the whole request with
   `PackSpectroIntegrationNotSupported` for a pack that the cloud of the
   cluster does not support. On an imported cluster, it also refuses each
   integration pack except Headlamp.
5. The integration packs again. The OIDC identity provider that an integration
   pack names must be in the Kubernetes pack or in the organization
   (`ClusterProfileOidcNotConfigured`). A pack that an integration pack names as
   a read-only dependency must keep its default values
   (`PackValuesOverrideForbidden`).
6. The pack validator, over the packs of each profile of the body that is not
   a system profile. It refuses a disabled pack (`DisabledPacksForbidden`).
   Then it checks the values and the dependencies of each pack. It checks a
   dependency over the packs of all the profiles of the body. So a pack that
   needs a newer version of a pack in another profile of the cluster fails
   here: `DependencyPackWithMinVersionRequired`,
   `Pack '<pack>' is required with minimum compatible version '<version>'`. On
   an imported cluster, the validator checks only the values.

The answer is `200` with `profiles[]`: only the profiles with an error, each
with its `uid` and `packs.results[].errors[]` of `code` and `message`. Palette
puts the error of a pack under each profile of the body that holds a pack of
that name. It gives the profiles in no fixed order. With no error, the answer
has no profile. `machinePools` stays empty: the route checks no machine. The
route does not say whether the change repaves: `validate/repave` answers that.
The route checks no variable and no minor version of Kubernetes: see
[What the update checks](#what-the-update-checks).

Some checks refuse the whole request: the YAML check of step 1, steps 3, 4,
and 5, and the check of a disabled pack in step 6. Such a refusal is HTTP `500`
with `code` and `message`. Palette gives each new error the response type of a
server fault, and these errors set no other type. So a planned refusal has the
status of a fault. The codes are in
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).

One refusal sets another type: `PackValuesOverrideForbidden` is HTTP `403`.
A cluster that does not exist is HTTP `404`.

A live run shows that the route accepts the packs of the release version on
the cluster that runs it, at the baseline. Stage 7 of `repave.sh` asks the
question for cluster A: see [repave.sh](../../development/live-runs.md#repavesh).
The route of the version answers it: see [The sent tag](#the-sent-tag).

### The sent tag

For a profile that the cluster runs, the route checks no sent tag. Palette
builds the target from the copy that the cluster holds, and puts the sent values
over it. The tag of the copy stays. For a uid that the cluster does not run, the
target is the published profile, so its tags count.

A live run shows it. Cluster `fred` runs Kubernetes 1.36.2. The route answers
HTTP 200 and `profiles: []`, with no error, for three bodies. The bodies differ
only in the tag of `kubernetes`: `1.36.2` (no move), `1.37.0`, and `9.99.9`. The
registry does not have 9.99.9.

Another live run shows the same for the move of `fred` from Kubernetes 1.35.8
to 1.37.0. The route of the cluster accepts it, and the route of the profile
version refuses it: see [The upgrade dependency](#the-upgrade-dependency).

So the answer of this route says nothing about the move of a pack in a profile
that the cluster runs. A team reads `palette accepts` as "Palette takes the
update", and the update can fail. Two commands ask the route of the profile
version that the cluster runs, in its place:

- `behind` sends the bump of a cluster record to the route of the version.
- `compare --validate` sends the packs of a profile version to the route of the
  version, when the cluster runs it. For any other version, it sends this route.

Stage 6 of `validate-packs.sh` sends this route a body with the tag `9.99.9`:
see [validate-packs.sh](../../development/live-runs.md#validate-packssh).

## POST /v1/spectroclusters/{uid}/validate/repave

This route says whether a change of the profiles of a cluster replaces its
nodes. The body is the `v1SpectroClusterPacksEntity` of `validate/packs`.
`compare --repave` sends it, and so do the checks of `engine-repave-rules`,
`eks-repave-answers`, and `gke-repave-answers`. The request changes no setting
of the cluster. For a pack of virtual machines, Palette can save a record: see
[What the route saves](#what-the-route-saves).

Palette builds the target cluster from the body. These facts come from the
code of Palette, not from a live run:

- A profile that the cluster runs, with no `packValues`, keeps the copy that
  the cluster holds.
- A profile that the cluster runs, with `packValues`, holds only the packs that
  the body names. Palette removes each other pack of the profile.
- Palette puts the sent values over the copy of each named pack. When the sent
  tag is not the version of that copy, Palette takes the pack from the profile
  as it is now.
- Palette takes a named pack that the cluster does not run from the profile as
  it is now. It ignores a name that the profile does not hold.
- A profile that the cluster does not run is the published profile, with the
  sent values over it.
- The target holds no profile that the body does not name.
- Palette reads no `replaceWithProfile` and no `variables[]` of the body.

Then Palette compares the cluster with the target, and gives a reason for each
pack that repaves the cluster.

So the body of a swap names each profile of the cluster by its uid, with the
new uid in place of the old one. A body that names the old uid alone, with the
`packValues` of the new version, asks about the old version with other values.
Palette then takes a pack with a new tag from the old version. See
[The body of a swap](./profiles.md#the-body-of-a-swap) for the answers to each
body.

### What the route saves

These facts come from the code of Palette, not from a live run. Palette builds
the cluster as it is now, and then the target, in the form that it gives to the
agent of the cluster. Each build saves at most two records. One is for the
console of `virtual-machine-orchestrator`, or of its older name
`spectro-vm-dashboard`. The other is for the console of
`vm-migration-assistant`.

The console reaches the service of the pack at an address of its own,
`/v1/tenantApps/<key>` under the address of Palette. Palette makes the key from
the service and the cluster uid. The record holds the address that the console
forwards to. A cluster and a body with none of the three packs get no record.

- The address is the value `charts.<pack>.appConfig.clusterInfo.consoleBaseAddress`
  of the pack. With no such value, Palette makes the address from the
  `serviceType` of the pack: the address of its load balancer, or an address
  under the proxy of Palette. Palette saves no record while the load balancer
  has no address.
- The build of the cluster saves the address from the values that the cluster
  holds. The agent of the cluster reads the cluster in the same form, so it
  saves the same address.
- The build of the target saves the address from the values of the body. So a
  body with other values of the pack moves the address of the console, until
  the agent reads the cluster again.
- A body that adds one of the packs can save a record for a cluster that does
  not run it. With a load balancer and no `consoleBaseAddress`, Palette saves
  no record, because the cluster has no such load balancer.
- When the cluster runs the pack, a body without its values does not stop the
  save: the build of the cluster saves the record.
- A body can name the profile of the pack by its uid alone, with no
  `packValues`. Such a body can also move the address that the console forwards
  to, until the agent reads the cluster again.
- Palette answers a cluster that a cluster template governs with no reasons, and
  saves no record. `compare --repave` asks the route of the template for such a
  cluster.

`POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave` asks the same
question about one cluster of the template, with each profile by its uid alone.
So it saves the same records for that cluster, and it can move the address
that the console forwards to, as above. A template with no cluster gets no
record. `POST /v1/spectroclusters/{cloudType}/validate` saves no record.

The forward of the console keeps the address that it reads for a console,
usually for at most 15 minutes. It keeps the answer 404 in the same way when it
finds no record. In some cases, it keeps an address until Palette starts the
forward service again. When the forward keeps no address for a console, the
next request reads the record. So a new record shows at once.

A line of a report that sends this route says what Palette saves, in a comment:
see [The reproduce lines](../../reference/preflight-rules.md#the-reproduce-lines).
Stage 9 of `repave.sh` is the live check: see
[repave.sh](../../development/live-runs.md#repavesh).

## POST /v1/clusterprofiles/validate/packs

This route checks the packs of a profile before a create. The body is a
`v1ClusterProfileTemplateDraft`: `cloudType`, `type`, and `packs[]`. Each pack
is a `v1PackManifestEntity` with `name`, `tag`, `uid`, `registryUid`, `type`,
`layer`, `values`, and `manifests[]` of `name` and `content`. The route writes
nothing. It does not check the name or the version of the profile, or the
names of the packs and the manifests. The
[name route](#get-v1clusterprofilesvalidatename) and the
[import validate route](#post-v1clusterprofilesimportvalidate) check them.
`snapshot --from-candidate --validate` sends it.

The checks, in order:

1. The body holds one pack or more. Palette refuses an empty `packs` with
   HTTP 400 and the code `ShouldNotBeEmpty`, before the checks below. A live
   request shows it:
   `Parameter 'Packs' should not be empty`. The API schema also refuses two
   equal packs with HTTP 422 and the code `610`, as the import validate route
   does. A live run shows it.
2. The body holds no more than one dashboard pack:
   `ClusterDashboardPacksMutuallyExclusive`.
3. The values and the manifests of each pack are YAML:
   `PackValuesYamlInvalid` or `PackManifestYamlInvalid`, for the first pack that
   fails.
4. The pack validator. It skips each manifest pack, and reads each other pack
   by its `uid`. A pack that Spectro Cloud disabled is
   `DisabledPacksForbidden`. Then it checks the values of each pack, and the
   dependencies of each pack over the packs of the body. In an add-on profile,
   it does not check a dependency on an infrastructure layer.
5. The limits: 200 packs and 200 manifests,
   `SubResourceLimitValidationFailure`.

The validator gives an error of a pack in a `200` answer:
`packs.results[]`, each with the `name` of a pack and `errors[]` of `code` and
`message`. With no error, `results` is empty. Each other check refuses the
whole request with a `V1Error` of `code` and `message`. Palette gives these
errors the response type of a server fault, so each one is HTTP 500.

A live run shows it: values that are not YAML get HTTP 500 with the code
`PackValuesYamlInvalid` and the message
`Pack 'cert-manager' values yaml is invalid`. Strata reads a 500 with a code
of these two routes as the answer of Palette, and does not send it again: see
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).
It reads each other status as an error.

A live run also shows that the route takes a candidate of `cert-manager`
1.21.1 from the Public Repo, with the values of the registry. See
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

## POST /v1/clusterprofiles/{uid}/validate/packs

This route checks the packs of a profile version before an update of that
version. The body is the body of the route above. `snapshot <ref> --validate`
sends it with the packs of the version, so Palette checks the version as an
update of itself.

The checks, in order:

1. The body holds one pack or more, as above.
2. The version exists, and the key can read it.
3. Palette reads the clusters that run the version. It refuses a pack of the
   body when another profile of such a cluster has a pack of that name in the
   same workload scope. The refusal is `ClusterProfilePacksInConflict`,
   with the names of the clusters.
4. The limits, the dashboard packs, and the YAML, as above.
5. The pack validator, as above. It also checks the dependencies over the packs
   that each such cluster runs.

The answer has the shape of the route above. A live run shows that the route
takes a published version of a cluster profile that no cluster runs, as an
update of itself. Stage 3 of `validate-packs.sh` asks it for a version that you
name.

### The upgrade dependency

A pack can declare a dependency of the type `upgrade`. Its `maxVersion`, such as
`+0.1`, limits the move of a cluster. The first three items come from the code of
Palette, and the last one from the public documentation. The paragraphs after
the list come from a live run:

- The pack validator reads the dependency of each pack of the body. It runs over
  each cluster that runs the version.
- For each cluster, Palette takes the version of the pack that the cluster runs.
  It derives the highest minor version that the cluster can move to.
- A pack of the body above that version is an error in `packs.results[]`:
  `DependencyPackUpgradeNotSupported`. The error holds the names of the clusters.
  The answer has a code and a message for each error, and no field for the
  clusters, so the names are in the message.
- The public documentation states the rule for Kubernetes: update to a newer
  version one minor version at a time.

A live run shows the refusal. Cluster A (`fred`) runs Kubernetes 1.35.8 from the
version of the profile. For the bump to 1.37.0, two minor versions above,
Palette refuses the move:

```text
DependencyPackUpgradeNotSupported
Pack upgrade to '1.37.0' is not compatible with the clusters 'fred'
```

The route accepts the bumps of the other pins of that version: `cni-cilium-oss`
1.20.1 to 1.20.3, and `csi-aws-ebs` 1.65.0 to 1.67.1. See
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

No read route gives the dependency. The summary of a pack has no field for it.
The `packValues[].dependencies` of `GET /v1/packs/{uid}` and of the tags route
hold the template dependencies. They are `[]` for `kubernetes` 1.36.2 in the
Public Repo. So strata cannot derive the limit, and asks the route.

`behind` sends this route for a record of a profile version. It also sends it for
a record of a cluster, with the uid of the profile version that the cluster
runs. See
[Palette accepts the move, or refuses it](../../commands/behind.md#palette-accepts-the-move-or-refuses-it)
for the body. A pin of `kubernetes` 1.35.3, with 1.37.0 in the registry, is a
move of two minor versions. Palette refuses it on each cluster that runs 1.35.
The pin and the registry decide whether a live example has such a pair.

The limit is a dependency of the pack, and not a rule of the route. So the route
accepts a move of two minor versions of a pack with no such dependency. The
update refuses each such move: see
[What the update checks](#what-the-update-checks).

Stage 6 of `validate-packs.sh` asks the route for a move of Kubernetes by two
minor versions, on a cluster that runs the profile. It needs a profile version
that pins an older Kubernetes than the example does.

## What the update checks

The update of the profiles of a cluster checks two rules that no validate route
checks. The update is a `PUT` or a `PATCH` of
`/v1/spectroclusters/{uid}/profiles`. The code of Palette shows each rule; no
live run shows it yet. Stage 7 of `validate-packs.sh` is the live check: see
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

### The minor version of Kubernetes

- One update moves the pack of the `k8s` layer one minor version at most. A
  move to a higher major version is refused too. The rule holds for each pack of
  that layer, such as `kubernetes` and `kubernetes-eks`, on each cloud.
- Palette refuses a move that skips with HTTP 400 and `K8sUpgradeNotSupported`:
  `Kubernetes upgrade from <current> to <new> is not supported`.
- Palette reads each version as a semantic version, or as the first three
  numbers of the text. When it reads no version, as in `1.29.x`, it makes no
  check.
- The current version depends on the request:
  - The update of the profiles of a cluster, and the apply of an update
    notification: the version that the cluster runs.
  - A `PUT` of a profile version that a cluster runs: the lowest version that a
    cluster of the version runs.
  - The publish of a draft of a version that a cluster runs: the published
    version.
  - The update of the profiles of a cluster template, and the attach of a
    cluster to a template: the version that each cluster runs.
- `validate/packs` of a cluster or of a profile version does not check the
  rule, and neither does `validate/repave`. A pack can also declare a limit of
  its own: see [The upgrade dependency](#the-upgrade-dependency).

The public documentation states the same rule: update Kubernetes one minor
version at a time.

### A value of each required variable

- The update gives each variable of the new profile version a value: the value
  of the body, or the default value of the variable. A dropdown variable with
  no default value takes the label of its default option, the option with
  `default: true`. Palette reads no option of a variable of another input
  type. The code of Palette shows it; no live run shows it yet.
- Palette refuses a required variable with no value, and changes nothing. The
  answer is HTTP 500 and `InvalidVariable`. The message ends with
  `can't be empty for required variable '<name>'`.
- No validate route checks a variable. The body of the profile routes has no
  field for one.

`behind` and `compare --validate` check the two rules before the update: see
[The gaps that strata fills](./profiles.md#the-checks-of-an-update).

## GET /v1/clusterprofiles/validate/name

This route checks the name and the version of a new profile version. It takes
the query parameters `name` and `version`, and writes nothing.
`snapshot --from-candidate --validate` sends it.

The checks, in order:

1. The name is not empty. An empty name is HTTP 500 with the code
   `ResourceNameEmpty`. An empty version becomes `1.0.0`.
2. The project does not hold a version with this name and this version. Palette
   compares the name with no case, and the version as text, so `v1.0.0` is not
   `1.0.0`. A match is HTTP 409 with the code `ClusterProfileAlreadyExists`, and
   the message
   `Cluster Profile with name '<name>' and version '<version>' already exists`.
3. The version parses as semver: an optional `v`, one to three numbers, and an
   optional pre-release and build data. A version that does not parse is
   HTTP 500 with the code `VersionParseFailure`, and the message
   `Unable to parse the version '<version>'`.

The route does not check the form of the name. On success, the answer is `204`
with no body.

A live run shows it: the route takes the version `1.1` and a name that the
project does not hold with `204`. It refuses the version `2.2.2.develop` with
HTTP 500 and `VersionParseFailure`. It refuses the name and the version of a
profile that the project holds with `ClusterProfileAlreadyExists`.
`snapshot --validate` of such a candidate prints the refusal, exits `2`, and
writes no record. See
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

## POST /v1/clusterprofiles/import/validate

This route checks a profile as an import, before the import writes it. The body
is a `v1ClusterProfileImportEntity`, the shape of a profile export:
`metadata.name`, `spec.version`, `spec.variables`, and `spec.template` with
`type`, `cloudType`, and `packs[]`. Each pack has `name`, `type`, `layer`,
`version`, `tag`, `values`, `manifests[]` of `name` and `content`, and
`registry.metadata`. The body holds no pack uid. The route writes nothing.
`snapshot --from-candidate --validate` sends it.

The checks, in order:

1. The API schema of the body. A list with two equal items is HTTP 422, with
   the number `610` as the code, and the message
   `packs in body shouldn't contain duplicates`.
2. Each pack is in a registry. Palette finds a pack by its name, its `version`,
   and the registries of its type. It uses the name, the kind, `isPrivate`, and
   `providerType` of `registry.metadata` to choose the registries. When no
   registry holds the pack, the route stops with HTTP 500 and a code such as
   `ClusterProfileImportPackNotFound`, and checks nothing more.
3. The key can create a profile, and the tenant holds fewer profiles than its
   limit.
4. The checks of a create that the route has. Palette runs them at the same
   time, and gives the first error in an order that is not fixed. So when two
   checks fail, a request can give either error. The checks are:
   - The limits of 200 packs and 200 manifests:
     `SubResourceLimitValidationFailure`.
   - The cloud type: `UnSupportedCloudType`.
   - Two dashboard packs that cannot go together:
     `ClusterDashboardPacksMutuallyExclusive`.
   - The name of each manifest, and of each manifest pack, as a DNS-1123 label
     of 32 characters at most: `InvalidName`.
   - One pack name two times: `DuplicatePacksForbidden`.
   - One manifest name two times in a pack: `DuplicatePackManifestsForbidden`.
   - Two versions of one pack: `MultiPackVersionsForbidden`.
   - The YAML of the values and the manifests: `PackValuesYamlInvalid` and
     `PackManifestYamlInvalid`.
   - The integration packs: `PackSpectroIntegrationNotSupported`.
   - A pack that Spectro Cloud disabled: `DisabledPacksForbidden`.
   - The name and the version, as the name route checks them.
   - A macro that the values use and that Palette does not define:
     `PackMacroUndefined`, HTTP 404.
   - The variables, and the values and the dependencies of each pack:
     `ClusterProfileCreationValidationFailure`.

The answer is `200` with the body that Palette takes, and the registry of each
pack filled in. Each refusal is HTTP 500 with `code` and `message`, except
`ClusterProfileAlreadyExists` (409) and `PackMacroUndefined` (404). The route
gives one error only.

The route does not check four rules of the create. These are the layers of
the type, the core layers, the cloud of each pack, and the tag of a pack that
Palette makes. Only the create checks them: see
[The gaps that strata fills](./profiles.md#the-gaps-that-strata-fills).

A live run shows that the route refuses each of these faults:

- A pack name two times: `DuplicatePacksForbidden`.
- Two equal packs: HTTP 422 and `610`.
- A manifest name that is not a DNS label: `InvalidName`.
- A manifest name two times in a pack: `DuplicatePackManifestsForbidden`.
- The version `2.2.2.develop`: `VersionParseFailure`.
- A pack with the mark `disabled`, such as `gloo` 1.2.18: HTTP 500 and
  `DisabledPacksForbidden`, with the message
  `Disabled packs 'gloo' are forbidden`.

It takes a candidate of `cert-manager` 1.21.1 from the Public Repo. See
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

## The property routes

Each answers a request that is right with a 2xx status, and a fault with an
HTTP error and a `V1Error`. Each calls the cloud with the credentials of the
cloud account. A live run shows the answers of the AWS route: see
[The list routes](#the-list-routes).

| Route | Body | Checks | Called before a create? |
| --- | --- | --- | --- |
| `POST /v1/clouds/aws/properties/validate` | `cloudAccountUid`, `region`, `sshKeyName` | The account is not Pod Identity; the key pair exists. | On an AWS create. |
| `POST /v1/clouds/eks/properties/validate` | `cloudAccountUid`, `name`, `region`, `sshKeyName`, `vpcId`, `subnets[]` | The key pair; `eks:DescribeCluster` on the name; with a VPC, no public subnet without `MapPublicIpOnLaunch`. | On an EKS create. |
| `POST /v1/clouds/gcp/properties/validate` | `cloudAccountUid`, `projectId`, `region`, `azs[]` | The project, with Cloud Resource Manager; each zone, with Compute. `gcp-properties` calls it. | On a GCP create. Not on a GKE create. |
| `POST /v1/clouds/gcp/azs/validate` | `uid`, `project`, `region`, `azs[]` | Each zone is a zone of the region. | On demand. |
| `POST /v1/clouds/aws/s3/validate` | a bucket and credentials | The bucket lists. | On demand. |
| `GET /v1/clouds/aws/regions/{region}/kmskeys/validate?cloudAccountUid=&keyArn=` | `cloudAccountUid` and `keyArn`, in the query | That the ARN is in the `kms:ListKeys` list of the account. | On an EKS create, when encryption is on. |

No such route exists for Azure, MAAS, or vSphere.

The AWS route checks the cloud account before the key pair. A Pod Identity account serves EKS
alone, so the route refuses it with HTTP `500` and the code `FeatureNotSupported`. The refusal
names the feature "Pod Identity for AWS infrastructure clusters". Palette gives it the type of a
server fault, as it does for `AwsKeyPairNotFound`. These facts come from the code of Palette,
not from a live run. Palette also refuses the create of a Pod Identity account in the SaaS mode,
with the same code. So a lab on SaaS cannot hold the account that shows the refusal.

### The KMS key route

The KMS key route checks the list of keys, not the key. It answers `204` when
the ARN is in the `kms:ListKeys` list of the account. It reads no key state
and no `kms:CreateGrant`. So a disabled key, or a key that the role cannot use
to create a grant, passes it. When the role has no `kms:ListKeys`, the route
answers `500`, not a 4xx. So `palette-validate` finds a key that the account
does not list, and no other fault of the key. These facts come from the code
of Palette, not from a live run.

The `kms` rule of `strata preflight` reads the state and the usage of the key
with `aws kms describe-key`. It does not test `kms:CreateGrant`, which depends
on the key policy and the IAM policy of the role together. The reproduce lines
of the rule read the key policy and call the route.

## The list routes

Five routes list what the cloud account holds. They refuse no cluster.
Strata reads each list and compares it with the entity.

| Route | Gives | Read by |
| --- | --- | --- |
| `GET /v1/clouds/{aws,gcp,azure}/regions/{region}/instancetypes?cloudAccountUid=&subscriptionId=&cpuGtEq=0&memoryGtEq=0` | Each instance type of the region: `type`, `category`, `cpu`, `memory`, `price`, `cost`, `supportedArchitectures[]`, and `nonSupportedZones` (an array, or `null`). | `instance-offering`, `aws-quotas`, `gcp-quotas`, `azure-vcpu-quota` |
| `GET /v1/clouds/aws/regions/{region}/keypairs?cloudAccountUid=` | `keyNames[]`: the key pairs of the account in the region. | `ssh-key-pair` |
| `GET /v1/clouds/aws/regions?cloudAccountUid=` | `regions[].name`: the regions that the account has enabled. | `aws-region-not-enabled` |
| `GET /v1/clouds/aws/regions/{region}/vpcs?cloudAccountUid=` | `vpcs[]`: each VPC of the account in the region, with `vpcId`, `name`, `cidrBlock`, and `subnets[]`. A subnet has `subnetId`, `name`, `az`, `isPrivate`, and `mapPublicIpOnLaunch`. | `static-subnets` |
| `GET /v1/clouds/aws/securitygroups?cloudAccountUid=&region=&vpcId=` | `groups[]`: the security groups of the VPC, with `groupId`, `groupName`, and `ownerId`. | `sg-name-collision` |

The query `cloudAccountUid` of the regions route is required. A request with no `cloudAccountUid`
gets HTTP 422. The VPC route and the key pair route require it too. The security group route requires `region` and
`vpcId`, and a request with no query gets HTTP 422. The `vpcId` is the id of a VPC from the VPC
listing of the same region.

The regions route lists the regions that answer a dry run with the key of the account. AWS can still
refuse the VPCs of a region of the list. A region can also lack the metadata of its instance types.
Both cases give HTTP 500: see
[What the source of Palette says](limits.md#what-the-source-of-palette-says).

The VPC route lists no count of free addresses and no tag for a subnet. Its `isPrivate` is true when
the route table of the subnet has no route to an internet gateway. A subnet belongs to the VPC entry
that holds it, and the subnet has no field for its VPC. The zone route
`GET /v1/clouds/aws/regions/{region}/availabilityzones?cloudAccountUid=` lists `zones[]` with `name`,
`state`, and `zoneId`. It lists no zone type. No rule reads the zone route.

For an AWS type, `cpu` is the count of vCPUs: a `t3.xlarge` gives 4. `memory` is in GiB. `gpu` is empty
for each AWS type, also for a type with a GPU. The three quota rules count the vCPUs of a pool from
`cpu`. The GCP and Azure routes give the same fields, and no run shows their numbers.

The instance type route filters its answer. When the query does not say, it lists only the types with 2 vCPUs or
more and 4 GiB of memory or more. A run shows it on AWS: with no query, the route lists no type
under those limits. The query `cpuGtEq=0&memoryGtEq=0` lists more types. A call by hand showed that
`t3.small` and `t3.micro` are absent with no query, and listed with that query. No run shows this
default on GCP and Azure: see
[What the source of Palette says](limits.md#what-the-source-of-palette-says).

Strata sends `cpuGtEq=0&memoryGtEq=0` with `cloudAccountUid`, and with `subscriptionId` for Azure.
For one engine, the four rules send the same query, so one request answers them.

The AWS route omits some old instance families, also with that query. A call by hand shows that
`m3.large` is absent. A quota rule fails for a pool whose type the route does not list, because it
cannot count the vCPUs of that pool. A rule reads no quota from these routes, because no route of
Palette lists a quota. Some accounts see fewer types: see
[What the source of Palette says](limits.md#what-the-source-of-palette-says).

`type` of an entry names the instance type. An entry has no `name`.
`nonSupportedZones` of a type is an array of the zones that do not offer it.
It is `null` for a type that each zone offers. Most types give `null`. So one
request answers every pool of the entity. A pool fails when its type names a
zone of the pool, or when the region does not list the type. A type that the
list does not mark can still fail in the cloud.

The property route of EKS checks the key pair only when the body has a `vpcId`.
An EKS cluster with a dynamic VPC reaches no check of the key pair, and the key
pair route answers it. The console calls the property route of AWS on an AWS
create, and that route checks the key pair.

A live run shows that the property route of AWS refuses a key pair that the
region lacks. The answer is HTTP `500` with the code `AwsKeyPairNotFound`. The
message reads:

```text
Aws KeyPair 'strata-no-such-key' not found for region us-east-2; caused by: InvalidKeyPair.NotFound: The key pair 'strata-no-such-key' does not exist
```

The same body with the key pair of the lab gets HTTP `200`. `iaas-pod-identity`
sends the key pair to that route, so `ssh-key-pair` does not name AWS.

A run on AWS also shows the key pair route. `ssh-key-pair` finds the key pair of
the clean entity in `keyNames`, and fails a name that is not there. A run shows
the answer of the AWS instance type route: its body holds `instanceTypes` alone.

No run shows the GCP answer of the instance type route. The rule reads `type`
for GCP too, and the checks `instance-types-shape` and `instancetypes-cpu` show the GCP entry.
No run shows the Azure answer, because the lab has no account of Azure. The
stand-in of the book models the GCP answer: see
[What no run covers](limits.md#what-no-run-covers).

## The account routes

`POST /v1/clouds/{aws,gcp,azure,maas,vsphere}/account/validate` takes the spec
of a cloud account and answers `204` when the account works: for AWS, the role
can be assumed and `ec2:DescribeRegions` answers; for GCP, the key parses and
the project list is not empty; for Azure, the subscriptions list; for MAAS, the
API authenticates; for vSphere, the credentials log in. A create or an update
of a cloud account calls it. A cluster create does not.

## What no route checks

The routes check the entity and the cloud account. A policy, a permission, a
cloud API that is off, a quota, the count of zones of EKS, and the release
channel of GKE are checks of the cloud, and no route runs them. A route lists
the zones that do not offer an instance type: see
[The list routes](#the-list-routes). Palette does not ask the cloud to simulate
a policy or to test a permission. `strata preflight` runs the tool of the cloud for some of
these, and prints `NOT CHECKED` with the command for the rest: see
[What strata preflight does not check](limits.md#what-strata-preflight-does-not-check).

Two rules of the update of a cluster are in no route either: see
[What the update checks](#what-the-update-checks).

## Where these answers come from

The code of Palette gives the checks of each route, the codes of the
refusals, and the shape of each answer. Each fact that names a live run comes
from that run. The findings report of a live run lists each check that no route
runs, and proposes the ones that fit a validate route to Spectro Cloud.
