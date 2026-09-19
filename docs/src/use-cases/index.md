# Use cases

Each page starts from a problem. It shows the commands, the real output of
strata, how to read the output, and what to do. When a page depends on a
behavior of Palette, it links to the live run that shows that behavior.

## The pages, in order

Each group answers a question that the group before it opens, so the pages read
from the top teach what the tool does. Each line also names the problem that
sends you to its page: read down the list to find yours.

**1. Compare two versions.** The everyday loop: what changed, and what it does
to a running cluster.

- [Review a change before you publish it](./review.md): a pull request changes a
  profile, and the reviewer needs its effect before anybody publishes.
- [What changed between two pack versions](./pack-versions.md): a bump moves a
  profile to a new pack version, and you must know what the pack changed.

**2. The same release under another name.** A comparison gives two roots. These
pages show why equal content can give different roots, and how strata says so.

- [One pack build under two tag names](./tag-name.md): two versions differ only
  in a tag, such as `24.04` and `LTS__24.x`.
- [One manifest in two profiles](./manifest-pack.md): two profiles with the same
  manifest have different roots.
- [A pack comes from another registry](./registry-move.md): a team pins a pack
  to another registry, and `same-release` fails without `--allow registry-move`.

**3. One name is not one release.** A name and a version number are not an
identity. The uid is, in the project that holds it.

- [Promote a release to another project](./promote.md): dev and prod have the
  same version name, and nothing yet says they have the same content.
- [The same names in two projects](./same-names.md): `cluster:payments` names
  two clusters, or `name@version` is not found.

**4. A version that changes after you record it.** This is why a record is
worth keeping, and what `verify` is for.

- [A published version is edited in place](./edited-in-place.md): `verify`
  reports `DRIFT` on a version that nobody bumped.
- [A version bump overwrites a version](./overwritten.md): a version
  disappeared from the list, and clusters still run it.

**5. What a cluster runs.** A release is what you publish. A run is what a
cluster has. The two can differ.

- [A cluster does not run its release](./cluster-release.md): a cluster must run
  a recorded release, and you need to know if it does.
- [Clusters give variables different values](./cluster-values.md): two clusters
  on one release show `cluster values CHANGED`, or a person changes a value on
  one cluster.

**6. Many profiles and many clusters.** The same questions, for a whole tenant
and a whole team.

- [Find the profiles that carry a pack](./find-a-pack.md): a pack version has a
  security problem.
- [Retire a version](./retire.md): you want to delete an old version.
- [A team pins each environment](./fan-out.md): a dozen developers pin each
  environment, and merge requests cross.
- [A publish fans out to what pins it](./publish-fan-out.md): a pack or profile
  publish opens a bump merge request on everything that pins it.

## The examples

The pages use one team and one profile, `aws-stack`, with real pack data from the
Public Repo registry. The tenant has two projects:

| Project | Holds |
| --- | --- |
| `dev` | the versions 1.0.0, 1.1.0, and 1.1.1 of `aws-stack`, and the clusters `payments`, `search`, and `ledger` |
| `prod` | its own copy of `aws-stack@1.1.1`, and its own cluster `payments` |

The examples keep a record at `releases/<version>/<project>.json`, and a cluster
record at `runs/<cluster>.json`. The names are a choice of the team, not of
strata.

Each page builds only the part of this tenant that it needs.
[Write the documentation](../development/documentation.md) tells how the book
runs each command.
