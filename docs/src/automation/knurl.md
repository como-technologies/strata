# The knurl adapter

[knurl](https://github.com/como-technologies/knurl) assembles pinned units, raises
them through channels, and converges running instances. It knows three roles:
leaf, intermediate, and root. It does not know what Palette is. Each external
system is behind a trait, and an adapter gives the trait.

`adapter/` is the adapter for Palette. It gives the read half of the platform
port of knurl, `PlatformRead`, and it reads Palette through the strata library.
Strata reads and never writes, so there is no write half here.

The port is `async`, and strata blocks. The adapter moves each read to a
blocking task of tokio, so the thread that waits on the future stays free. The
port of knurl names no runtime. A caller of this adapter runs the future on a
tokio runtime, as `adapter/examples/observe.rs` does.

The `strata` crate does not depend on knurl. `adapter/` is its own package, as
`examples/library/` is. `cargo build` and `cargo test` at the root do not build
it, and no release carries it.

## The roles

| knurl | Palette | strata ref |
| --- | --- | --- |
| leaf | a pack version in a registry | `pack:<name>@<version>` |
| intermediate | a cluster profile version | `<name>@<version>` |
| root | nothing: Palette holds no root unit | none |
| instance | a cluster | `cluster:<name>` |

An address in knurl holds the role. The adapter needs it: a pack and a profile
are in two namespaces, and they can have one name.

## What each method reads

| Method | Reads | Gives |
| --- | --- | --- |
| `observe_unit` | the record of a profile version, or of a pack version | the two digests, and each pack from a registry as a pin, in the order of the layers |
| `observe_instance` | the cluster, one time, its summary, and its notifications | the state, the health from the summary, the two digests of each profile copy that the cluster holds, by name, and whether Palette says an update waits |
| `pinned_by` | `consumers` | the clusters that use a profile version: those the profile lists, and those the second reading of `consumers` finds. Each one by name, or by uid where Palette gives no name |
| `identity_of` | a candidate | the content digest before the version is in Palette |
| `verify`, from knurl | `observe_unit` again | unchanged, or what changed |

The exact digest of knurl is the root. The content digest of knurl is
[the content digest](../internals/hashes.md#the-content-digest). For a pack it is
the digest of the pack in its registry. The adapter gives each one the written
form of knurl, `sha256:` and 64 hex digits, and it does not encode it again. So
the digest that strata prints is the digest that knurl pins.

One case differs. For a leaf that several registries hold, the exact digest
covers the registry and the root of each copy. It is a digest of knurl over
those values, and not one root. It changes when a registry receives a copy of
the leaf, and when a registry loses one.

A failure keeps the class that `Error::class` gives it, and the strata error is
its cause.

## The definition of a unit

knurl gives a defined unit its pins, as name and version in order, and its own
parameters, as named text. The adapter reads one parameter, `candidate`: a
[candidate file](../reference/candidate-file.md), as JSON. It holds what the pins
do not: the tag and the registry of each layer, the values, the manifests, and
the variables. The pins give the order of the layers. That
[order is content](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster),
and knurl keeps it in the pin set.

`strata_knurl::definition_of` writes that definition from a published version.
`identity_of` reads it back: it sets the name and the version from the
definition, puts the layers in the order of the pins, each at the pinned
version, gives the candidate to strata in memory, and writes no file. A layer
with no pin, a pin with no layer, a definition with no pins, and a definition
with no `candidate` parameter are each a terminal failure.

## What the adapter cannot answer

- **Health, where Palette serves no cluster summary.**
  [The cluster summary](../internals/palette/clusters.md#what-the-cluster-listing-answers)
  carries the health of a cluster, as one of two states with its conditions. The
  adapter maps the two states, and it names the first condition of an unhealthy
  cluster. Where Palette serves no summary, the adapter says `Unknown`. Any
  other failure of the summary is a failure of the read.
- **Whether an update waits, where Palette serves no notifications.** The
  adapter gives `pending` as a cluster record reads it:
  [Whether an update waits](../commands/snapshot.md#whether-an-update-waits).
  `Waiting` names each profile that has an open update, and `Clear` says that
  none waits. Where Palette does not serve the notifications, the adapter says
  `Unknown`, and the read still succeeds.
- **A leaf with no digest.** Where Palette lists no digest for a pack, the
  adapter refuses the leaf: nothing pins it by content.
  [What no run covers](../internals/palette/limits.md#what-no-run-covers) names the
  registries that can list none.
- **The order of the profiles of a cluster.**
  [Palette lists them in an order that says nothing](../internals/palette/clusters.md#the-order-of-the-profiles-of-a-cluster).
  The adapter gives the copies by name.
- **A root.** Palette holds no root unit. The adapter gives a terminal failure.
- **Who uses a pack.** Palette names the clusters of a profile version alone,
  and a leaf or a root is a terminal failure. `pinned_by` does not trust that
  list alone: it reads each cluster the key can list as well, as
  [`consumers`](../internals/palette/clusters.md#what-the-cluster-listing-answers) does.
- **Which of two archives, with no digest.** An address has no registry, and a
  tenant can have a pack version in two of them. The adapter reads each one.
  When the digests are equal, it is one leaf: a raise copies a pack by digest
  within one kind of registry. Two kinds of registry can also hold two archives
  under one name and version. An address can hold the content digest, and then
  the adapter gives the archive with that digest: a pin holds it, so what pins
  the leaf can say which one it means. A digest that no archive has is a
  terminal failure that names each archive. Where one registry holds the pack,
  the digest of the address refuses nothing. With no digest and two archives,
  the adapter gives a terminal failure that names each registry and its
  archive.
- **A publish.** Strata does not write.

## Run it by hand

One command runs the two sides against the playground. It needs no tenant and no
key:

```bash
just adapter-by-hand                 # a profile version, its cluster, and who uses it
just adapter-by-hand candidate       # the identity of a definition, and a leaf
```

It serves one chapter of the playground on this machine, runs the `strata`
command and then the platform port of knurl, and stops the server. Each line is
a command, its answer, and its exit code.

Read the two sides together. On the strata side, `root` and `contentDigest` are
bare hex. On the knurl side, `exact` and `content` are the same two values with
the prefix `sha256:`. The clusters that `consumers` names are the instances that
`pinned-by` names.

The second chapter has an intentional failure. That tenant has `csi-aws-ebs`
1.65.0 in two kinds of registry, with two archives, so an address with no
digest cannot say which one it means. The chapter then asks again with the
digest of one archive, and gets that one.

### One command at a time

The knurl side is `adapter/examples/observe.rs`. It is not a command on the
`PATH`, and it is not in the repository of knurl: `just adapter-observe` builds it
and runs it. It reads the settings that the `strata` command reads, so it runs
against a tenant too. It reads, and it never writes.

```bash
just playground cluster-release        # terminal 1: it prints four export lines, and waits
```

```bash
# terminal 2, in this repository
export PATH="$PWD/target/release:$PATH"   # the strata that this checkout built
# paste the four export lines of terminal 1 here

strata snapshot aws-stack@1.1.0 -o /tmp/record.json
jq -c '{root, contentDigest}' /tmp/record.json
just adapter-observe unit intermediate aws-stack 1.1.0
just adapter-observe instance payments
just adapter-observe pinned-by aws-stack 1.1.0
just adapter-observe verify aws-stack 1.1.0
```

A chapter of the playground serves what its page needs, and no more. Two
commands need a chapter that serves the registries:

| Command | Chapter | Why |
| --- | --- | --- |
| `unit intermediate`, `instance`, `pinned-by`, `verify` | `cluster-release` | It has the clusters. It serves no registry. |
| `identity aws-stack 1.1.1`, `unit leaf cert-manager 1.21.1` | `candidate` | It serves the registries. It has no cluster. |

## Run it against a tenant

```bash
just live-profiles     # the live profile versions: seconds, and no cost
just live-knurl        # each method of the port, against the strata answer for the same object
just live-up           # the cluster too, for the instance checks: billable until live-down
```

`examples/live/knurl.sh` runs each method against the real profile versions,
the real registries, and each cluster that runs a version. It then runs the
strata command for the same object, and it compares the two answers. The exact
digest is the root, the content digest is `contentDigest`, the content of a leaf
is the digest of the pack in its registry, and `pinned_by` is `consumers`. The
two-archive failure runs on a real pack that two registries of the tenant hold.
[What the live example verifies](../development/live-runs.md#knurlsh) lists
its checks.

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
