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
public pages give other rules for the other engines. On
an Edge cluster a change to the Kubernetes layer repaves nothing, and on the OS
layer only the `uri` repaves; a managed cluster takes a rolling upgrade for any
override of a node pool
([Node pools](https://docs.spectrocloud.com/clusters/cluster-management/node-pool/),
[Upgrade behavior](https://docs.spectrocloud.com/clusters/edge/cluster-management/upgrade-behavior/)).
No run shows any of this, so the table of impact levels is not scoped by a
live result, and `compare --repave` is how a reader asks Palette.

These are open:

- A Helm registry of type `helm` whose endpoint is `oci://`.
- An OCI registry with an auth other than ECR and basic, and a registry service
  other than ECR and the Docker registry: ACR, GHCR, Harbor, and JFrog
  Artifactory are named on the public page, and no run uses one.
- The keys that the values of a profile must carry for the preset option that
  they select.
- The order in which a cluster installs the packs of two add-on profiles.
- An install priority other than `0` on an add-on pack.
- A version bump with the `immutable-clusterprofiles` flag on and `skip_destroy`
  off.
- The variables of a cluster template while the template applies.
- A published profile, or a cluster, that pins a pack version when Spectro Cloud
  later disables or deletes it. Palette refuses a new profile that pins one, so
  the example cannot make this case.

## Where these answers come from

The scripts under `examples/live/` build a real tenant, make each change, and
assert each answer above. [The live example](../../development/live-example.md)
says how to run them and what each one costs.
[Live runs](../../development/live-runs.md) lists the checks and what each one
establishes.
