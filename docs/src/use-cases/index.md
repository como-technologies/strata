# Use cases

Each page starts from a problem. It shows the commands, the real output of
strata, how to read the output, and what to do. When a page depends on a
behavior of Palette, it links to the live run that shows that behavior.

## The words these pages use

The pages are written to be read in order, and each one uses the words the
pages before it taught. These six come first, because the output blocks print
them from the first page.

**A pack** is one layer of a profile: an OS, Kubernetes, a CNI, a CSI, or an
add-on. Each pack has a name, a version, a digest, and values. The reports
write the layer in brackets, as `[os   ]`, `[k8s  ]`, `[cni  ]`, `[csi  ]`, and
`[addon]`.

**A profile** is a stack of those layers, published under a name and a version,
such as `aws-stack@1.1.1`. A **cluster profile** carries the layers that build
a cluster. An **add-on profile** carries only add-on layers, and a cluster can
run one cluster profile and several add-on profiles at once.
`platform-addons@1.0.0` in these pages is an add-on profile.

The layers of a cluster profile come from its environment, not from strata. On
AWS and GCP, as in these pages, it has four: an OS, Kubernetes, a CNI, and a
CSI. An Edge profile needs no CSI
([Model edge cluster profile](https://docs.spectrocloud.com/clusters/edge/site-deployment/model-profile/)),
a Nutanix profile leaves the OS and Kubernetes layers empty
([Nutanix](https://docs.spectrocloud.com/clusters/data-center/nutanix/)), and
an imported cluster runs add-on profiles alone
([Attach an add-on profile](https://docs.spectrocloud.com/clusters/imported-clusters/attach-add-on-profile/)).
Strata records the layers that a profile has, and a layer that is not there
gives no impact.

**The root** is one digest over the whole content of a profile version: every
pack, its build, its values, and its manifests. Two versions with the same root
are the same release, whatever their names, their uids, or the project that
holds them. The root is what every comparison on these pages compares.
[Hashes](../internals/hashes.md) gives the rule for each field.

**A record** is a file strata writes with `strata snapshot`, holding the content
of a profile version or of a cluster, and its root, at the moment it read them.
The record is what makes a later question possible: `strata verify` asks Palette
again and compares it with the record. These pages keep a release record at
`releases/<version>/<project>.json` and a cluster record at
`runs/<cluster>.json`. The names are a choice of the team, not of strata.

**A change flag** on a pack line says what moved. `IDENTITY` says the layer
points at another pack build, `UPSTREAM` that the content digest moved, and
`AUTHORED` that the values changed. **An impact** says what the change does to a
running node, from `none` up to `repave`, which replaces every node.
[Impact](../reference/impact.md) gives each flag and each level.

**The playground** serves the tenant of one page on your machine, with no key
and no cost: `just playground <name>`. A page whose steps change the tenant has
more than one **seed**, and a seed is one state of that tenant, named
`<page>-1`, `<page>-2`, and so on. Each page says which seed each of its
sections needs, and each seed serves on its own port.

## The pages, in order

Each group answers a question that the group before it opens, so the pages read
from the top teach what the tool does. Each line also names the problem that
sends you to its page: read down the list to find yours.

**1. Compare two versions.** The everyday loop: what changed, and what it does
to a running cluster.

- [What changed between two pack versions](./pack-versions.md): a bump moves a
  profile to a new pack version, and you must know what the pack changed.
- [Review a change before you publish it](./review.md): a pull request changes a
  profile, and the reviewer needs its effect before anybody publishes.

**2. Which objects a name means.** A name and a version number are not an
identity. The uid is, in the project that holds it.

- [The same names in two projects](./same-names.md): `cluster:payments` names
  two clusters, or `name@version` is not found.
- [Find the profiles that carry a pack](./find-a-pack.md): a pack version has a
  security problem, and you need every profile and cluster that carries it.

**3. What a cluster runs.** A release is what you publish. A run is what a
cluster has. The two can differ.

- [A cluster does not run its release](./cluster-release.md): a cluster must run
  a recorded release, and you need to know if it does.
- [Clusters give variables different values](./cluster-values.md): two clusters
  on one release show `cluster values CHANGED`, or a person changes a value on
  one cluster.
- [Two clusters on one release differ](./cluster-tiers.md): two clusters run one
  release, and differ in their node pools, their policy, their labels, or the
  pack version that each one reconciled.
- [A profile runs on edge hosts](./edge-hosts.md): a version is bound to edge
  hosts, and a delete must count them, or an edit reached them.
- [A cluster group pins a profile](./cluster-groups.md): a cluster group pins a
  profile for its virtual clusters, and you must know that each one runs it.

**4. The same release under another name.** These pages show why equal content
can give different roots, and how strata says so.

- [One pack build under two tag names](./tag-name.md): two versions differ only
  in a tag, such as `24.04` and `LTS__24.x`.
- [One manifest in two profiles](./manifest-pack.md): two profiles with the same
  manifest have different roots.
- [A pack comes from another registry](./registry-move.md): a team pins a pack
  to another registry, and `same-release` fails without `--allow registry-move`.
- [A pack has no digest](./no-digest.md): `verify` reports `UNVERIFIED`, or
  `same-release` reports `NO DIGEST`.

**5. Move a release, and end its life.** What the groups above make possible.

- [Promote a release to another project](./promote.md): dev and prod have the
  same version name, and nothing yet says they have the same content.
- [Retire a version](./retire.md): you want to delete an old version.

**6. A version that changes after you record it.** These four are what a team
should not do, and what to do when somebody already did. This is why a record
is worth keeping, and what `verify` is for.

- [A published version is edited in place](./edited-in-place.md): `verify`
  reports `DRIFT` on a version that nobody bumped.
- [A version bump overwrites a version](./overwritten.md): a version
  disappeared from the list, and clusters still run it.
- [A macro value changes](./macros.md): a profile reads a macro, and the value
  changed with no change to any record.
- [An app profile is updated in place](./app-profiles.md): an app profile has no
  publish step, and `verify` reports `DRIFT` on a version that nobody bumped.

**7. Many profiles and many clusters.** The same questions, for a whole tenant
and a whole team.

- [A team pins each environment](./fan-out.md): a dozen developers pin each
  environment, and pull requests cross.
- [A publish fans out to what pins it](./publish-fan-out.md): a pack or profile
  publish opens a bump pull request on everything that pins it.

## The examples

The pages use one team and one profile, `aws-stack`, with real pack data from the
Public Repo registry. The tenant has two projects:

| Project | Holds |
| --- | --- |
| `dev` | the versions 1.0.0, 1.1.0, and 1.1.1 of `aws-stack`, and the clusters `payments`, `search`, and `ledger` |
| `prod` | its own copy of `aws-stack@1.1.1`, and its own cluster `payments` |

Each page builds only the part of this tenant that it needs.
[Write the documentation](../development/documentation.md) tells how the book
runs each command.
