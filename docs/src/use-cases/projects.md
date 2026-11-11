# Environments in the projects of one tenant

## The problem

Many teams keep each environment in a project of one tenant, such as `dev` and
`prod`. Each project holds its own copy of each release, with its own uid. So a
question about two environments is a question about two projects. Strata reads
each side of the question in its own project, in one run.

Here `aws-stack@1.1.0` reached prod by a promotion. Somebody built the `1.1.1`
of prod by hand, and changed one value. The cluster `payments` of prod runs
`1.1.0`.

**Who runs this.** A person at a terminal, and a job of the pipeline, such as
the promotion gate: see
[The promotion gate and the retire gate](../automation/gates.md).

<div class="playground-demo">

`just playground projects` serves this tenant on this machine, with no key and no cost.

</div>

## The flags

Each command that reads Palette takes `--project <uid>` or `--project-name
<name>`. The flag replaces `PALETTE_PROJECT_UID` and `PALETTE_PROJECT` for one
command. `--project ''` is tenant scope. See [The project](../configure.md#the-project).

`compare` and `same-release` also take a project for each side. Side A is the
first ref: `--project-a <uid>` or `--project-name-a <name>`. Side B is the
second ref: `--project-b <uid>` or `--project-name-b <name>`. The flag of a side
has priority over `--project`.

## 1. Do dev and prod have the same release?

```text
{{#include ../output/projects-same.txt}}
```

`IDENTICAL`: prod holds the content of dev. The line `project` names the project
of each side, by uid. `strata list projects` gives the name of each uid. In the
JSON, `records[].projectUid` names the project of each record.

The copy of `1.1.1` in prod is not the release of dev:

```text
{{#include ../output/projects-different.txt}}
```

Exit `1`. The reason names the pack and the value path.

## 2. What changed between two versions?

```text
{{#include ../output/projects-compare.txt}}
```

The line `project` names the project of each side. In the JSON, `a.projectUid`
and `b.projectUid` name them. The prod copy keeps its volumes after a delete:
`Retain` in place of `Delete`.

## 3. Record each project

A gate compares the records that a reviewer approved, not what Palette holds
today. So record each project when the release reaches it:

```text
{{#include ../output/projects-record.txt}}
```

Each record keeps the uid of its project, so each later command reads the record
in that project.

## 4. Does Palette still have the version that I recorded?

```text
{{#include ../output/projects-verify.txt}}
```

One run checks the records of both projects. `verify` reads each record in the
project that the record names.

## 5. Does this cluster run its release?

```text
{{#include ../output/projects-cluster.txt}}
```

The record is a file, so strata reads it with no request. `--project-name prod`
gives the project of the cluster side.

## 6. Does the registry have newer versions of what we pin?

```text
{{#include ../output/projects-behind.txt}}
```

One run reads the pins of both projects. Strata reads each pin in the project of
the first record that pins it.

## 7. Can I delete this version?

```text
{{#include ../output/projects-consumers.txt}}
```

Exit `1`: the cluster `payments` of prod runs the copy of prod. The copy of dev
is another version with its own consumers. `--project-name dev` lists them.

## 8. Which clusters are production?

```text
{{#include ../output/projects-list.txt}}
```

The project is the environment, so the clusters of `prod` are the production
clusters. A label also selects them: see [Find the fleet from Palette](./fleet.md).

## The other questions

- **Who changed it?** `strata verify record.json --who` reads the audit log in
  the project of the record.
- **What does the update that waits for a cluster change?** and **Does this
  change replace the nodes?** `compare` reads the cluster in the project of its
  side. `--repave` asks Palette in the project of the cluster side.
- **What changed between two pack versions?** `compare pack:<name>@<version>`
  reads the registries that the scope of the command lists. `--project` gives
  that scope.
- **Which digest does each tag run as on this cluster?** The observer runs in
  one cluster, and it reads the records of the project of that cluster: see
  [The observer](../automation/observer.md).

## What to do

- Give the project of each side with `--project-name-a` and `--project-name-b`.
  A job then answers in one run, and needs no record first.
- Record each project when the release reaches it. Keep each record in the
  repository, because a gate reads the records: see
  [Promote a release to another project](./promote.md).
- Give each record a name that holds its project, such as
  `releases/1.1.0/prod.json`.
- When a name means two objects, give the uid: see
  [The same names in two projects](./same-names.md).
- [What the live example verifies](../development/live-runs.md#projectssh)
  holds the live check of the flags in two projects of one tenant.
