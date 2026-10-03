# Find the fleet from Palette

## The problem

A scheduled `verify` covers the records that somebody wrote, and each record is
a path that somebody typed. A team that builds clusters from automation keeps a
list of them in its repository, and a change of the fleet is then a change of
that list. A cluster that nobody added to the list is not checked, and nothing
says so.

Palette knows the fleet already. The automation that creates a cluster gives it
tags, and a tag of the provider is a label of the cluster: `env:prod` is the
label `env` with the value `prod`. Palette searches the clusters of every
project by their labels, and it keeps saved tag filters, which a team can
declare in code.

In this example, the project `dev` has the clusters `payments` and `search`,
labeled `env=dev`, and the project `prod` has `payments` and `ledger`, labeled
`env=prod`. The tenant has the saved tag filter `production`, which selects
`env=prod`.

**Who runs this.** A scheduled job that keeps the records of the fleet, and a
person who asks which clusters are production.

**Follow along.** `just playground fleet-1` serves this tenant on this machine,
with no key and no cost. The label of `ledger` changes in step 4, so this page
has two seeds. Each section names the one it needs.

## 1. Find the clusters of a label

```text
{{#include ../output/fleet-tag.txt}}
```

Palette searches every project the key can read, so each row names its
project, and the key needs no project of its own. Each row carries the labels
of the cluster.

## 2. Name the selector

```text
{{#include ../output/fleet-filters.txt}}
```

`filter:production` names the saved filter. Declare it in code, next to the
automation that labels the clusters, with the `spectrocloud_filter` resource of
the provider:

```hcl
resource "spectrocloud_filter" "production" {
  metadata {
    name = "production"
  }
  spec {
    filter_group {
      conjunction = "and"
      filters {
        key      = "env"
        operator = "eq"
        values   = ["prod"]
        negation = false
      }
    }
  }
}
```

## 3. Record each cluster of the set

```text
{{#include ../output/fleet-snapshot.txt}}
```

One record for each cluster, as `<name>.json`, each read in its own project.
`--prune` removes each cluster record of the directory whose cluster the filter
no longer selects. The same query as JSON:

```text
{{#include ../output/fleet-json.txt}}
```

A scheduled job reads the report of the snapshot as JSON, and not its lines.
Each entry gives the file and the root of a record, or the error of a cluster
that strata could not record:

```text
{{#include ../output/fleet-set-json.txt}}
```

See [`strata-set/1`](../reference/documents.md#strata-set1) for each field.

## 4. The fleet changes

**Follow along.** `just playground fleet-2` is the same tenant after the
automation relabels `ledger` as `env=retired`. Stop the first seed, and export
what this one prints.

```text
{{#include ../output/fleet-snapshot-again.txt}}
{{#include ../output/fleet-verify.txt}}
```

The record of `ledger` is gone, and `verify` covers the fleet as it is now.

## The loop

1. Declare the selector in code as a saved tag filter, and review a change to it
   like any other change: it says what counts as production.
2. Label each cluster from the automation that creates it.
3. On a schedule, run
   `strata snapshot filter:production --dir fleet/prod --prune`, and commit what
   it writes as a pull request.
4. Run `verify 'fleet/prod/*.json'` on the records that the repository holds.

**Why commit the resolved list.** The selector is a decision, and it belongs
under review. Which clusters meet it is discovery, and it changes whenever a
cluster starts or stops. The commit turns each change of the fleet into a diff
that a person sees: a new record is a cluster that joined, and a removed record
is a cluster that left. A cluster that loses its label by mistake shows up as a
removal in a pull request, and does not quietly drop out of the nightly
`verify`.

A new record also changes the roots that `verify` checks next: review it as you
review a new release record.

## Palette behavior

The live run of `just live-fleet` labels cluster A, makes a saved tag filter,
records the set, removes the label, and shows each rule on this page. See
[What the cluster listing answers](../internals/palette/clusters.md#what-the-cluster-listing-answers).
