# The validate routes

Palette has routes that check a cluster before a create. The console calls them
in its create wizard, and a create through the API goes to the create route.
This page states what each route checks, from the source of Palette (hubble on
master), and who calls it. `strata preflight` calls each one that takes the
body of a create. One more route checks the packs of a cluster before an update
of its profiles: `compare --validate` calls it. Two routes check the packs of a
profile before a create or an update: `snapshot --validate` calls them.

## POST /v1/spectroclusters/{cloudType}/validate

The body is the body of the create. The route exists for `aws`, `eks`, `gcp`,
`gke`, `azure`, `aks`, `maas`, `edge-native`, and `vsphere`; `gke` takes the
GCP entity, and `aks` the Azure entity.

The checks, in order:

1. The converter of the entity: a nil spec, the timezone, the cluster API
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

## POST /v1/spectroclusters/{uid}/validate/packs

This route checks a cluster before an update of its profiles. The body is a
`v1SpectroClusterPacksEntity`, the body of `validate/repave`: `profiles[]`,
each with a `uid` and the `packValues` to send. The route treats the profiles
of the body as all the profiles that the cluster will run. `compare --validate`
sends it.

The checks, in order:

1. The cluster exists, and the values of each pack of the body are YAML.
2. For each profile of the body, Palette builds the target: the copy that the
   cluster holds of that uid, or the published profile for a new uid, with the
   sent values over it.
3. The metadata of the packs: two dashboard packs that cannot go together, the
   names of the packs and the manifests, one pack or one manifest two times in
   a profile, one pack name in two profiles of one workload scope (the system
   scope or the app scope), and two versions of one pack across the profiles.
4. The integration packs: a pack that the cloud of the cluster does not
   support, and on an imported cluster each integration pack but Headlamp, is a
   refusal of the whole request, `PackSpectroIntegrationNotSupported`.
5. The integration packs again: the OIDC identity provider that an integration
   pack names is set in the Kubernetes pack or in the organization
   (`ClusterProfileOidcNotConfigured`), and a pack that an integration pack
   names as a read-only dependency keeps the default values of that pack
   (`PackValuesOverrideForbidden`). Then the pack validator, over the packs of
   each profile of the body that is not a system profile: no disabled pack
   (`DisabledPacksForbidden`), the values of each pack, and the dependencies of
   each pack.
   A dependency is checked over the packs of all the profiles of the body, so a
   pack that needs a newer version of a pack in another profile of the cluster
   fails here: `DependencyPackWithMinVersionRequired`,
   `Pack '<pack>' is required with minimum compatible version '<version>'`. On
   an imported cluster, only the values are checked.

The answer is `200` with `profiles[]`: only the profiles with an error, each
with its `uid` and `packs.results[].errors[]` of `code` and `message`. Palette
puts the error of a pack under each profile of the body that holds a pack of
that name, and gives the profiles in no fixed order. With no error, the answer
has no profile. `machinePools` stays empty: the route checks no machine. The
route does not say whether the change repaves: `validate/repave` answers that.

A refusal of the whole request, from the YAML check of step 1, from steps 3 and
4, and from step 5 before the values and the dependencies, is HTTP `500` with
`code` and `message`. Palette gives each new error the response type of a
server fault, and these errors set no other type. So a refusal by design
arrives with the status of a fault. The codes are in
[Refusals that strata does not send again](../../reference/errors.md#refusals-that-strata-does-not-send-again).
One refusal sets another type: `PackValuesOverrideForbidden` is HTTP `403`.
A cluster that does not exist is HTTP `404`.

No live run has asked this route yet. Stage 7 of `repave.sh` asks it on
cluster A: see [repave.sh](../../development/live-runs.md#repavesh).

## POST /v1/clusterprofiles/validate/packs

This route checks the packs of a profile before a create. The body is a
`v1ClusterProfileTemplateDraft`: `cloudType`, `type`, and `packs[]`, each a
`v1PackManifestEntity` with `name`, `tag`, `uid`, `registryUid`, `type`,
`layer`, `values`, and `manifests[]` of `name` and `content`. The route writes
nothing, and does not check the name or the version of the profile.
`snapshot --from-candidate --validate` sends it.

The checks, in order:

1. The body holds one pack or more. The converter of the route refuses an
   empty `packs` with HTTP 400 and the code `ShouldNotBeEmpty`, before the
   checks below. A live request shows it:
   `Parameter 'Packs' should not be empty`.
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
errors the response type of a server fault, so each one is HTTP 500. A live
run shows it: values that are not YAML get HTTP 500 with the code
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
3. Palette reads the clusters that run the version. A pack of the body whose
   name a pack of another profile of such a cluster also has, in the same
   workload scope, is a refusal: `ClusterProfilePacksInConflict`, with the
   names of the clusters.
4. The limits, the dashboard packs, and the YAML, as above.
5. The pack validator, as above. It also checks the dependencies over the packs
   that each such cluster runs.

The answer has the shape of the route above. A live run shows that the route
takes a published version of a cluster profile that no cluster runs, as an
update of itself. Stage 3 of `validate-packs.sh` asks it for a version that you
name.

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
runs them: hubble has no call to `SimulatePrincipalPolicy` or
`testIamPermissions`. `strata preflight` runs the tool of the cloud for some of
these, and prints `NOT CHECKED` with the command for the rest: see
[What strata preflight does not check](limits.md#what-strata-preflight-does-not-check).

## Where these answers come from

The source of hubble on master:
`services/service/spectrocluster/internal/service/spectrocluster/internal/base/spectrocluster_validate.go`
(the profiles and the machines), `internal/base/spectrocluster_profile.go` and
`internal/base/spectrocluster_base.go` in the same directory (`validate/packs`
of a cluster, and the shape of its answer),
`internal/base/spectrocluster_pack.go` in the same directory (`validate/packs`
of a profile version),
`services/service/spectrocluster/internal/service/clusterprofile/clusterprofile_service.go`
and `../pack/pack_validate.go` (the checks of the packs of a profile),
`handler/converter/v1/clusterprofiles_v1_converter.go` lines 265-283 and
`svcclient/validator/params_validator.go` lines 41-45 (an empty `packs`),
`base/herr/err_pack.go` (the codes),
`handler/converter/v1/clusters_v1_converter.go`
(the converter, and the GKE pool size), `aws/spectrocluster_aws_validate.go`
(the zones and the CIDRs of AWS), and
`services/service/cloud/internal/service/cloudservice/{aws,gcp}_service.go`
(the property and account routes). The findings report
`palette-precreate-validation-gaps-findings-2026-09-28` lists each check that
no route runs, and proposes the ones that fit a validate route to Spectro
Cloud.
