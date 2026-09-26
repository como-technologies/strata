# A profile runs on edge hosts

## The problem

A profile can be bound to an edge host, and not only applied to a cluster. On
an edge fleet, most of the users of a profile version are edge hosts. A person
who asks "can I delete this version?" needs them counted, and a record of what
each edge host holds.

An edge host is not a cluster, and Palette counts clusters alone in the
consumer list of a profile. Strata searches the edge hosts as well.

In this example, `dev` holds `platform-addons@1.0.0`. The cluster `payments`
runs it, and it is bound to the edge hosts `store-1` and `store-2`.

**Who runs this.** A person at a terminal before a delete, and a job that keeps
a record of each edge host.

**Follow along.** `just playground edge-hosts-1` serves this tenant on this
machine, with no key and no cost. A person edits the profile in step 3, so this
page has two seeds. Each section names the one it needs.

## 1. Count the consumers

```text
{{#include ../output/edge-consumers.txt}}
```

Exit `1`: one cluster and two edge hosts use the version. An edge host line
ends with `edge host`.

## 2. Record an edge host

```text
{{#include ../output/edge-record.txt}}
{{#include ../output/edge-record-cluster.txt}}
```

`edgehost:<host>` takes the uid or the name of the edge host. The record holds
the profiles bound to the host, and has the root of the version:

```text
{{#include ../output/edge-compare.txt}}
```

## 3. A person edits the version in place

**Follow along.** `just playground edge-hosts-2` is the same tenant after a
person edits the manifest of `platform-addons@1.0.0` in place. Stop the first
seed, and export what this one prints.

```text
{{#include ../output/edge-verify.txt}}
```

Both records are `OK`. Palette gives an edge host its own copy of each manifest
when it binds a profile, under a uid of its own, as a cluster has one. The edit
does not reach the copy. Strata reads the copy of the host by that uid. The
values of a pack are a copy too.

So the edge host now runs other content than the version that it names:

```text
{{#include ../output/edge-verify-profile.txt}}
```

Exit `1`. The note says that both sides name `platform-addons@1.0.0`: the
version changed in place after Palette bound it to the host.

## What to do

- Run `consumers` before you delete a version. Its exit counts the edge hosts.
- Record each edge host that you care about, beside each cluster, and verify
  the records on a schedule. A drifted edge host record is a change of the copy
  that the host holds.
- Record the version too. After an edit in place, the version drifts and the
  hosts keep their copies, so two edge hosts bound at two times can hold two
  contents under one version name.

## Palette behavior

The live run of `just live-edge-host` shows each rule on this page: the root of
the record, the consumers, and the copy of each manifest and each value that the
host keeps. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#edge-hosts).
