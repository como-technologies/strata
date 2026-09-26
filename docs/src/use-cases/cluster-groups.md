# A cluster group pins a profile

## The problem

A cluster group pins add-on profiles for its virtual clusters. Each virtual
cluster that a team creates in the group runs them, beside the profiles of its
own. A platform team must know that each member runs the pin, and not a version
from before.

A comparison of the group with a whole member is not the question: the member
runs more than the pin. Strata compares the pin with the same profiles on the
member.

In this example, the cluster group `dev-group` pins `platform-addons@1.0.0`.
Its virtual clusters `tenant-a` and `tenant-b` run it, and `tenant-a` runs
`team-a-apps@1.0.0` as well.

**Who runs this.** A job that checks each member of a group, and a person who
changes the pin.

**Follow along.** `just playground cluster-groups-1` serves this tenant on this
machine, with no key and no cost.

## 1. Record the group

```text
{{#include ../output/group-record.txt}}
```

The record holds the pinned profiles, and names the virtual clusters of the
group:

```text
{{#include ../output/group-show.txt}}
```

## 2. Compare the pin with a member

```text
{{#include ../output/group-compare.txt}}
```

Exit `0`: `tenant-a` runs the pin. The cluster side holds `platform-addons`
alone, and not `team-a-apps`. When one side is a cluster group, and the other is
a `cluster:` ref that selects no profile, the cluster side holds only the
profiles that the group pins. In a script, compare the record with each member
that it names:

```bash
for member in $(jq -r '.source.cluster.members[].uid' runs/dev-group.json); do
  strata compare runs/dev-group.json "cluster:$member" || echo "$member differs from the pin"
done
```

## After an edit in place

A person who edits a manifest of `platform-addons@1.0.0` in place changes what
the group pins: the group holds each manifest by its uid, and not a copy. Each
member keeps the copy that it got. So after the edit, `compare` of the group
with a member names the manifest, and exits `1`. A record of the group from
before the edit drifts.

## What to do

- Record each cluster group, and compare the group with each member on a
  schedule. A member that differs runs another version of a pinned profile, or
  another copy of it, as after an edit in place.
- `consumers` of a pinned version names each virtual cluster that runs it: the
  consumer list of Palette names them. The cluster listing does not, so `list
  clusters` does not show a virtual cluster.

## Palette behavior

The live run of `just live-cluster-group` builds a group on cluster A with one
virtual cluster, and shows each rule on this page. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#cluster-groups).
