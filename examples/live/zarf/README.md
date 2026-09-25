# strata-live-zarf

A Zarf package with one manifest and no image, for `registry-kinds.sh` (#75). The script
builds it with `zarf package create`, publishes it into the OCI registry of the example's
registry host, and publishes the same version again with other bytes.
