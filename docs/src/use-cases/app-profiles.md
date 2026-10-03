# An app profile is updated in place

## The problem

An app profile describes an application in tiers: a Helm chart, a manifest, a
database operator. Palette versions it, and an app profile has no publish step:
an update changes the version, and the uid and the version number stay. So
`shop@1.0.0` can mean two contents at two times, as a cluster profile edited in
place can.

A tier has a type, a version, values, properties, and manifests. It has no
digest. So a record of an app profile answers a weaker question than a record
of a cluster profile. It says whether the tiers changed. It does not say whether
somebody pushed the chart of a helm tier again under its version.

In this example, `dev` has the app profile `shop@1.0.0`, with the helm tier
`web` and the manifest tier `config`.

**Who runs this.** The scheduled verify job, and a person who reviews a change
of an application.

**Follow along.** `just playground app-profiles-1` serves this tenant on this
machine, with no key and no cost. A person updates the app profile in steps 3
and 4, and a team deploys it in step 5, so this page has four seeds. Each
section names the one it needs.

## 1. Record the app profile

```text
{{#include ../output/app-record.txt}}
```

Each tier is a pack in the layer `app`. The properties of a tier are its
params, with the install order of the tier as `installOrder`. The root covers
the params. The record lists the tiers by `installOrder`, then by name:

```text
{{#include ../output/app-show.txt}}
```

## 2. Verify it

```text
{{#include ../output/app-verify-unverified.txt}}
```

Exit `1`: `UNVERIFIED`. The helm tier has no digest, so strata cannot say that
its chart is the one that it was. A manifest tier needs no digest: its content
is in the record. When you accept that, say so:

```text
{{#include ../output/app-verify-ok.txt}}
```

## 3. A person updates the app profile

**Follow along.** `just playground app-profiles-2` is the same tenant after a
person changes the replicas of the `web` tier. Stop the first seed, and export
what this one prints.

```text
{{#include ../output/app-verify.txt}}
```

Exit `1`: `DRIFT`. Both sides are `shop@1.0.0` with one uid, so the note says
the version was edited in place. The `param` line names the property that
changed.

## 4. A person changes the install order of a tier

**Follow along.** `just playground app-profiles-3` is the same tenant after a
person gives the `web` tier the install order `1`, so that it installs after
`config`.

```text
{{#include ../output/app-order-compare.txt}}
```

Exit `1`. The install order changes what a deployment of the app profile does,
so the root covers it. The `param installOrder` line names the change, with the
replicas of step 3.

## 5. Find the app deployments before a delete

Palette refuses the delete of an app profile while an app deployment uses it.
Before the team deploys `shop@1.0.0`, no deployment uses it:

```text
{{#include ../output/app-consumers-free.txt}}
```

Exit `0`: a delete can go on.

**Follow along.** `just playground app-profiles-4` is the same tenant after a
team deploys `shop@1.0.0` as `shop-dev`.

```text
{{#include ../output/app-consumers.txt}}
```

Exit `1`: Palette refuses the delete. Remove the app deployment first, or move
it to another version. The record from step 1 gives the same answer as the ref:

```text
{{#include ../output/app-consumers-json.txt}}
```

## What to do

- Record each app profile version that you deploy, and verify the records on a
  schedule, with `--allow-no-digest` when you accept that a tier has no digest.
- Treat a drift of an app profile as a change from outside the review of the
  team: an app profile has no publish step, so an update reaches the version at
  once.
- Keep the record from before the update: `compare` of the old record and the
  app profile now names each param and each manifest that changed.
- Before you delete an app profile version, run `strata consumers` on it, and
  delete only on exit `0`.

## Palette behavior

The live run of `just live-app-profile` updates the manifest of a tier. It shows
that a plain update keeps the uid and the version, and that the record drifts
with the tier `AUTHORED`. A property is part of the same tier, and the root
covers it as it covers the manifest. No live run changes the install order of a
tier. Palette names the app deployments of an app profile, and refuses the
delete of an app profile while an app deployment uses it. No live run shows
these two rules. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#app-profiles).
