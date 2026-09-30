# One cluster, end to end, on your tenant

Type these lines, one at a time, in a terminal at the root of this repository.
They run the story of [Can Palette build this cluster?](../use-cases/stop-a-build.md)
against your tenant: check the body of a create, break it and see the check
fail, build the cluster for real, watch it stand up, inspect it, check a
change, remove it. About 35 minutes, most of it the build.

## 0. Before you start

The live example must be up with at least its profiles and accounts
(`just live-profiles`, or `just live-up`), and one run of the preflight plan
must have written a clean entity (`just live-preflight-plan aws`). The key and
the project are in `~/.palette.env`; the AWS profile of the lab is
`spectro-admin`.

```bash
cd ~/repos/spectrocloud/strata
just build
set -a; . ~/.palette.env; set +a; export AWS_PROFILE=spectro-admin
P=target/release/strata-preflight
```

Take the clean entity of the last plan run: the body of the create that built
cluster A, filled from your tenant (the account, the profile, the region, two
`t3.xlarge` pools).

```bash
R=$(grep -o 'run: [0-9]*T[0-9]*Z' preflight/evidence/runs.yaml | tail -1 | cut -d' ' -f2)
cp examples/live/runs/$R/preflight-plan/clean.json ~/cluster.json
jq '{name: .metadata.name, account: .spec.cloudAccountUid, profile: .spec.profiles[0].uid, region: .spec.cloudConfig.region}' ~/cluster.json
```

You see the name `strata-preflight-aws`, the uid of the account `como-aws`,
the uid of the release profile, and `us-east-2`.

## 1. Can it be built?

Every tier: the file itself, Palette's validate and account routes, and the
`aws` CLI against the account.

```bash
$P ~/cluster.json --cloud aws --verbose --strict; echo "exit $?"
```

For about 30 seconds you see one line for each check as it runs: the request
or the command, its answer, the time, and the verdict (`static: name-prefix:
jq over the world -> 0 ms, PROBE PASS`; `palette: palette-validate: POST
/v1/spectroclusters/aws/validate -> HTTP 200, 4260 ms, PROBE PASS`; `cloud:
aws-quotas: aws service-quotas ... -> exit 0, 732 ms, PROBE PASS`). Then the
report: each rule, a `NOTE` for the bastion, the section scorecard, and the
last line `OK: each counted check passed`. The exit is `0`.

Now break a copy: one worker of four cores, which the packs cannot run on.

```bash
jq '(.spec.machinepoolconfig[] | select(.poolConfig.isControlPlane != true) | .poolConfig) |= (.size=1 | .minSize=1 | .maxSize=1)' ~/cluster.json > ~/cluster-broken.json
$P ~/cluster-broken.json --cloud aws --verbose --strict; echo "exit $?"
```

