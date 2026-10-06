# The same names in two projects

## The problem

Names repeat across projects. Here `dev` and `prod` each have a cluster named
`payments` and a profile version `aws-stack@1.1.1`. Strata reads a ref in one
scope, so the scope decides which object a name means. The prod copy of the
profile has one changed value.

**Who runs this.** A person at a terminal, working out which object a name means.

**Follow along.** `just playground same-names` serves this tenant on this machine,
with no key and no cost.

## 1. See every cluster

`strata list` is the command that asks Palette what is there. It takes the kind
to list, `profiles`, `clusters` or `projects`, and it lists them in one scope.
The `--project ''` here is tenant scope. Palette answers a tenant-scope listing
of clusters with the clusters of every project, which is what this step wants:

```text
{{#include ../output/names-tenant.txt}}
```

Each line ends with the project of the cluster, by name and by uid. The name
comes from the project listing, and a listing that strata cannot read leaves
the uid alone.

## 2. A name at tenant scope

With no project, the name `payments` matches two clusters:

```text
{{#include ../output/names-ambiguous.txt}}
```

Exit `2`. The error lists the uids. `strata list --cluster payments --project ''`
gives the same two clusters, with the profile versions each one runs. The
`--project ''` is tenant scope: without it, the command answers for one project
and finds one cluster.

```text
{{#include ../output/names-cluster.txt}}
```

This tenant holds no profiles at tenant scope, so a profile name there matches
nothing. Exit `2`:

```text
{{#include ../output/names-profile.txt}}
```

## 3. Name the project

```text
{{#include ../output/names-by-project.txt}}
```

Each record keeps the project it came from. That is what lets one command check
both, from tenant scope, with no project set:

```text
{{#include ../output/names-verify.txt}}
```

`verify` reads each record in the project the record names. Two clusters of one
name, in two projects, and one command answers for both.

## 4. Compare the two clusters

```text
{{#include ../output/names-compare.txt}}
```

Two clusters with one name and one version run different content.

## 5. The same name in the tenant and in the project

A profile of the tenant is in the listing of each project. When it has the
name and the version of a profile of the project, a ref matches two versions.
Strata exits `2`, and names each uid with its scope:

```text
{{#include ../output/scope-ambiguous.txt}}
```

Give the uid in place of the name. `strata list` marks the tenant version: see
[A profile of the tenant](../commands/list.md#a-profile-of-the-tenant).

## What to do

- Set `PALETTE_PROJECT` for each command. `list` and `verify` also take
  `--project` and `--project-name`; the other commands do not.
- When a profile of the tenant has the name and the version of a profile of
  the project, give the uid. `strata list --json` gives the scope of each
  version.
- In a script, name the project and the uid together:
  `PALETTE_PROJECT=prod strata snapshot cluster:64f1000000000000000000f1`. A uid
  names one cluster, and it does not widen the scope: strata reads a cluster in
  the project it works in.
- Name the records by project: `dev-payments.json`, `prod-payments.json`.
- To find the cause of the difference, see
  [Promote a release to another project](./promote.md).
