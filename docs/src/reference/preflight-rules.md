# The rule format

A rule is data: one thing that can stop or break a cluster build, where a check
finds it, how much to trust the check, and the check itself as a jq program. The
catalog in the binary and a file of custom probes (`--probes FILE`) are one
format. One file holds one section.

```yaml
section: networking
title: Networking
about: The zones, the subnets, the CIDRs, and the routes that a build needs.
rules:
  - id: eks-one-zone
    title: EKS needs subnets in two zones
    engines: [eks]
    tier: static
    level: probe
    severity: fail
    status: source
    what: |
      EKS needs subnets in two zones or more. Palette checks the count of the control plane
      subnets, not the count of distinct zones.
    check:
      kind: static
      expr: |
        [ .entity.spec.machinepoolconfig[]
          | select(((.cloudConfig.azs // []) | unique | length) < 2)
          | "pool \(.poolConfig.name) names one zone; EKS needs 2" ]
    fix:
      owner: palette
      text: Check the count of distinct zones in the EKS validate route.
    reproduce:
      - "aws ec2 describe-subnets --region {{ .entity.spec.cloudConfig.region }} --query 'Subnets[].AvailabilityZone'"
    evidence:
      - kind: report
        ref: palette-precreate-validation-gaps-findings-2026-09-28 F6
    tests:
      - fault: fixtures/eks/one-zone.json
        clean: fixtures/eks/clean.json
```

`strata preflight --schema rules` prints the JSON Schema of a section file.

## The fields of a rule