Two rules fail: `worker-cpu-floor` and `palette-validate`, both with Palette's
own words, "A minimum allocation of '4.1' CPU cores is required across all
worker nodes. Current allocation : '4' CPU cores". Under each, `ran: POST
/v1/spectroclusters/aws/validate -> HTTP 200` and a `reproduce:` line with the
`curl` that shows the same answer. The last line is `FAIL: 2 check(s) failed`,
and the exit is `1`: with `--strict` a failed probe stops a pipeline. Through
the provider, this fault comes after a full apply of 20 minutes.

Other edits that fail the same way: `.spec.cloudAccountUid =
"000000000000000000000000"` (`account-missing`),
`.spec.cloudConfig.controlPlaneLoadBalancer = "internal"`
(`api-endpoint-reach`), `.spec.profiles[0].uid` set to the uid of a GKE profile
(`profile-cloud-mismatch`).

## 2. Build it

The body that passed is the body of the create. Send it, and keep the uid.

```bash
U=$(examples/live/api.sh post /v1/spectroclusters/aws "$(cat ~/cluster.json)" | jq -r .uid); echo "$U"
```

You see a 24-character uid within seconds. The cluster is `Provisioning` in
the console at once. A second send with the same body answers
`ResourceAlreadyExists`, because the name is taken: change `.metadata.name`
for another one.

## 3. Watch it stand up

Start the watch now. It reads the cluster, its summary, and its events every
30 seconds, judges the seven watch rules, and ends at `Running`, at a known
stuck state with its evidence, or after 90 minutes.

```bash
$P cluster:$U --watch --verbose --out /tmp/pf-watch
```

Before the first reading, one line per rule with the program it evaluates.
Then, every 30 seconds:

- `watch: read GET /v1/spectroclusters/..., GET /v1/dashboard/spectroclusters/..., GET /v1/events/...: state Provisioning, health Unknown, 12 events`,
- one line for each condition that changed, `condition CloudInfrastructureReady=True InfrastructureReady: ...` in green, `False` in yellow,
- one line for each new event, with its time and its reason,
- `watch: Provisioning after 120s, reading 5`, then the verdict of each rule, `watch: pivot-webhook-timeout: PROBE PASS`.

Some builds show `webhook-call-failed` as `WARN` about 11 minutes in, right
after the pivot, with a `Reconciler error ... failed calling webhook
"mspectrocluster.kb.io"`: Palette makes the call again, and the build goes on.
A rule that fails ends the watch with the event that shows the fault. After 13
to 22 minutes the state line turns green, `watch: Running after 764s, reading
25`, the report prints, and the last lines say `Written to /tmp/pf-watch:
report.html, report.md, report.json, world.json, entity.json`. `world.json`
holds the last reading with the full text of every event.

## 4. Once it runs

The cluster as Palette holds it, every tier:

```bash
$P cluster:$U --verbose
```

Two notes come out of a cluster that runs: `variables-in-use` (the profile is
in use, so its variable definitions cannot change) and `bastion-cost`. Now a
change before you make it: a new instance type for the worker pool.

```bash
jq -n '{spec: {machinepoolconfig: [{poolConfig: {name: "worker-pool"}, cloudConfig: {instanceType: "m5.xlarge"}}]}}' > ~/change.json
$P cluster:$U ~/change.json --verbose
```

`WARN pool-replace-fields`: "instanceType changed; on aws that makes a new
machine template, and the machines roll". Palette accepts the change, and the
machines of the pool are replaced; a warning never changes the exit code. A
change that Palette refuses is a `FAIL`.

## 5. Remove it

Delete the cluster after it is `Running`, never while it is `Provisioning`: a
delete during the build stays in `Deleting`. Then wait for `Deleted`, and read
the inventory of the cloud, which must be 0 on every line.

```bash
examples/live/api.sh delete-cluster $U
examples/live/tenant.sh cluster $U        # again every minute, until "Deleted"
examples/live/aws-inventory.sh strata-preflight-aws us-east-2
```

The delete takes about 10 minutes: the events name each security group and the
VPC as they go, and the inventory reads 0 on every line before the record says
`Deleted`. This cluster is not in the state of the live example, so `just
live-down` does not remove it; these lines do. Cluster A of the example stays
as it was.

## What each step used

| Step | Command | Tier or route |
| --- | --- | --- |
| 1 | `strata preflight FILE --cloud aws` | static, `POST .../validate`, the property and account routes, `aws` |
| 2 | `POST /v1/spectroclusters/aws` | the create route, with the same body |
| 3 | `strata preflight cluster:UID --watch` | `GET` of the cluster, its summary, and its events, every 30 seconds |
| 4 | `strata preflight cluster:UID [CHANGE]` | the cluster document and its cloud config, then every tier |
| 5 | `DELETE /v1/spectroclusters/UID` | the delete route; the inventory of the cloud after |

The same story, without a key or a tenant, is
[Can Palette build this cluster?](../use-cases/stop-a-build.md) on the
playground; the lab runs it end to end with `just live-preflight-plan aws
--build --down`, and `just live-watch --build --down` streams the watch of
step 3 on a cluster of its own.
