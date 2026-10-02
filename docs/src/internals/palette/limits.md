# What no run covers

Strata makes no claim about the behaviors in this section.

The digest of each registry kind that a run shows is in
[The digest of each registry kind](packs.md#the-digest-of-each-registry-kind). For the
Spectro pack registries of Spectro Cloud, every record of every run carries a
pack-ref digest, and no check asserts it.

The live runs used three engines. The first is a kubeadm cluster that Palette
builds with the cluster API, on AWS and on GCP. On each cloud Palette gives the
same answers, and each repave answer agrees with the impact table (see
[A second cloud: GCP](../../development/live-runs.md#a-second-cloud-gcp)). The
second is EKS, a managed engine: a Kubernetes version bump repaves, and a change
of the values of the Kubernetes or CSI pack does not (see
[eks.sh](../../development/live-runs.md#ekssh)). The third is GKE, a managed
engine: a Kubernetes version bump repaves, and the four GKE packs have no values
to change (see [gke.sh](../../development/live-runs.md#gkesh)). No run uses AKS.
The
public page gives another rule for Edge: a change to the Kubernetes layer
repaves nothing, and on the OS layer only the `uri` repaves
([Upgrade behavior](https://docs.spectrocloud.com/clusters/edge/cluster-management/upgrade-behavior/)).
The repave check of Palette names more fields for Edge than that page does. No
run shows an Edge cluster, so the table of impact levels is not scoped by a
live result for it, and `compare --repave` is how a reader asks Palette.

These are open:

- An OCI registry with an auth other than ECR and basic, and a registry service
  other than ECR and the Docker registry: ACR, GHCR, Harbor, and JFrog
  Artifactory are named on the public page, and no run uses one.
- The variables of a cluster template while the template applies.

## What strata preflight does not check

`strata preflight` calls no cloud API. A rule that only the cloud can answer
runs the tool of the cloud (`aws`, `gcloud`) when it is on the machine, and is
`NOT CHECKED` with the command when it is not. These rows are `NOT CHECKED` on
every machine, because no check exists yet or the entity does not hold what the
check needs; the report prints each one with its reproduce lines:

- An instance type that a zone does not offer (`instance-offering`): the zone
  of a pool is in the entity, and the check needs one call for each pair of
  type and zone.
- The quotas of the cloud (`aws-quotas`, `eks-quotas`, `gcp-quotas`,
  `aws-sg-rules`): the usage is not in the entity.
- The image Palette builds for an OS and Kubernetes version on GCP
  (`gcp-image-not-shared`), and the AMI of an OS pack on AWS (`aws-ami`): the
  name is not in the entity.
- The routes that exist and that strata has no path for yet (`root-volume`,
  `dedicated-hosts`): the row names them.
- A race (`spot-capacity`, `profile-follows-push`, `edit-in-place`): no check
  before the build finds it; the watch finds its effect.
- A rule of the tool that makes the entity (`provider-timeout-success`,
  `provider-eks-azs`): out of scope; the row stays so that the catalog is
  complete.

`strata preflight --rules --cloud <engine>` lists each rule with its tier and
its check. A rule with a check whose tool is not on the machine is `NOT
CHECKED` on that machine alone. The findings report
`palette-precreate-validation-gaps-findings-2026-09-28` proposes the first
three to Spectro Cloud as validate routes.

## What the catalog does not hold yet

A read of the whole catalog against each section found these faults with no
row. Each one is a candidate rule; none has a run or a row of #91 yet, so none
is in the catalog.

| Section | The fault | Where a check would look |
| --- | --- | --- |
| identity | A `secret` AWS account with rotated keys, or a disabled GCP key | `aws sts get-caller-identity` under the account; `gcloud iam service-accounts keys list` |
| cloud services | The service-linked roles a first EKS create needs | `aws iam get-role --role-name AWSServiceRoleForAmazonEKS`, and the node group one |
| quotas | The cluster limit of the tenant or the project that the create path enforces | no route gives it |
| networking | The public addresses of Palette in the allowlist of a private endpoint; `enableDnsSupport` and `enableDnsHostnames` on a static VPC | `aws ec2 describe-vpc-attribute` |
| compute | The SSH key pair on EKS with a dynamic VPC | `aws ec2 describe-key-pairs`; the property route runs with a `vpcId` alone |
| kubernetes | An EKS version in extended support | the `status` of `aws eks describe-cluster-versions` |
| storage | A `kmsKeyId` in the parameters of a StorageClass | `aws kms describe-key` |
| images | The sync status of the registry of each pack, before a build | `GET /v1/registries/pack/{uid}/sync/status` |
| profiles | A version with no published template | `status.isPublished` of the profile |
| lifecycle | A pool renamed, which is a delete and a create; a `size` outside `minSize` and `maxSize` | a static diff of the change |
| platform | The version of the agent against the release of Palette | no route gives the release to a tenant key |
| tools | An AWS CLI older than 2.22, which `eks-version-not-offered` needs | `aws --version` |
| teardown | The IRSA roles a delete leaves | `aws iam list-roles`, by tag |

## What the source of Palette says

The Spectro Cloud source settles these. No live run checks them, so strata
makes no claim that depends on them.

- **The install order of the packs of two add-on profiles.** Palette sorts
  every add-on pack of a cluster by its install priority, over all its add-on
  profiles. The order of the profiles plays no part. Packs of one priority
  install together, which the start times of `template.sh` show, and a pack
  waits until each pack of a lower priority is ready.
- **An install priority other than `0`.** The priority is a signed number, from
  the annotation `spectrocloud.com/install-priority`. A lower number installs
  first, so a negative one installs before the default `0`. A delete goes in the
  other order.
- **A version bump with the `immutable-clusterprofiles` flag on and
  `skip_destroy` off.** The provider refuses the plan: a version change needs
  `skip_destroy = true`
  ([cluster_profile](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs/resources/cluster_profile)).
- **A pack version that Spectro Cloud disables or deletes after a profile pins
  it.** Palette then refuses an update or a publish of that profile. It refuses
  a new cluster from it only for a deleted pack: a disabled pack still builds
  clusters. A tag that the registry deletes marks the profile invalid.
- **The presets of a template option.** Palette writes them into the values of
  a pack on a create and on an update of a profile, and no route gives them:
  see [The presets of a template option](packs.md#the-presets-of-a-template-option).
- **A Helm registry of type `helm` whose endpoint is `oci://`.** Palette reads
  `index.yaml` from the endpoint over HTTP, so such a registry lists nothing. An
  `oci://` chart URL inside an index is read. An OCI Helm registry is its own
  kind: see [The digest of each registry kind](packs.md#the-digest-of-each-registry-kind).
- **The fields that repave, for each engine.** Palette keeps a list for the OS
  layer and the Kubernetes layer of each engine, and checks no other layer. A
  new pack version on either layer repaves on every engine. Strata does not copy
  the lists: they can change with each release of Palette, and
  `compare --repave` asks Palette for its answer for a cluster.

## Where these answers come from

The scripts under `examples/live/` build a real tenant, make each change, and
assert each answer above. [The live example](../../development/live-example.md)
says how to run them and what each one costs.
[Live runs](../../development/live-runs.md) lists the checks and what each one
establishes.
