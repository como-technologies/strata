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

A person who edits a manifest of `platform-addons@1.0.0` in place does not
change what the group pins. Palette gives the group its own copy of each
manifest, under a uid of its own, and each member keeps the copy that it got.
The group and its members still agree, and a record of the group verifies. The
version drifts: a record of `platform-addons@1.0.0` from before the edit
reports `DRIFT`, and `compare` of the group with the version names the
manifest.

## When the group changes its config

The group also gives each virtual cluster a config: limits, an endpoint, and
values. The record keeps it in its `policy` tier:

```text
{{#include ../output/group-policy.txt}}
```

A person who lowers the CPU limit of the group changes no pinned profile. The
root stays, and `verify` of the record names the change:

```text
{{#include ../output/group-policy-verify.txt}}
```

`just playground cluster-groups-2` serves the tenant after the change.

## What to do

- Record each cluster group, and compare the group with each member on a
  schedule. A member that differs runs another version of a pinned profile, or
  another copy of it, as after an edit in place.
- Verify the record of each group on a schedule. A change of the limits, the
  endpoint, or the values of the group shows in the `policy` tier. The root
  stays.
- `consumers` of a pinned version names each virtual cluster that runs it: the
  consumer list of Palette names them. The cluster listing does not name them.
  So `list clusters` asks the search of Palette for them, and the row of each
  ends with its group: see
  [The virtual clusters](../commands/list.md#the-virtual-clusters).

## Palette behavior

The live run of `just live-cluster-group` builds a group on cluster A with one
virtual cluster, and shows each rule on this page. It also changes the CPU
limit of the group, and `verify` and `compare` name the same fields as on the
stand-in. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#cluster-groups).
