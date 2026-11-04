# What no run covers

Strata makes no claim about the behaviors in this section.

The digest of each registry kind that a run shows is in
[The digest of each registry kind](packs.md#the-digest-of-each-registry-kind). For the
Spectro pack registries of Spectro Cloud, every record of every run carries a
pack-ref digest, and no check asserts it.

The live runs used three engines. The first is a kubeadm cluster that Palette
builds with the cluster API, on AWS and on GCP. On each cloud Palette gives the
same answers (see
[A second cloud: GCP](../../development/live-runs.md#a-second-cloud-gcp)). Each
repave answer agrees with the impact table. A live run gave the rule for a move
of the OS pack to another registry at one version. See
[A pack that moves between registries](packs.md#a-pack-that-moves-between-registries). The
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
- A Palette that runs in FIPS mode: no run uses one. See
  [The FIPS rule](packs.md#the-fips-rule).
- A bring-your-own-image OS pack, and a new `imageId` under one version: no run
  gives an OS pack an `imageId`. So no run shows if Palette replaces the nodes
  for a new image, or which `imageId` a cluster copy holds. See
  [The OS pack names a new machine image](../../use-cases/os-image.md).
- Two cases of a layer that a person removes from the cluster. No run shows what
  Palette does with an object that an earlier edit dropped from the manifest, or
  with a namespace that the layer owns. A run shows only a Deployment that the
  content names. See [An object that an edit drops from a manifest](clusters.md#an-object-that-an-edit-drops-from-a-manifest).
- A change that a person makes with `kubectl` to an object that a manifest layer
  applied, and a delete of such an object. No run shows what Palette does. See
  [What the source of Palette says](#what-the-source-of-palette-says).
- A key with a role smaller than the role of the example. No run has used a key
  with Project Viewer, Cluster Viewer, or a custom role. See
  [The role that a key needs](clusters.md#the-role-that-a-key-needs).
- A change of the endpoint or the values of a cluster group: the run of
  [cluster-group.sh](../../development/live-runs.md#cluster-groupsh) changes
  the CPU limit alone. See
  [The config that a cluster group gives](../../commands/snapshot/other-objects.md#the-config-that-a-cluster-group-gives).
- A cluster create from a version with an invalid pack: no run shows what
  Palette answers. See
  [A tag that a registry deletes](packs.md#a-tag-that-a-registry-deletes).
- The answer of `POST /v1/clusterprofiles/validate/packs` to a pack that
  requires a pack that the profile does not hold. No pack of the Public Repo
  lists a dependency. See
  [The validate routes](validate-routes.md#post-v1clusterprofilesvalidatepacks).
- The answer of `POST /v1/clusterprofiles/import/validate` to a name and a
  version that the project holds. A run shows the refusal of the name route
  alone, because strata prints one refusal when two routes give the same one.
  See
  [The validate routes](validate-routes.md#post-v1clusterprofilesimportvalidate).
- An HTTP 403 on `config/rbacs`, `config/namespaces`, or `repave/status`. The
  stand-in gives the body `AccessDenied`, and no run shows that body yet. A key
  of an active user with no role in the project gets HTTP 500 and not 403: see
  [Cluster tiers](clusters.md#cluster-tiers). Stage 6 of
  [tiers.sh](../../development/live-runs.md#tierssh) reads `config/rbacs` with
  such a key. Palette answers HTTP 500 with the code `ResourceAccessDenied` and
  the message `Project '<project uid>' access denied`. `strata snapshot` of the
  cluster writes no record and exits `2`. See
  [The policy tier](../../commands/snapshot/cluster.md#the-policy-tier).
- An imported cluster: no run imports one. So no run shows the `clusterType`
  of an import in the listing, or a read-only import with no add-on profile.
  The stand-in of the book models them. See
  [An imported cluster](clusters.md#an-imported-cluster).
- A move of a cluster to a version that reads a profile variable in a field
  that repaves. The body of `compare --repave` gets no repave, and a body with
  the `packValues` of the version gets a repave. No run applies such a version
  to a cluster. See
  [A variable in a field that repaves](profiles.md#a-variable-in-a-field-that-repaves).
- The GCP answer of the instance type route. No run shows it, so the stand-in
  of the book models it. See
  [The list routes](validate-routes.md#the-list-routes).

## What strata preflight does not check

`strata preflight` calls no cloud API. A rule that only the cloud can answer
runs the tool of the cloud (`aws`, `gcloud`) when it is on the machine, and is
`NOT CHECKED` with the command when it is not. These rows are `NOT CHECKED` on
every machine, because no check exists yet or the entity does not hold what the
check needs; the report prints each one with its reproduce lines:

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
| networking | The public addresses of Palette in the allowlist of a private endpoint; `enableDnsSupport` and `enableDnsHostnames` on a static VPC | `aws ec2 describe-vpc-attribute` |
| kubernetes | An EKS version in extended support | the `status` of `aws eks describe-cluster-versions` |
| storage | A `kmsKeyId` in the parameters of a StorageClass | `aws kms describe-key` |
| images | The sync status of the registry of each pack, before a build | `status.sync` of `GET /v1/registries/pack/summary` |
| profiles | A version with no published template | `status.isPublished` of the profile |
| lifecycle | A pool renamed, which is a delete and a create; a `size` outside `minSize` and `maxSize` | a static diff of the change |
| platform | The version of the agent against the release of Palette | the one route that gives the release refuses a tenant key: see [The release of Palette](#the-release-of-palette) |
| tools | An AWS CLI older than 2.22, which `eks-version-not-offered` needs | `aws --version` |
| teardown | The IRSA roles a delete leaves | `aws iam list-roles`, by tag |

## The release of Palette

One route gives the release of Palette: `GET /v1/mgmt/app/version`. The public
API reference does not list it. A call with no `ApiKey` header answers 200. The
body holds the exact release in `version.installedVersion`, for example
`4.10.17-patch.1`. A call with any `ApiKey` header answers 500. So a tenant key
cannot read the release.

Strata does not call this route. Every other call of strata sends the key, and
this route refuses a call with the key. `--release` stays the input of the
release: see [the options of `strata preflight`](../../commands/preflight.md#options).
The run of [release.sh](../../development/live-runs.md#releasesh) makes both
calls:

```bash
examples/live/api.sh app-version --no-key   # 200, with the release
examples/live/api.sh app-version            # HTTP 500
```

## What the source of Palette says

The code of Palette gives these rules. No live run checks them, so strata
makes no claim that depends on them.

- **An object that a person changes in a workload cluster.** Palette applies each
  object of a manifest layer again at each reconcile. So it creates an object again
  after a delete. It can also restore the value that the manifest gives to a field.
  Its API has no route and no field that compares the objects of the cluster with the
  profile. The code of the apply reports no difference. The period of the reconcile
  belongs to the deployment of Palette. So the observer does not see such an edit, and
  no run shows that Palette reports it.
- **The install order of the packs of two add-on profiles.** Palette sorts
  every add-on pack of a cluster by its install priority, over all its add-on
  profiles. The order of the profiles plays no part. Packs of one priority
  install together, which the start times of `template.sh` show, and a pack
  waits until each pack of a lower priority is ready.
- **An install priority other than `0`.** The priority is a signed number, from
  the annotation `spectrocloud.com/install-priority`. A lower number installs
  first, so a negative one installs before the default `0`. A delete goes in the
  other order.
- **The tiers of an app profile with no install order, and the order of the
  tiers route.** Palette does not give `installOrder` for a tier at 0, and does
  not list the tiers in install order. Strata sorts the tiers by the order
  itself, and a tier with no order is at 0. A live run shows that Palette
  keeps the order of the create: see
  [App profiles](other-objects.md#app-profiles).
- **A version bump with the `immutable-clusterprofiles` flag on and
  `skip_destroy` off.** The provider refuses the plan: a version change needs
  `skip_destroy = true`
  ([cluster_profile](https://registry.terraform.io/providers/spectrocloud/spectrocloud/latest/docs/resources/cluster_profile)).
- **A pack version that Spectro Cloud disables or deletes after a profile pins
  it.** Palette then refuses an update or a publish of that profile. It refuses
  a new cluster from it only for a deleted pack: a disabled pack still builds
  clusters. A tag that the registry deletes marks the profile invalid.
- **The presets of a template option.** Palette writes them into the values of
  a pack on a create and on an update of a profile. No route gives them: see [The presets of a template option](packs.md#the-presets-of-a-template-option).
- **A Helm registry of type `helm` whose endpoint is `oci://`.** Palette reads
  `index.yaml` from the endpoint over HTTP, so such a registry lists nothing. An
  `oci://` chart URL inside an index is read. An OCI Helm registry is its own
  kind: see [The digest of each registry kind](packs.md#the-digest-of-each-registry-kind).
- **The mask of a published pack and of a cluster copy.** Palette writes
  `********` over each value of the format `password` when it serves a
  published version, and when it serves the copies of the profiles of a
  cluster. Strata does not depend on the schema for this. A record names each
  value that is the text `********`. See
  [A secret changes in a pack value](../../use-cases/masked-value.md).
- **The mask of a draft.** Palette writes `********` over each value of the
  format `password` in the published packs only. It serves the packs of a
  draft in clear. Strata writes the mask over the draft itself: see
  [Secrets in the values](profiles.md#secrets-in-the-values). When Palette
  masks the draft too, the values hold `********` already, and the mask of
  strata changes nothing.
- **The variables in the pack values of a cluster.** Palette writes the value
  of a variable into the pack values of a cluster on one condition. The request
  sets `resolvePackValues` on `GET /v1/spectroclusters/{uid}`, or
  `resolveMacros` on `GET /v1/spectroclusters/{uid}/profiles/packs/manifests`,
  to true. Both are false by default, and strata sends neither. So the pack
  values of a cluster record hold `{{.spectro.var.NAME}}`, and not the value.
  Only `variables[]` of the record holds the value of a variable.
- **The fields that repave, for each engine.** Palette keeps a list for the OS
  layer and the Kubernetes layer of each engine, and checks no other layer. A
  new pack version on either layer repaves on every engine. Strata does not copy
  the lists: they can change with each release of Palette, and
  `compare --repave` asks Palette for its answer for a cluster.
- **The values in a notification event.** An event of the values of a pack
  holds the full values in `packValues`. It holds the names of the password
  fields in `passwordFields`. Palette serves the event with no mask, and masks the values
  only in its own log. So `--raw-dir` masks the values before it saves the
  notifications: see
  [The saved answers of Palette](../../commands/snapshot/no-network.md#the-saved-answers-of-palette).
- **The zones that do not offer an instance type.** A type that the list does
  not mark can still fail in the cloud. See
  [The list routes](validate-routes.md#the-list-routes).
- **The default filter of the GCP and Azure instance type routes.** They list only the types with
  2 vCPUs or more and 4 GiB of memory or more, when the query does not say. Strata sends
  `cpuGtEq=0&memoryGtEq=0` to each route.
- **An AWS account with the annotation `freeCredit` set to `true`.** The AWS instance type route
  lists only the types with up to 4 vCPUs and 8 GiB of memory for such an account.
- **The key pairs of an account.** The property route of EKS checks the key pair
  only with a `vpcId`, so `ssh-key-pair` reads the list for EKS. See
  [The list routes](validate-routes.md#the-list-routes).
- **A region that AWS refuses for the VPC route.** The regions route lists only the regions that
  answer a dry run of an EC2 call. The dry run uses the key of the account. A region that is off
  is not in the list, and the order of the list can change from one request to the next. AWS can
  still refuse the read of the VPCs of a listed region. The VPC route then answers HTTP 500 with
  the code `AwsVpcsReadError`, and not a 4xx. See
  [The list routes](validate-routes.md#the-list-routes).
- **A region with no metadata of its instance types.** The instance type route answers HTTP 500 with
  the code `CloudInstanceMetadataNotFound`. The rule `instance-offering` then reports the code.
- **The cluster limit of the tenant.** A trial tenant has a limit, and
  `palette-validate` reports the refusal: see
  [The checks of the route](validate-routes.md#post-v1spectroclusterscloudtypevalidate).

## Where these answers come from

The scripts under `examples/live/` build a real tenant, make each change, and
assert each answer above. [The live example](../../development/live-example.md)
says how to run them and what each one costs.
[Live runs](../../development/live-runs.md) lists the checks and what each one
establishes.
