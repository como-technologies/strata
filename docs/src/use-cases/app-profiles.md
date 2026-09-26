# An app profile is updated in place

## The problem

An app profile describes an application in tiers: a Helm chart, a manifest, a
database operator. Palette versions it, but a plain update changes it: there is
no publish step, and the uid and the version stay. So `shop@1.0.0` can mean
two contents at two times, as a cluster profile edited in place can.

A tier has a type, a version, values, properties, and manifests. It has no
digest. So a record of an app profile answers a weaker question than a record
of a cluster profile. It says whether the tiers changed. It does not say whether
somebody pushed the chart of a helm tier again under its version.

In this example, `dev` has the app profile `shop@1.0.0`, with the helm tier
`web` and the manifest tier `config`.

**Who runs this.** The scheduled verify job, and a person who reviews a change
of an application.

**Follow along.** `just playground app-profiles-1` serves this tenant on this
machine, with no key and no cost. A person updates the app profile in step 3,
so this page has two seeds. Each section names the one it needs.

## 1. Record the app profile

```text
{{#include ../output/app-record.txt}}
```

Each tier is a pack in the layer `app`. The properties of a tier are its
params, and the root covers them:

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

## What to do

- Record each app profile version that you deploy, and verify the records on a
  schedule, with `--allow-no-digest` when you accept that a tier has no digest.
- Treat a drift of an app profile as an unreviewed change: Palette has no
  publish step to stop it.
- Keep the record from before the update: `compare` of the old record and the
  app profile now names each param and each manifest that changed.

## Palette behavior

The live run of `just live-app-profile` updates the manifest of a tier. It shows
that a plain update keeps the uid and the version, and that the record drifts
with the tier `AUTHORED`. A property is part of the same tier, and the root
covers it as it covers the manifest. See
[Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md#app-profiles).
