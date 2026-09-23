# The chart of the live example

`strata-live-chart` is the one Helm chart that `examples/live/registry.sh`
pushes into the example's own OCI Helm registry: one ConfigMap. The script
pushes it as version `1.0.0` and pushes the same version again with other
bytes, to record what Palette lists for a chart in an OCI registry, and whether
that follows a push. No shared registry sees any of it.

The script packages the directory with `helm package` and pushes the archive
with `helm push`, as
[the Palette documentation](https://docs.spectrocloud.com/registries-and-packs/registries/oci-registry/add-oci-helm/)
shows.
