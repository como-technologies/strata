# Edge hosts, cluster groups, and app profiles

Three more objects of Palette hold profiles, and strata records each:

```text
{{#include ../../output/edge-record.txt}}
{{#include ../../output/group-record.txt}}
{{#include ../../output/app-record.txt}}
```

- `edgehost:<host>` records the profiles bound to an edge host, as for a
  cluster. The record says `kind: edgeHost`. See
  [A profile runs on edge hosts](../../use-cases/edge-hosts.md).
- `clustergroup:<group>` records the profiles that a cluster group pins for its
  virtual clusters, and names the virtual clusters in `members[]`. The record
  says `kind: clusterGroup`. See
  [A cluster group pins a profile](../../use-cases/cluster-groups.md).
- `appprofile:<name>@<version>` records an app profile: each tier is a pack in
  the layer `app`, and the properties of a tier are its params. `source.type`
  is `app`. A tier has no digest. See
  [An app profile is updated in place](../../use-cases/app-profiles.md).

The params of a tier also hold `installOrder`, the install order of the tier,
and `containerRegistryUid`, the registry of its image, when Palette gives them.
The author of the app profile sets each one, as for a property. So the root
covers them, and a change of the install order changes the root. The record
lists the tiers by `installOrder`. A tier with no `installOrder` is at 0, and
tiers of one order keep the order in which Palette lists them. So an app
profile whose tiers have no install order keeps the root of an older record.

An edge host and a cluster group give no variable values, so their records hold
the variable definitions of each profile, as a profile record does. So a record
of an edge host has the root of the profile bound to it.
