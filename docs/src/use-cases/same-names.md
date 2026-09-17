# The same names in two projects

## The problem

Names repeat across projects. Here `dev` and `prod` each have a cluster named
`payments` and a profile version `aws-stack@1.1.1`. Strata reads a ref in one
scope, so the scope decides which object a name means. The prod copy of the
profile has one changed value.

## 1. See every cluster

```text
{{#include ../output/names-tenant.txt}}
```

Each line ends with the project of the cluster.

## 2. A name at tenant scope

With no project, the name `payments` matches two clusters:

```text
{{#include ../output/names-ambiguous.txt}}
```

Exit `2`. The error lists the uids.

A profile name at tenant scope finds only the profiles of the tenant, not of a
project:

```text
{{#include ../output/names-profile.txt}}
```

## 3. Name the project

```text
{{#include ../output/names-by-project.txt}}
```

Each record keeps its project uid. `verify` checks each record in its own
project.

## 4. Compare the two clusters

```text
{{#include ../output/names-compare.txt}}
```

Two clusters with one name and one version run different content.

## What to do

- Set `PALETTE_PROJECT` for each command, or give `--project-name`.
- In a script, use a uid: `cluster:64f1000000000000000000f1`. A uid also needs
  the project of the cluster.
- Name the records by project: `dev-payments.json`, `prod-payments.json`.
- To find the cause of the difference, see
  [Promote a release to another project](./promote.md).
