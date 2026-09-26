# What no run covers

Strata makes no claim about the behaviors in this section.

The digest of each registry kind that a run shows is in
[The digest of each registry kind](packs.md#the-digest-of-each-registry-kind). For the
Spectro pack registries of Spectro Cloud, every record of every run carries a
pack-ref digest, and no check asserts it.

The live runs used two clouds, AWS and GCP, and one upgrade engine: a kubeadm
cluster that Palette builds with the cluster API. On each cloud Palette gives
the same answers, and each repave answer agrees with the impact table (see
[A second cloud: GCP](../../development/live-runs.md#a-second-cloud-gcp)). The
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
- **The keys of a preset.** No key is required. Palette writes the values of the
  preset option that the values select into the values of the pack.
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
