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

Palette masks the value of a sensitive variable in its answer. Strata sends the
mask as Palette gives it, so the validate route gets a mask and not the secret.
A change file merges over the variables by name. A variable of the file
replaces the value of the variable with that name. A new name adds a variable.

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

Palette lists some facts of a cloud account in one request. The instance type
route lists each type of a region, and `nonSupportedZones` of a type names the
zones that do not offer it. The key pair route lists the key pairs of the
account in a region. The tool of the cloud gives the same answer, but it needs
the credentials of the cloud on your machine. These rules need the key of
Palette alone.

`instance-offering` fails for each pool whose type names a zone of the pool in
`nonSupportedZones`. It also fails for a pool whose type the region does not
list. One request answers every pool. Here the pools name a zone that the list
marks for `t3.xlarge`:

```text
{{#include ../../output/preflight-offering.txt}}
```

`ssh-key-pair` fails when `sshKeyName` of the entity is not in the list of the
account. The property route of EKS does not check a key pair for a cluster
with a dynamic VPC. Here the entity names a key pair that the account lacks:

```text
{{#include ../../output/preflight-ssh-key.txt}}
```

The lists are the catalog of Palette, not the cloud. A type that the list does
not mark can still fail, so the `reproduce:` line asks the cloud. See
[The list routes](../../internals/palette/validate-routes.md#the-list-routes).
