# Stop a build that Palette cannot finish

## The problem

A cluster build in Palette fails late, and often with no message. The live runs
of this repository show a GKE cluster that waits 50 minutes with "control plane
nodes addition done" while GCP refuses every create, an EKS role without the EKS
policy that fails after a full apply, a pool of 4 that Palette refuses at the
end, and a CSI pod that crash-loops while Palette says `Running`. Each one cost
30 minutes or more of cloud time before a person saw it.

Palette has validate routes that find some of these in seconds, and a create
through the provider or the API calls few of them. Others, such as an API that
is off or a role that lacks a permission, no route checks at all.

**Who runs this.** The person who writes the cluster, before the first apply.
The pipeline, on each pull request and before each build. The engineer who
scopes a migration, on the first call with a customer: how ready is this estate
to start?

**Follow along.** `just playground preflight` serves this tenant on this
machine, with no key and no cost.

## 1. The body of the create

`cluster.json` is the body of `POST /v1/spectroclusters/aws`: the cloud account,
the region, two pools of one `t3.xlarge`, and the profile `aws-stack@1.1.1`. The
provider, the API, and the console all send this shape. Run the static tier,
with no key:

```text
{{#include ../output/preflight-static.txt}}
```

Each rule is one line. The static tier reads the entity and the records alone,
so it runs on every pull request in milliseconds.

## 2. Ask Palette

With a key, strata calls the validate route of the cloud type and the other
routes that a create does not call:

```text
{{#include ../output/preflight-live.txt}}
```

The workers have too few cores for the packs. Through the provider, this is
"A minimum allocation of '4.1' CPU cores is required" after a full apply of 20
minutes. Here it is one line, and the `reproduce:` lines give the `curl` that
shows the same answer.

## 3. Read the result

The result names the level of each rule. `PROBE FAIL` is a probe: strata knows
the fault, and the runs of the lab do not yet prove the check. It does not
change the exit code, so a young rule cannot stop a build by itself. `FAIL` is a
preflight check: the runs prove it on this engine, and the exit code is `1`.
`--strict` counts a failed probe too:

```text
{{#include ../output/preflight-strict.txt}}
```

`NOT CHECKED` is a fault that strata knows and cannot check here: a check that
needs the tool of the cloud that is not on this machine, or a row with no check
yet. The `reproduce:` lines say what to run by hand.

## 4. The fix

Each failed rule names its fix and its owner. The fix of the CPU floor is on
the person: two workers, or a larger type. The fix for good is on the provider,
which can call the validate route at plan time. A findings report of this
repository asks Palette for each fix of the catalog.

## 5. Where this runs

- On each pull request: the static tier, with the records of the repository.
- Before each build: the live tier, with a key. Exit `1` stops the build.
- During the build: `--watch cluster:UID`, which stops at the first known stuck
  state with its evidence.
- In a scoping meeting: the report as HTML, with the section scorecard and the
  readiness of each section. A section under 80% says where the work is before
  a first build.

[Preflight in a pipeline](../automation/preflight.md) gives the jobs.

## What the runs show

The catalog comes from the live runs of #47 to #90 and from a read of the
source and the documentation of Palette. Each rule names its evidence: a run, a
line of the source, a page, or a findings report. Each starts as a probe, and
becomes a preflight check when the runs prove it: see
[Confidence](../internals/preflight.md#confidence). The scorecard of each
section says how much of it a live run shows, and the weakest bound of its
promoted rules.