| Field | Values | Meaning |
| --- | --- | --- |
| `id` | kebab-case, unique | The id of the row of the catalog. The report and the ledger name a rule by it. |
| `title` | one line | The good state, as a statement. |
| `engines` | `aws`, `eks`, `gcp`, `gke`, `azure`, `aks`, `maas`, `edge`, `vsphere`, `lab`, `all` | The engines the rule applies to. `lab` marks a check of the lab; a promotion counts the clouds alone, because no run of the plan records `lab`. Palette removed OpenStack in 2026, so it is not one. |
| `tier` | `static`, `palette`, `cloud`, `watch`, `race`, `tool` | Where a check can find the fault. See [The tiers](#the-tiers). |
| `level` | `probe` (default), `preflight` | How much to trust the check. See [Two levels](../internals/preflight.md#two-levels-probe-and-preflight-check). |
| `severity` | `fail` (default), `warn`, `note` | `fail`: the build cannot finish, or the cluster breaks. `warn`: it finishes, and something is wrong or costs. `note`: a fact. |
| `status` | `seen`, `source`, `inferred`, `proposed` | A live run shows it; the source or the documentation of Palette shows it; a reading, not confirmed; a proposal of strata. |
| `releases` | a range, such as `>=4.9 <4.11` | The Palette releases the rule applies to. No value is every release. |
| `what` | text | What goes wrong, and what Palette shows. |
| `when` | a jq program | When the rule applies: `.before != null` for a change, `.clusterUid != null` for an existing cluster, `.account != null` for the cloud account. A rule that does not apply is `SKIP`. |
| `check` | see below | The check. A rule with none is a row: `NOT CHECKED` with its `reproduce` lines. |
| `fix` | `owner`, `text` | The fix for good: `palette`, `provider`, `lab`, `strata`, `operator`, or `none`. |
| `reproduce` | commands | What a person runs to see the same thing. `{{ jq }}` templates over the world are filled in the report. |
| `evidence` | `kind`, `ref`, `note` | `run`: an issue or a run id. `source`: the source or the documentation of Palette shows it. `doc`: a page. `report`: a findings report and its finding. A `source` entry has a `note` that states what Palette does. It quotes no code of Palette, and names no file, function, or commit of it. Its `ref` is empty, or a file of this repository. |
| `promotion` | `bar`, `runs` | For a preflight check: the bar its bounds met, `0.8`, and the run ids of the promotion. |
| `tests` | `fault`, `clean` | Fixtures the test of the catalog runs the check on: the fault must fail, the clean must pass. |

## The tiers

| Tier | Letter in #91 | Where the check runs | Kind of check |
| --- | --- | --- | --- |
| `static` | S | The entity and the records alone. No key, no network. | `static` |
| `palette` | P | One request to Palette, or one request for each item of a list. | `palette` |
| `cloud` | C | One command with the tool of the cloud, on this machine. | `exec`, or no check with `reproduce` lines |
| `watch` | W | The conditions and the events of the cluster during the build. | `watch` |
| `race` | X | No check before the build finds it; the watch finds its effect. | none |
| `tool` | T | A rule of the tool that makes the entity. Out of scope; the row stays. | none |

A rule with a check has the tier of its check. Strata does not call a cloud
itself: a `cloud` rule runs the tool of the cloud (`aws`, `gcloud`) when it is
on the machine and authenticated, and prints `NOT CHECKED` with the command
when it is not.

## The checks

Each kind runs a jq program over the [world document](#the-world-document), and
the kinds differ in what they add to it first. The program outputs one of three
shapes: `true` or `false`; a list of messages, where an empty list passes and
each message is one failure; or an object `{pass, message}`.

| Kind | Fields | Adds to the world |
| --- | --- | --- |
| `static` | `expr` | nothing |
| `palette` | `method` (`GET`, `POST`), `path` (with `{{ jq }}` templates), `body` (a jq program that gives the body of a POST; default `.entity`), `each` (a jq program that gives a list; see [A request for each item](#a-request-for-each-item)), `expr` | `response`: `{ok, status, body}`. A 4xx is an answer with `ok: false` and its `V1Error` body; a 204 is `ok: true` with a `null` body. A 5xx or a transport error is `ERROR`. With `each`, `responses` instead. |
| `exec` | `argv` (each with templates), `needs` (the tools, checked on the PATH), `timeout` (seconds, default 60), `expr` | `exec`: `{exit, stdout, stderr}`. `stdout` is JSON when it parses. A tool that is not there is `NOT CHECKED`. |
| `watch` | `expr` | `cluster`, `summary`, `events`, `elapsed` (seconds), on each reading |

jq is jaq 3, which runs the programs of jq 1.7 with a small number of
differences. Two to know: `false // x` gives `x`, so read a boolean with
`if has("k") then .k else true end`; and `IN(...)` is not defined, so test a
value against a list with `(["a", "b"] | index($v)) != null`. A program that
does not compile makes the rule `ERROR`, and the message names it. So does a
program that gives no output, or `null`: a path that is not in the world is a
wrong program, not a pass.

## A request for each item

A rule over a list needs one request for each item. The items can be the
profiles of the entity, the hosts of every pool, or the pools with a host
resource group. Give the check of Palette an `each`: a jq program over the world
that outputs a list. The check makes its request one time for each item, in the
order of the list.

`$item` is the item in the templates of `path` and in the program of `body`.
`expr` runs one time, over the world with `responses` added instead of
`response`. `responses` has one `{item, response}` for each item. Each
`response` has the shape of `response` above, so a 4xx for one item is the
answer for that item.

```yaml
check:
  kind: palette
  each: .entity.spec.profiles
  path: /v1/clusterprofiles/{{ $item.uid }}
  expr: |
    [ .responses[]
      | select((.response.body.status.inUseClusterTemplates // []) | length > 0)
      | "profile \(.item.uid) is linked to a cluster template" ]
```

A check with no `each` makes one request. A list with no item makes no request,
and `responses` is empty. The verdict is `ERROR` when `each` does not output a
list, when a template fails, or when Palette does not answer one request. The
catalog refuses a `path` or a `body` that reads `$item` when the check has no
`each`.

To send one request for several items, group the items in `each`.
`vsphere-compute-resources` sends one request for each compute cluster, and
lists the pools of that cluster in the item.

The report keeps each request in `requests[]`. `--verbose` prints one line for
each request. For a GET, the report adds one `curl` line for each request to its
`reproduce` lines. A rule with `reproduce` lines of its own for the engine gets no
such line, so no request has two lines. See
[The static tier and the live tier](../commands/preflight/tiers.md#a-rule-over-a-list).

## A check that a validate route answers

A check that a validate route answers is a call of that route. A validate route
of Palette checks the body of a create with the rules of the create.
`palette-validate` sends the body to the route of the cloud type, so the route
reports each check that it answers. The catalog does not copy such a check,
because a copy becomes wrong when Palette changes the rule. See
[the rules of the scope](../boundary.md#the-rules-of-the-scope).

A rule leaves the catalog only when the route answers it. A call of the route
must show the answer: a call by hand, which changes nothing, or a live check. A
guess is not enough. A rule that the route does not answer stays in the catalog. It is
a gap that preflight fills. The call needs the key, so a run with no key does
not make the checks that the route answers.

### The checks that the route answers

| Rule that left the catalog | The check | What the route answers |
| --- | --- | --- |
| `gke-pool-multiple-3` | The size of a GKE worker pool is a multiple of 3. | `InvalidParam`, "Node pool size should be multiple of 3". A live run shows it: see [What the preflight runs show](../development/live-runs.md#what-the-preflight-runs-show). |
| `profile-cloud-mismatch` | Each profile is of the cloud type of the cluster, or of `all`. | HTTP 500 and `ClusterProfilesNotSupported`. `palette-validate` reports the refusal. See [The validate routes](../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate). |

A run with `--rule` and one of these ids is a `usage` error with exit `2`, because the
id is not in the catalog.

### The checks that wait for a call of the route

Each rule in this table copies a check that strata expects the route to answer.
The rule stays in the catalog until [`validate-route.sh`](../development/live-runs.md#validate-routesh)
shows that the route refuses the fault. Then the part in the second column
leaves the rule, and its fixtures leave with it.

| Rule | The part that strata expects the route to answer |
| --- | --- |
| `vsphere-instance-floor` | All of it: the CPU, the memory, the disk, and the missing instance type. |
| `eks-ami-type` | The AL2 type, and a type that is not known. |
| `edge-two-node` | The Kubernetes pack that is not K3s. |
| `ssh-key-format` | The vSphere and Edge engines. |
| `azure-ssh-key` | The empty key. |
| `pack-state` | The `disabled` and `deleted` states. |
| `vsphere-static-ip-pool` | All of it. |
| `vsphere-controlplane-endpoint` | The empty host. |
| `edge-vip-multi-node` | The empty host. |
| `azure-subnets-in-vnet` | The empty `vnetCidrBlock`. |
| `ntp-servers` | The format of a server. |

### The gaps that preflight fills

These checks are not copies of an answer of the route. The route does not report
them, or they need more than the body of the create. They stay in the catalog.

| Rule | What stays |
| --- | --- |
| `eks-ami-type` | The hop limit of the instance metadata. |
| `edge-two-node` | The count of the control plane hosts, and the priority of each. |
| `ssh-key-format` | The MAAS engine. |
| `vsphere-controlplane-endpoint` | The mapping of the dynamic DNS. |
| `edge-vip-multi-node` | The count of the hosts. |
| `azure-subnets-in-vnet` | The containment of the subnet CIDRs in the CIDR of the vnet. |
| `ntp-servers` | The warning when the cluster names no server. |
| `pack-cloud-support` | All of it. No route refuses the fault: see [The validate routes](../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate). |
| `template-outside`, the `edge-host-*` rules | They read a document of the tenant. No validate route answers them. |
| `instance-offering`, `ssh-key-pair` | They read a list of the cloud account: the instance types of a region, and its key pairs. No route refuses the fault, and Palette owns the rule. For `ssh-key-pair`, the gap is an EKS cluster with a dynamic VPC or with no subnet id. |
| `palette-validate`, `pack-dependency`, `ecr-oci-images`, `worker-cpu-floor`, `eks-public-subnet-mapping`, `iaas-pod-identity`, `sts-account-not-assumable`, `gcp-properties` | They call a route, and copy no answer of it. |

## The world document

```text
{
  "cloud":      "eks",                the engine
  "release":    "4.10" | null,        --release
  "source":     "cluster.json",       where the entity came from
  "entity":     { metadata, spec },   the body of POST /v1/spectroclusters/{cloudType}; for cluster:UID,
                                      each profile holds the variables of the cluster
  "before":     { ... } | null,       the entity before a change
  "profiles":   [ record, ... ],      one strata record for each profile of the entity
  "packs":      { "<packUid>": ... }, GET /v1/packs/{uid} for each layer, when live
  "account":    { ... } | null,       the cloud account, with no secret
  "clusterUid": "<uid>" | null,       for cluster:UID
  "accountKind": "aws",               the kind of cloud account of the engine
  "routeCloud": "edge-native"         the cloud type of the engine in a route
}
```

`accountKind` is the kind of the cloud account of the engine. `eks` uses an
account of kind `aws`, `gke` one of kind `gcp`, and `aks` one of kind `azure`.
`routeCloud` is the cloud type of the engine in a route of Palette: the engine
itself, and `edge-native` for `edge`. A rule reads these two fields, and does not
map the engine itself.

A record is a `strata-snapshot/1` document: `source.{uid, name, version,
cloudType}` and `packs[].{layer, name, version, tag, packUid, registryUid,
values, valuesCanon, hash}`. `valuesCanon` is the values YAML as JSON, so a
program reads a value of a pack with a path. The world never holds a secret:
the fields of a cloud account that hold one are `<redacted>`. `--out` writes
the world as `world.json`.

## Fixtures

A fixture is a world with the answers written in, for the test of the catalog:
`_responses` maps `"METHOD /path"` to a `response`, `_exec` maps a command to
its output, `_tools` lists the tools that are "there", and `_palette: false`
says no key. A check with `each` needs one entry of `_responses` for each
request. A fixture can say `_base` and hold only what differs:

```json
{ "_base": "clean.json",
  "_note": "the one worker pool has size 0",
  "_patch": { "entity": { "spec": { "machinepoolconfig": [ { "poolConfig": { "name": "worker-pool", "size": 0 } } ] } } },
  "_delete": [ "/account/spec/sts" ] }
```

`_patch` merges as a change of a cluster merges: an object key by key, a pool
by its name, a profile by its uid, a variable by its name; another list
replaces. A `_responses` beside `_base` replaces the answers of the base, so put
it in `_patch` to add to them. `_delete` removes each JSON pointer. The test of
the catalog runs every fixture, and refuses a fixture that no rule names.

## The ledger

The ledger holds the runs of the test plan: one entry for each run, with the
engine, the Palette release, and what each check answered.

```yaml
schema: strata-preflight-ledger/1
runs:
  - run: <the UTC time of the run directory>
    engine: gke
    release: "4.10"
    results:
      - {rule: gcp-name-length, result: caught}        # the run made the fault, and the check found it
      - {rule: gke-api-off, result: passed}            # a clean build, and the check passed
      - {rule: eks-one-zone, result: missed}           # the run made the fault, and the check passed: wrong
      - {rule: worker-cpu-floor, result: false-fail}   # a clean build, and the check failed: wrong
```

The binary carries the ledger of the lab. `--ledger FILE` adds the runs of a
company. `4.10.3` and `4.10` are one release for the counts.
`strata preflight --schema ledger` prints the JSON Schema.
