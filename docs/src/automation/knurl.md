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
- **Which of two archives.** An address has no registry, and a tenant can have
  a pack version in two of them. The adapter reads each one. When the digests
  are equal, it is one leaf: a raise copies a pack by digest. Two kinds of
  registry can also hold two archives under one name and version. Then the
  adapter gives a terminal failure that names each registry and its digest,
  because knurl cannot say which one it means.
- **A publish.** Strata does not write.

## Run it by hand

`adapter/examples/observe.rs` calls the port by hand, over HTTP. It reads the
settings that the `strata` command reads, so it runs against a tenant or against
the playground. It reads, and it never writes.

```bash
just playground cluster-release        # in one terminal: it prints four export lines
```

```bash
# in another terminal, after the four export lines
just adapter-observe unit intermediate aws-stack 1.1.0
just adapter-observe instance payments
just adapter-observe pinned-by aws-stack 1.1.0
just adapter-observe verify aws-stack 1.1.0
```

Put each answer next to the answer of strata. `exact` is `root`, and `content` is
`contentDigest`, each with the prefix `sha256:`:

```bash
strata snapshot aws-stack@1.1.0 -o record.json
jq '{root, contentDigest}' record.json
strata consumers aws-stack@1.1.0
```

A chapter of the playground serves what its page needs, and no more. Two
commands need a chapter that serves the registries:

| Command | Chapter | Why |
| --- | --- | --- |
| `unit intermediate`, `instance`, `pinned-by`, `verify` | `cluster-release` | It has the clusters. It serves no registry. |
| `identity aws-stack 1.1.1`, `unit leaf cert-manager 1.21.1` | `candidate` | It serves the registries. It has no cluster. |

In the chapter `candidate`, `unit leaf csi-aws-ebs 1.65.0` is the failure for two
archives: that tenant has the version in two kinds of registry.

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
