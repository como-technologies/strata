# The validate routes

Palette has routes that check a cluster before a create. The console calls them
in its create wizard, and a create through the API goes to the create route.
This page states what each route checks, from the source of Palette (hubble on
master), and who calls it. `strata preflight` calls each one that takes the
body of a create.

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
(the profiles and the machines), `handler/converter/v1/clusters_v1_converter.go`
(the converter, and the GKE pool size), `aws/spectrocluster_aws_validate.go`
(the zones and the CIDRs of AWS), and
`services/service/cloud/internal/service/cloudservice/{aws,gcp}_service.go`
(the property and account routes). The findings report
`palette-precreate-validation-gaps-findings-2026-09-28` lists each check that
no route runs, and proposes the ones that fit a validate route to Spectro
Cloud.
