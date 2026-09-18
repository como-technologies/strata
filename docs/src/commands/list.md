# strata list

List the profile versions, clusters, and projects in the scope. The first
column of each line is a ref that the other commands accept.

The examples use the projects `dev` and `prod`, with `PALETTE_PROJECT=dev`.

## List the refs in the project

```text
{{#include ../output/list.txt}}
```

Profiles sort by name, then by version in natural order, so `1.9.0` comes
before `1.10.0`. Clusters and projects sort by name.

## List the projects

```text
{{#include ../output/list-projects.txt}}
```

Strata always reads the projects at tenant scope, so the list holds each
project that the key can see.

## List another project

```text
{{#include ../output/list-prod.txt}}
```

`--project <uid>` does the same with the uid.

## List the clusters of every project

```text
{{#include ../output/list-tenant.txt}}
```

`--project ''` is tenant scope. A cluster line then ends with its project. A
cluster ref works only in the project of the cluster: set `PALETTE_PROJECT`
before you use it. See [The same names in two projects](../use-cases/same-names.md).

## Find the profile versions that carry a pack

A pack is not a ref. `--pack <text>` keeps the profile versions with a pack
whose name contains the text, in upper or lower case:

```text
{{#include ../output/list-pack.txt}}
```

In a terminal, the part of each pack name that matched is marked, as a search
tool marks a match. A pipe, a file and `--no-color` get the plain name.

`--pack` reads each profile version, one request for each. It also lists only
profiles, unless you name the kinds.

`--packs` reads the same packs with no filter, so every profile version shows
what it carries:

```text
{{#include ../output/list-packs.txt}}
```

A pack under another tag name shows the tag: `24.04 (tag LTS__24.x)`. A version
that strata cannot read stays in the list with its error, because strata
cannot tell if it carries the pack.

[Find the profiles that carry a pack](../use-cases/find-a-pack.md) searches
every project.

## What to expect

Name the kinds to list: `strata list profiles`, or `strata list clusters
projects`. With no kind, strata lists profiles and clusters. Palette lists each
kind differently by scope:

| Scope | Profiles | Clusters | Projects |
| --- | --- | --- | --- |
| A project | the profiles of the project | the clusters of the project | that project only |
| Tenant scope | the profiles of the tenant only | the clusters of every project | each project that the key can see |

Exit `0`, or `2` when strata cannot read a listing. `--json` writes
[`strata-list/1`](../reference/documents.md#strata-list1).

## Options

```text
{{#include ../output/help-list.txt}}
```
