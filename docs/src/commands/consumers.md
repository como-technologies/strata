# strata consumers

List the clusters that use a profile version. Strata always reads the list from
Palette.

The first examples use the project `dev`, which holds `aws-stack@1.0.0` and
`aws-stack@1.1.0`. The clusters `payments` and `search` run 1.1.0.

## A version in use

```text
{{#include ../output/retire-in-use.txt}}
```

Exit `1`. Each line gives the name, uid, project, and state of a cluster. The
project reads as a name and a uid, and as the uid alone when strata could not
read the project listing.

## A version that no cluster uses

```text
{{#include ../output/retire-free.txt}}
```

Exit `0`. Use the exit status as a gate before a delete:

```text
{{#include ../output/retire-gate.txt}}
```

See [Retire a version](../use-cases/retire.md).

## A cluster that runs an overwritten version

In another project, a version bump overwrote the version that `payments` runs:

```text
{{#include ../output/overwritten-consumers.txt}}
```

The copy of the cluster names `1.1.1`, but the uid names `1.2.0` now. A legacy
version bump or a rename overwrote the version that the cluster runs. See
[A version bump overwrites a version](../use-cases/overwritten.md).

## What to expect

- The ref is a profile uid, `<name>@<version>`, `<name>@latest`, or a profile
  record. Strata uses the uid and the project of a record. A cluster ref or a
  pack ref is an error: `profile-ref-required`.
- The names and the projects come from `status.inUseClusters` of the profile.
  Strata then reads each cluster in its own project, for its state and the
  version that its copy names.
- A cluster that strata cannot read shows `unknown` and the error. Strata counts
  it as a consumer, so the exit status is `1`.
- Palette computes `status.inUseClusters` when it is asked, in the projects the
  key can read. So strata reads each
  cluster the key can list as well, for the profiles it applies. A cluster that
  applies the version and is not in the list joins the answer, marked
  `not in the consumer list of the profile`. The last line says how far that
  second reading got: `second reading: 3 of 3 clusters read`, or
  `second reading: not done` with the reason.
- A cluster that Palette is deleting stays in the list with the state
  `Deleting`, and the exit is `1`. After the deletion, the list drops it. See
  [What the cluster listing answers](../internals/palette.md#what-the-cluster-listing-answers).

| Exit | Meaning |
| --- | --- |
| `0` | No cluster uses the version: the profile lists none, and no cluster the key can list applies it. |
| `1` | A cluster uses the version. |
| `2` | Strata could not answer. |

`--format markdown` writes a table with the version that each cluster runs.
`--json` writes [`strata-consumers/1`](../reference/documents.md#strata-consumers1).

## Options

```text
{{#include ../output/help-consumers.txt}}
```
