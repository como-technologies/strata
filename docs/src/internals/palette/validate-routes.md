# The validate routes

Palette has routes that check a cluster before a create. The console calls them
in its create wizard, and a create through the API goes to the create route.
This page states what each route checks, and who calls it. The checks come
from the code of Palette. Each fact that names a live run comes from that run.

`strata preflight` calls each route that takes the body of a create. One more
route checks the packs of a cluster before an update of its profiles:
`compare --validate` calls it. Two routes check the packs of a profile before a
create or an update: `snapshot --validate` calls them. Two more routes check
the name, the version, and the names of the packs and the manifests of a new
profile: `snapshot --from-candidate --validate` calls them too.

## POST /v1/spectroclusters/{cloudType}/validate

The body is the body of the create. The route exists for `aws`, `eks`, `gcp`,
`gke`, `azure`, `aks`, `maas`, `edge-native`, and `vsphere`; `gke` takes the
GCP entity, and `aks` the Azure entity.

The checks, in order:

1. The fields of the entity: a nil spec, the timezone, the cluster API
   override, and for each pool a size over 0, min and max that agree, and a
   size inside them. For a GKE worker pool, a size that is not a multiple of 3
   is refused here, as a 400 `InvalidParam` "Node pool size should be multiple
   of 3".
2. The name: valid, and not in use. The credit of the tenant.
3. The cloud: for AWS, the zones of each worker pool inside the zones of the
   control plane, and with dynamic placement one zone or more for each worker
   pool. For EKS, the KMS key when encryption is on. For edge-native and
   vSphere, the SSH key.
4. The profiles: an infrastructure profile is there, no pack is in two
   profiles, the packs and their variables, the state of each pack, and the
   FIPS mode. The errors go in `profiles[].packs.results[].errors[]`.
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

**Who calls it.** The console, in its create wizard. The Terraform provider,
palette-cli, and the SDK for cloud clusters do not. The create path of the API
runs the checks of the name and the limits, not those of the packs and the
machines.

### The variables of the body

The route reads `variables[]` of each profile of the body. Each variable must
be a variable of the profile, and its value must pass the schema of the
variable. Palette then puts each value into the values of the packs, for the
checks of the cloud config. One such check compares the CIDRs of the Kubernetes
pack with the CIDR of the VPC.

A body with no `variables[]` gives the route no value to check. A value of a
pack that reads `{{.spectro.var.NAME}}` then reaches the checks of the cloud
config unresolved. `strata preflight cluster:UID` sends the name and the value
of each variable of the cluster, so `palette-validate` answers for the cluster.
Palette masks the value of a sensitive variable in its answer, and strata
sends the mask.


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
   uid. Palette puts the sent values over it.
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

Some checks refuse the whole request: the YAML check of step 1, steps 3, 4,
and 5, and the check of a disabled pack in step 6. Such a refusal is HTTP `500`
with `code` and `message`. Palette gives each new error the response type of a
server fault, and these errors set no other type. So a planned refusal has the
status of a fault. The codes are in
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).

One refusal sets another type: `PackValuesOverrideForbidden` is HTTP `403`.
A cluster that does not exist is HTTP `404`.

A live run shows that the route accepts the packs of the release version on
the cluster that runs it, at the baseline. Stage 7 of `repave.sh` asks it on
cluster A: see [repave.sh](../../development/live-runs.md#repavesh).

## POST /v1/spectroclusters/{uid}/validate/repave

This route says whether a change of the profiles of a cluster replaces its
nodes. The body is the `v1SpectroClusterPacksEntity` of `validate/packs`.
`compare --repave` sends it. The request changes nothing.

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

Then Palette compares the cluster with the target, and gives a reason for each
pack that repaves the cluster.

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

It takes a candidate of `cert-manager` 1.21.1 from the Public Repo. See
[validate-packs.sh](../../development/live-runs.md#validate-packssh).

## The property routes

Each answers `204` on success, and a 4xx `V1Error` on a fault. Each calls the
cloud with the credentials of the cloud account.

| Route | Body | Checks | Called before a create? |
| --- | --- | --- | --- |
| `POST /v1/clouds/aws/properties/validate` | `cloudAccountUid`, `region`, `sshKeyName` | The account is not Pod Identity; the key pair exists. | On an AWS create. |
| `POST /v1/clouds/eks/properties/validate` | `cloudAccountUid`, `name`, `region`, `sshKeyName`, `vpcId`, `subnets[]` | The key pair; `eks:DescribeCluster` on the name; with a VPC, no public subnet without `MapPublicIpOnLaunch`. | On an EKS create. |
| `POST /v1/clouds/gcp/properties/validate` | `cloudAccountUid`, `projectId`, `region`, `azs[]` | The project, with Cloud Resource Manager; each zone, with Compute. | On a GCP create. Not on a GKE create. |
| `POST /v1/clouds/gcp/azs/validate` | `uid`, `project`, `region`, `azs[]` | Each zone is a zone of the region. | On demand. |
| `POST /v1/clouds/aws/s3/validate` | a bucket and credentials | The bucket lists. | On demand. |

No such route exists for Azure, MAAS, or vSphere.

## The account routes

`POST /v1/clouds/{aws,gcp,azure,maas,vsphere}/account/validate` takes the spec
of a cloud account and answers `204` when the account works: for AWS, the role
can be assumed and `ec2:DescribeRegions` answers; for GCP, the key parses and
the project list is not empty; for Azure, the subscriptions list; for MAAS, the
API authenticates; for vSphere, the credentials log in. A create or an update
of a cloud account calls it. A cluster create does not.

## What no route checks

The routes check the entity and the cloud account. A policy, a permission, a
cloud API that is off, an instance type in a zone, a quota, the count of zones
of EKS, and the release channel of GKE are checks of the cloud, and no route
runs them. Palette does not ask the cloud to simulate a policy or to test a
permission. `strata preflight` runs the tool of the cloud for some of
these, and prints `NOT CHECKED` with the command for the rest: see
[What strata preflight does not check](limits.md#what-strata-preflight-does-not-check).

## Where these answers come from

The code of Palette gives the checks of each route, the codes of the
refusals, and the shape of each answer. Each fact that names a live run comes
from that run. The findings report
`palette-precreate-validation-gaps-findings-2026-09-28` lists each check that
no route runs, and proposes the ones that fit a validate route to Spectro
Cloud.
