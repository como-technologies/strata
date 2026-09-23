# The pack of the live example

`strata-live-pack` is the one pack that `examples/live/registry.sh` pushes into
the example's own OCI pack registry: one ConfigMap, and no dependency. The
script pushes it as version `1.0.0`, pushes the same version again with other
bytes, and pushes `1.0.1`, to record what Palette does with a registry that
changes under a published profile. No shared registry sees any of that.

The layout is the one Palette reads: `pack.json`, `values.yaml`, and the
manifests that `pack.json` names. The script packs the directory with `tar` and
pushes the archive with `oras`, as
[the Palette documentation](https://docs.spectrocloud.com/registries-and-packs/registries/oci-registry/add-pack-oci/add-pack-oci-ecr/)
shows.
