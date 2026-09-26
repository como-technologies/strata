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

The cluster keeps its copy of the manifest, and its record is `OK`. The edge
host does not: Palette holds each manifest of an edge host by its uid alone,
with no content, so the host gets the edited manifest, and its record drifted.

The values of a pack are different. An edge host keeps its own copy of them, as
a cluster does: an edit of a value in place does not reach the host.

## What to do

- Run `consumers` before you delete a version. Its exit counts the edge hosts.
- Record each edge host that you care about, beside each cluster, and verify
  the records on a schedule. A drifted edge host record after an edit in place
  of a manifest is the edit that reached the host.
- Treat an edit in place of a manifest as a change of each edge host that the
  version is bound to, at once.

## Palette behavior

The live run of `just live-edge-host` shows each rule on this page: the root of
the record, the consumers, the manifest that the host gets, and the values that
it keeps. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#edge-hosts).
