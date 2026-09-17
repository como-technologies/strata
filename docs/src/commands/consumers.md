# strata consumers

List the clusters that use a profile version. Strata always reads the list from
Palette.

The first examples use the project `dev`, which holds `aws-stack@1.0.0` and
`aws-stack@1.1.0`. The clusters `payments` and `search` run 1.1.0.

## A version in use

```text
{{#include ../output/retire-in-use.txt}}
```

Exit `1`. Each line gives the name, uid, project, and state of a cluster.

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
  record. Strata uses the uid and the project of a record. A cluster ref is an
  error: `profile-ref-required`.
- The names and the projects come from `status.inUseClusters` of the profile.
  Strata then reads each cluster in its own project, for its state and the
  version that its copy names.
- A cluster that strata cannot read shows `unknown` and the error. Strata counts
  it as a consumer, so the exit status is `1`.

| Exit | Meaning |
| --- | --- |
| `0` | No cluster uses the version. |
| `1` | A cluster uses the version. |
| `2` | Strata could not answer. |

`--format markdown` writes a table with the version that each cluster runs.
`--json` writes [`strata-consumers/1`](../reference/documents.md#strata-consumers1).

## Options

```text
{{#include ../output/help-consumers.txt}}
```
