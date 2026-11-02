# The static tier and the live tier

The checks of the body of a create, or of a cluster as Palette holds it now:
first without a key, then with the answers of Palette.
[strata preflight](../preflight.md) gives the exit codes and the options.

## The static tier: no key, no network

```text
{{#include ../../output/preflight-static.txt}}
```

`--verbose` prints, after each check, what it ran, the answer, the time, and
the verdict: the request to Palette with its HTTP status, the command with its
exit code, or `jq over the world` for a static check. A check with one request
for each item prints one line for each request first. It is the live form of
the reproduce lines. It wins over `STRATA_QUIET`; `--quiet` wins over both.

```text
{{#include ../../output/preflight-verbose.txt}}
```

Each rule is one line: the result, the id, what the rule checks, and the time.
A failed rule adds its messages, a `reproduce:` line that shows the same thing
with `jq`, and the fix for good with its owner. `NOT CHECKED` is a rule that
strata cannot run here, with the commands to run by hand.

The result names the level of the rule. `PROBE PASS` and `PROBE FAIL` are
probes: strata knows the fault, and the runs do not yet prove the check. `PASS`
and `FAIL` are preflight checks: the runs prove them. A failed probe does not
change the exit code; a failed preflight check makes it `1`. See
[Two levels: probe and preflight check](../../internals/preflight.md#two-levels-probe-and-preflight-check).

## The live tier: the answers of Palette

With a key, strata records each profile of the entity, reads the cloud account
and the pack of each layer, and calls the validate routes of Palette:

```text
{{#include ../../output/preflight-live.txt}}
```

`POST /v1/spectroclusters/aws/validate` answered that the workers have too few
cores for the packs. The console calls this route in its create wizard. A
create through the provider or the API goes to the create route, so the same
fault shows after a full apply. See
[The validate routes](../../internals/palette/validate-routes.md).

A run sends each request one time. The backend keeps the answer of each request,
by its method, its path, and its body. A 4xx is an answer, and the backend keeps
it. An error such as a 503 is not an answer, and the next rule asks again.

So `palette-validate`, `worker-cpu-floor`, `pack-dependency`, and
`ecr-oci-images` send the validate route one time. The six rules that read an
edge host send one request for each host. The world reads each profile of the
entity through the same backend, so `template-outside` sends no second request
for a profile.

## A profile of another cloud type

Here the body of an AWS create names a profile of the cloud type `gcp`. The
validate route refuses it, and `palette-validate` fails with the message of
Palette:

```text
{{#include ../../output/preflight-cloud-mismatch.txt}}
```

The catalog has no rule of its own for this check, because the route answers it.
See [The validate routes](../../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate)
for the refusal, and
[The gaps that preflight fills](../../reference/preflight-rules.md#the-gaps-that-preflight-fills)
for the rule `pack-cloud-support`.

## A cluster as Palette holds it

`strata preflight cluster:UID` turns the cluster into the body of a create.
Each profile of that body holds the variables of the cluster: the `name` of each
variable and the `value` that the cluster gives it. `--out` writes the body to
`entity.json`:

```text
{{#include ../../output/preflight-cluster-variables.txt}}
```

The validate route reads these values. So `palette-validate` and
`worker-cpu-floor` answer for the values of the cluster, and not for the default
values of the profile. A rule over `.entity.spec.profiles[].variables` reads
them too. See
[The validate routes](../../internals/palette/validate-routes.md#the-variables-of-the-body).

Strata sends the value of a sensitive variable as the mask that Palette gives,
so the validate route gets a mask and not the secret. A change file merges over
the variables by name. A variable of the file replaces the value of the variable
with that name. A new name adds a variable.

## A rule over a list

Some checks read one document of Palette for each item of a list. The create
refuses a profile that a cluster template links, so `template-outside` reads
each profile of the entity. The edge host rules read each host of each pool. A
check of the first item alone passes an entity whose fault is in the second.

Here the entity names a second profile, and a cluster template links it. The
report names that profile and lists both requests:

```text
{{#include ../../output/preflight-each.txt}}
```

`--verbose` prints one line for each request, then the line of the check with
the count of its requests:

```text
{{#include ../../output/preflight-each-verbose.txt}}
```

A request that Palette does not answer makes the check `ERROR`, and the report
keeps the requests up to that one. [The rule format](../../reference/preflight-rules.md#a-request-for-each-item)
states `each`.

## A rule over the list of an account

Palette lists some facts of a cloud account in one request: the instance types
of a region, its key pairs, its VPCs with their subnets, and the security groups
of a VPC. The tool of the cloud gives the same answer, but it needs the
credentials of the cloud on your machine. These rules need the key of Palette
alone. See
[The list routes](../../internals/palette/validate-routes.md#the-list-routes)
for what each route gives.

`instance-offering` fails for each pool whose type names a zone of the pool in
`nonSupportedZones`. It also fails for a pool whose type the region does not
list. One request answers every pool. Here the pools name a zone that the list
marks for `t3.xlarge`:

```text
{{#include ../../output/preflight-offering.txt}}
```

The rule reads `null` in `nonSupportedZones` as no zone. Here the same pools use
`t3.large`, and they pass in that zone:

```text
{{#include ../../output/preflight-offering-null.txt}}
```

`ssh-key-pair` fails when `sshKeyName` of the entity is not in the list of the
account. The rule applies to EKS, and the list routes say why. Here the entity
of an EKS cluster names a key pair that the account lacks:

```text
{{#include ../../output/preflight-ssh-key.txt}}
```

`static-subnets` reads the VPCs of the region. It fails a subnet that no VPC
lists. It also fails a subnet in another VPC, and a subnet in another zone than
the pool says. Here the pools say that `subnet-0bbb` is in `us-east-2a`:

```text
{{#include ../../output/preflight-subnets.txt}}
```

`sg-name-collision` reads the security groups of the VPC. It fails when a group
has the name that Palette gives to a group of the cluster. Here an earlier
cluster of the name `payments` left `payments-node`:

```text
{{#include ../../output/preflight-security-group.txt}}
```

The `reproduce:` line of each rule asks the cloud, because the list is the
catalog of Palette and not the cloud.

### A rule that reads Palette and the cloud

Some rules need a fact that Palette lists, and a fact that only the cloud gives.
`aws-quotas`, `gcp-quotas`, and `azure-vcpu-quota` count the vCPUs of the pools
from the field `cpu` of the instance type route. They compare the sum with the
quota that the tool of the cloud reads, because no route of Palette lists a
quota. These rules are in the `cloud` tier. They need the key and the tool, and
the report shows the command and the request. The request has the query
`cpuGtEq=0&memoryGtEq=0`, so the route lists the small types too. A pool whose
type the route still does not list fails the rule. The message names the pool and
the type. [The rule format](../../reference/preflight-rules.md#a-read-of-palette-before-the-command)
states the key `palette` of the check.

## A rule over the property route of a cloud

`gcp-properties` sends the project, the region, and the zones of the pools to
`POST /v1/clouds/gcp/properties/validate`. It fails when Palette refuses them.
The console calls this route on a GCP create, and not on a GKE create. See
[The property routes](../../internals/palette/validate-routes.md#the-property-routes)
for what the route reads.

Here the body of a GCP create passes. `--verbose` shows the request:

```text
{{#include ../../output/preflight-gcp-properties.txt}}
```

The `reproduce:` lines of a failed rule read the project and the zones with
`gcloud`.
