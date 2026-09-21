# The knurl adapter

[knurl](https://github.com/como-technologies/knurl) assembles pinned units, raises
them through channels, and converges running instances. It knows three roles:
leaf, intermediate, and root. It does not know what Palette is. Each external
system is behind a trait, and an adapter gives the trait.

`adapter/` is the adapter for Palette. It gives the read half of the platform
port of knurl, `PlatformRead`, and it reads Palette through the strata library.
Strata reads and never writes, so there is no write half here.

The `strata` crate does not depend on knurl. `adapter/` is its own package, as
`examples/library/` is. `cargo build` and `cargo test` at the root do not build
it, and no release carries it.

## The roles

| knurl | Palette | strata ref |
| --- | --- | --- |
| leaf | a pack version in a registry | `pack:<name>@<version>` |
| intermediate | a cluster profile version | `<name>@<version>` |
| root | nothing: a root is in the forge | none |
| instance | a cluster | `cluster:<name>` |

An address in knurl holds the role. The adapter needs it: a pack and a profile
are in two namespaces, and they can have one name.

## What each method reads

| Method | Reads | Gives |
| --- | --- | --- |
| `observe_unit` | the record of a profile version, or of a pack version | the two digests, and each pack from a registry as a pin |
| `observe_instance` | the cluster, one time | the state, and the two digests of each profile copy that the cluster holds |
| `pinned_by` | `consumers` | the clusters that use a profile version |
| `identity_of` | a candidate | the content digest before the version is in Palette |
| `verify`, from knurl | `observe_unit` again | unchanged, or what changed |

The exact digest of knurl is the root. The content digest of knurl is
[the content digest](../internals/hashes.md#the-content-digest). For a pack it is
the digest of the pack in its registry. The adapter gives each one the written
form of knurl, `sha256:` and 64 hex digits, and it does not encode it again. So
the digest that strata prints is the digest that knurl pins.

A failure keeps the class that `Error::class` gives it, and the strata error is
its cause.

## The definition of a unit

knurl gives a defined unit its pins, as name and version, and its own
parameters, as named text. The adapter reads one parameter, `candidate`: a
[candidate file](../reference/candidate-file.md), as JSON. It holds what the pins
do not: the order of the layers, the tag and the registry of each one, the
values, the manifests, and the variables. The order is there because Palette
installs the layers in order, so the order is content.

`strata_knurl::definition_of` writes that definition from a published version.

## What the adapter cannot answer

- **Health.** Strata reads the state of a cluster and no health. The adapter
  says `Unknown`.
- **A root.** Palette holds no root unit. The adapter gives a terminal failure.
- **Who uses a pack.** Palette names the clusters of a profile version alone.
- **A pack in two registries.** An address has no registry. When two registries
  hold the pack version, strata gives `pack-in-several-registries`, and the
  adapter gives a terminal failure. A raise copies a pack by digest, so the
  answer is the same in each registry, and the adapter must learn to say so.
- **A publish.** Strata does not write.

## Test it

knurl writes the rules of its port one time, as a suite. Its in-memory fake runs
the suite, and this adapter runs the same code. The tenant is in memory. It comes
from the registry answers that this repository tracks and from the profiles and
the cluster that `just examples` builds, so the digests are those of a real
registry.

```bash
just adapter-test
```

The recipe needs git access to knurl, which is a private repository. To test
against a checkout of knurl on this machine, give Cargo the two paths:

```bash
K=$HOME/repos/knurl
CARGO_NET_GIT_FETCH_WITH_CLI=true cargo test --manifest-path adapter/Cargo.toml --target-dir target/adapter \
  --config "patch.'ssh://git@github.com/como-technologies/knurl.git'.knurl-core.path='$K/knurl-core'" \
  --config "patch.'ssh://git@github.com/como-technologies/knurl.git'.knurl-ports.path='$K/knurl-ports'"
```
