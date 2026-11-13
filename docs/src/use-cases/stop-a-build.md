# Can Palette build this cluster?

One cluster, end to end: check the body of the create, build it, watch it
stand up, inspect it, check a change, remove it.

## The problem

A cluster build depends on the cloud account, its quotas, its network, and the
packs of the profile. A fault in one of them shows after the build starts, in
the cloud, and the build runs until then. The live runs of this repository show
a GKE cluster that waits 50 minutes with "control plane nodes addition done"
while GCP refuses each create, an EKS role without the EKS policy that stops
the build after a full apply, a pool too small for the packs that the create
refuses after a full apply, and a CSI pod that restarts while the cluster is
`Running`. Each one cost 30 minutes or more of cloud time before a person saw
it.

Palette has validate routes that find some of these in seconds, and the console
calls them in its create wizard. A create through the provider or the API goes
to the create route, so a pipeline gets the answer of the routes when a step
calls them. A fault of the cloud, such as an API that is off or a role without
a permission, is outside the routes, and the tool of the cloud shows it.

**Who runs this.** The person who writes the cluster, before the first apply.
The pipeline, on each pull request and before each build. The engineer who
scopes a migration, on the first call with a customer: how ready is this estate
to start?

<div class="playground-demo">

`just playground preflight` serves this tenant on this
machine, with no key and no cost. Each command below runs against it.

</div>

## 1. Can it be built?

`cluster.json` is the body of `POST /v1/spectroclusters/aws`: the cloud account,
the region, two pools of one `t3.xlarge`, and the profile `aws-stack@1.1.1`. The
provider, the API, and the console all send this shape. Run the static tier,
with no key:

```text
{{#include ../output/preflight-static.txt}}
```

Each rule is one line. The static tier reads the entity and the records alone,
so it runs on every pull request in milliseconds. Each rule that ran here is a
probe, so no check of the run can fail it, and the last line says
`NOT COUNTED`. The readiness names the tiers that ran.

