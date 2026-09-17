# Use cases

Each page starts from a problem. It shows the commands, the real output of
strata, how to read the output, and what to do. When a page depends on a
behavior of Palette, it links to the live run that shows that behavior.

## Find your problem

| You see or want | Page |
| --- | --- |
| A pull request changes a profile, and the reviewer needs its effect | [Review a change before you publish it](./review.md) |
| Dev and prod have the same version name, but you do not know if they have the same content | [Promote a release to another project](./promote.md) |
| A pack moved to another registry, and `same-release` fails | [A pack comes from another registry](./registry-move.md) |
| Two versions differ only in a tag such as `24.04` and `LTS__24.x` | [One pack build under two tag names](./tag-name.md) |
| Two profiles with the same manifest have different roots | [One manifest in two profiles](./manifest-pack.md) |
| `verify` reports `DRIFT` on a version that nobody bumped | [A published version is edited in place](./edited-in-place.md) |
| A version disappeared from the list, but clusters still run it | [A version bump overwrites a version](./overwritten.md) |
| A cluster must run a recorded release | [A cluster does not run its release](./cluster-release.md) |
| Two clusters on one release show `cluster values CHANGED` | [Clusters give variables different values](./cluster-values.md) |
| `cluster:payments` names two clusters, or `name@version` is not found | [The same names in two projects](./same-names.md) |
| A pack version has a security problem | [Find the profiles that carry a pack](./find-a-pack.md) |
| You want to delete an old version | [Retire a version](./retire.md) |
| A team of developers pins each environment, and merge requests cross | [A team pins each environment](./fan-out.md) |

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
