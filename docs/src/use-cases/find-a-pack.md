# Find the profiles that carry a pack

## The problem

A pack version has a security problem, for example a Cilium release. You need
each profile version that carries the pack, in each project, and the clusters
that run those versions. `pack:<name>@<version>` reads the pack itself.
[What changed between two pack versions](./pack-versions.md) compares two pack
versions. Neither says which profiles carry the pack. `list` answers that.

**Who runs this.** A person at a terminal, answering an incident. Nothing here writes.

**Follow along.** `just playground list` serves this tenant on this machine, with
no key and no cost. Nothing on this page changes the tenant, so one seed serves
every step. The playground name is `list`, not the page name.

## 1. Search one project

```text
{{#include ../output/list-pack.txt}}
```

The text matches a part of the pack name, in upper or lower case: `cilium`
finds `cni-cilium-oss`. Each version shows its consumers and its packs, so the
listing answers "which versions" and "how urgent" together. Here 1.0.0 carries
the old Cilium and no cluster runs it.

## 2. Search every project

```text
{{#include ../output/list-pack-all.txt}}
```

The loop names each project. `strata list projects` gives you the names to put
in it, and a script reads them from `strata list projects --json`, under
`.projects[].name`. Prod holds its own `aws-stack@1.1.1`, under its own uid.

The projects are not the whole tenant. A profile at tenant scope belongs to no
project, so no project listing holds it. A search of the projects alone does not
find it. Read the tenant scope too:

```text
{{#include ../output/list-pack-tenant.txt}}
```

This example keeps its profiles in projects, so the tenant scope holds none. A
profile that belongs to no project is in this listing only.

## 3. Get one line for each match

A report is for a person. For a ticket or a script, take the JSON and print one
line for each pack that matches:

```text
{{#include ../output/list-pack-json.txt}}
```

Each line is one profile version and the pack it carries. The version of the
pack is the last field, so a reader sees at once that 1.0.0 is behind.

## 4. Find the clusters

For each version with the pack, list its clusters:

```text
{{#include ../output/list-pack-consumers.txt}}
```

Exit `1` means that a cluster still uses the version, and the rows name each
cluster. `no consumers` and exit `0` means the version is free to retire.

`consumers` takes no project option, so the last command names the project in
the environment. It sets `PALETTE_PROJECT_UID` to nothing first, because a uid
has priority over a name: with a uid still set, `PALETTE_PROJECT=prod` is
ignored and the answer comes from the other project, with nothing to say so. The
playground exports a uid, so keep the empty assignment when you follow along.
See [strata consumers](../commands/consumers.md) and
[Configure](../configure.md).

## What to do

- Publish a new version with a fixed pack, and move the clusters to it.
- Then [retire](./retire.md) the old versions.