With a key, strata calls the validate route of the cloud type and the other
routes that a create does not call, and the tool of the cloud when it is on the
machine. The validate route needs `cluster.create`, and no viewer role gives
it: see [The key of the live tier](../commands/preflight/tiers.md#the-key-of-the-live-tier).

```text
{{#include ../output/preflight-live.txt}}
```

The workers have too few cores for the packs. Through the provider, this is
"A minimum allocation of '4.1' CPU cores is required" after a full apply of 20
minutes. Here it is one line, and the `reproduce:` lines give the `curl` that
shows the same answer.

The result names the level of each rule. `PROBE FAIL` is a probe: strata knows
the fault, and the runs of the lab do not yet prove the check. It does not
change the exit code, so a young rule cannot stop a build by itself. `FAIL` is a
preflight check: the runs prove it on this engine, and the exit code is `1`.
`--strict` counts a failed probe too, and is the gate of a pipeline:

```text
{{#include ../output/preflight-strict.txt}}
```

`NOT CHECKED` is a fault that strata knows and cannot check here: a check that
needs the tool of the cloud that is not on this machine, or a row with no check
yet. The `reproduce:` lines say what to run by hand. Each line reads, and changes
nothing, with one exception. At `validate/repave`, Palette can save the address
of the console of a pack of virtual machines, and the line says so: see
[The reproduce lines](../reference/preflight-rules.md#the-reproduce-lines).
The fix of the CPU floor is on the person: two workers, or a larger type. Change
the file, run the command again, until it is clean.

A rule that reads a list asks Palette once for each item. An add-on profile that
a cluster template links fails the build in any position of the list. The same
holds for an edge host that another cluster uses, in any pool. The report names
that item. See
[A rule over a list](../commands/preflight/tiers.md#a-rule-over-a-list).

A rule can also read a list of the cloud account in one request. `instance-offering`
fails a pool whose instance type a zone does not offer. On EKS, `ssh-key-pair`
fails a key pair that the account lacks. See
[A rule over the list of an account](../commands/preflight/tiers.md#a-rule-over-the-list-of-an-account).

### After an upgrade of Palette

The first AWS create after an upgrade of Palette updates the CAPA stack of the
account. An update that the role of the cloud account cannot apply stops each
AWS create of the account, and only the events of the cluster say why: see
[The CAPA stack of an AWS account](../internals/palette/clusters.md#the-capa-stack-of-an-aws-account).
Three cloud rules check the stack with the `aws` tool:

```text
{{#include ../output/preflight-capa-rules.txt}}
```

Give the release with its patch, such as `--release 4.10.12`. A patch release
can change the stack too, and `capa-stack-release-behind` compares the parts of
the release that you give. A warning of `capa-stack-update-denied` names each
IAM action that the role cannot do, and the resources. Before the next create,
grant the actions, or let the landing zone own the stack.

## 2. Build it

The body that passed is the body of the create. Send it, and keep the uid of
the answer. The key goes to `curl` on standard input, with `--config -`. It then never
appears in the list of processes:

```bash
uid=$(printf 'header = "ApiKey: %s"\n' "$PALETTE_API_KEY" \
  | curl -s --config - -X POST -H "ProjectUid: $PALETTE_PROJECT_UID" \
  -H 'Content-Type: application/json' --data @cluster.json \
  "https://api.spectrocloud.com/v1/spectroclusters/aws" | jq -r .uid)
```

The provider and the console send the same body. The answer is `{"uid": "..."}`
in seconds; the build takes 20 minutes or more.

## 3. Watch it stand up

From the moment Palette lists the cluster, the watch reads it, its summary, and
its events every 30 seconds, judges each watch rule, and ends at `Running`, at
a known stuck state with its evidence, or at `--limit`. `--verbose` prints, at
each reading, the three requests it made, each condition that changed, each new
event, and the verdict of each rule:

```text
{{#include ../output/preflight-watch.txt}}
```

The cluster of this tenant is `Running`, so the watch ends at the first reading.
On a real build, the same command reads for 20 minutes: the conditions turn
`True` one by one, the events say what Palette does, and a rule that fails stops
the watch with the event that shows the fault. The live runs show a build on aws
that reached `Running` after 43 readings, with a warning of `webhook-call-failed`
along the way; a build on eks that the watch stopped at a failed webhook call;
and a build on gcp that the watch warned on and that went on to `Running`:
[Live runs](../development/live-runs.md#watchsh). `--out DIR` keeps the report
and the last reading. If Palette deletes the cluster, the watch ends at once. The exit code is `0`
when no counted rule failed, and the last line then says that the cluster is gone: see
[The watch](../commands/preflight/watch.md).

## 4. Once it runs

`cluster:UID` reads the cluster as Palette holds it, which is the entity the
create sent, and runs every tier on it. The entity holds the value that the
cluster gives each variable of its profiles, so the validate route checks those
values. See [A cluster as Palette holds it](../commands/preflight/tiers.md#a-cluster-as-palette-holds-it).

```text
{{#include ../output/preflight-cluster.txt}}
```

A change comes as a second file: the pools or the profiles that change. Before
the change is made, strata says what it does to the cluster:

```text
{{#include ../output/preflight-change.txt}}
```

A new instance type makes a new machine template on aws, and the machines of
the pool roll. That is a warning: Palette accepts the change, and a warning
never changes the exit code. A change that Palette refuses is a failure.

## 5. Remove it

Delete a cluster that is `Running`, never one that is `Provisioning`: a delete
during the build stays in `Deleting`, and the rule `delete-while-provisioning`
says so on `cluster:UID`. Then read what the cloud still holds. The
`reproduce:` lines of `force-delete-leaves-cloud` read it by the name of the
cluster. Each line must find nothing before any force delete:

```bash
strata preflight cluster:$uid --rule force-delete-leaves-cloud   # before the delete: the reads of the cloud for this cluster
printf 'header = "ApiKey: %s"\n' "$PALETTE_API_KEY" \
  | curl -s --config - -X DELETE -H "ProjectUid: $PALETTE_PROJECT_UID" \
  "https://api.spectrocloud.com/v1/spectroclusters/$uid"
```

[The live example](../development/live-example.md#when-a-teardown-is-slow-or-stuck)
gives the steps when a delete does not end.

## Where this runs

- On each pull request: the static tier, with the records of the repository.
  `preflight-comment.yml`, or `pr-comment.sh --preflight` on another host, posts
  the report as a comment: see
  [The report as a comment](../automation/preflight.md#the-report-as-a-comment).
- Before each build: the live tier, with a key. `--strict` makes exit `1` the
  gate. A job with no key runs offline, and its log and its report say so: see
  [The key of the job](../automation/preflight.md#the-key-of-the-job).
- During the build: `--watch cluster:UID`, which stops at the first known stuck
  state with its evidence. In the workflow, give the input `watch`: see
  [The watch in the workflow](../automation/preflight.md#the-watch-in-the-workflow).
- In a scoping meeting: the report as HTML, with the section scorecard and the
  readiness of each section. A section under 80% says where the work is before
  a first build.

[Preflight in a pipeline](../automation/preflight.md) gives the jobs, and
[The live example](../development/live-example.md#the-preflight-checks) the
lab that runs this story on a real tenant: `just live-watch --build --down`
builds a cluster and streams the watch of step 3 to the terminal.

[One change, start to end](../walkthrough.md#5-before-a-build) puts this check between the
promotion of a release and the build of a cluster on it.

## What the runs show

The catalog comes from the live runs of #47 to #90 and from a read of the
source and the documentation of Palette. Each rule names its evidence: a run, a
line of the source, a page, or a findings report. Each starts as a probe, and
becomes a preflight check when the runs prove it: see
[Confidence](../internals/preflight.md#confidence). The scorecard of each
section says how much of it a live run shows, and the weakest bound of its
rules with runs.
