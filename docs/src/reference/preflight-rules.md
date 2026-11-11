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
      - kind: run
        ref: a live run
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
| `fix` | `owner`, `text` | The fix for good. `owner` is one of six values, and the loader refuses another: `palette`, `provider`, `lab`, `strata`, `operator`, or `none`. |
| `reproduce` | commands | What a person runs to see the same thing. `{{ jq }}` templates over the world are filled in the report. A line reads and changes nothing, with one exception that the line names: see [The reproduce lines](#the-reproduce-lines). |
| `evidence` | `kind`, `ref`, `note` | `kind` is one of four values, and the loader refuses another. `run`: an issue or a run id. `source`: the source or the documentation of Palette shows it. `doc`: a page. `report`: a findings report and its finding. |
| `evidence` of `kind` `source` | `ref`, `note` | A `source` entry has a `note` that states what Palette does. It quotes no code of Palette, and names no file, function, or commit of it. Its `ref` is empty, or a file of this repository. |
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
when it is not. Each call of a cloud is a fact of the list: see [The facts of the
clouds](../internals/preflight.md#the-facts-of-the-clouds).

## The reproduce lines

A person runs a `reproduce` line of a report by hand, to see what a check saw.
So each line reads, and changes nothing. One route of Palette is an exception,
and its line says so. The rules of a line:

- A line reads Palette, the cloud, the cluster, or a file. It deletes nothing,
  makes nothing, and changes no setting.
- A `POST` goes to a validate route of Palette, which checks the body: see
  [The validate routes](../internals/palette/validate-routes.md). The test of
  the catalog names each such route that saves nothing.
- One route can save a record, `validate/repave`: for a pack of virtual
  machines, Palette can save the address of its console. A line can send it only for a check
  that names `console-address` in `saves`. The line ends with
  `{{ saves_comment("console-address") }}`. The report fills it with a comment
  that says what Palette saves, and the line that the report adds for the check
  ends with the same comment. See
  [What the route saves](../internals/palette/validate-routes.md#what-the-route-saves).
- A line writes no file outside the current directory and the folder of
  `world.json`. The lines that the report adds for a check write `request.json`
  and `body.json` into that folder, beside the values of the variables. See
  [The report on disk](../commands/preflight/report.md#the-report-on-disk).
- A line makes no login to a registry or a cloud. A login writes the credentials
  to a file in the home directory.
- A line names no path of the live example, `examples/live`. No machine outside
  the lab has the scripts of the live example.
- A line that reads Palette starts its `curl` with `{{ palette_curl }}`, and
  names no header itself.
- A line that starts with `#` is words. It says what to read by hand when no
  command of the rule reads it, such as on a cloud with no line of its own.

A rule that only the lab can reproduce says so in such a line, as
`inventory-managed` does. A command that fixes the fault is no reproduce line,
because the operator decides when it runs. Write the lines of a file of custom
probes the same way. A test of the catalog fails on a line that changes
something, and on a text of a rule that names `examples/live`. It also fails on
a line that reads Palette with no `{{ palette_curl }}`, or that names a header.
It fails on a line that sends `validate/repave` with no
`{{ saves_comment("console-address") }}` at its end.

The report fills `{{ palette_curl }}` with the `curl` that its own lines start
with. That `curl` sends the key in a config that `printf` writes into `<( )`.
`printf` is a builtin of the shell, so no process gets the key as an argument.
The list of processes does not show it, also on a shared runner of CI. The
config reads a `\` and a `"` in the key as an escape, so the line puts a `\`
before each one. A line break in the key can add a line to the config. So a key
with a line break stops the line before `curl`, with the message
`the key of Palette holds a line break`. The `curl` sends the project in the
header `ProjectUid`, and the line gives the address as
`"${PALETTE_API:-https://api.spectrocloud.com}/v1/..."`. Run the line in bash or
zsh, which have `[[ ]]` and `<( )`.

## The checks

Each kind runs a jq program over the [world document](#the-world-document), and
the kinds differ in what they add to it first. The program outputs one of three
shapes: `true` or `false`; a list of messages, where an empty list passes and
each message is one failure; or an object `{pass, message}`.

| Kind | Fields | Adds to the world |
| --- | --- | --- |
| `static` | `expr` | nothing |
| `palette` | `method`, `path`, `body`, `each`, `refusal_codes`, `saves`, `expr` | `response`: `{ok, status, body}`. A 4xx is an answer with `ok: false` and its `V1Error` body; a 204 is `ok: true` with a `null` body. A 5xx with a code of `refusal_codes` is an answer too. Another 5xx or a transport error is `ERROR`. With `each`, `responses` instead. |
| `exec` | `argv`, `needs`, `timeout`, `palette`, `also`, `expr` | `exec`: `{exit, stdout, stderr}`. `stdout` is JSON when it parses. A tool that is not there is `NOT CHECKED`. With `palette`, also `response`. With `also`, also `also`, in the same shape. |
| `watch` | `expr` | `cluster`, `summary`, `events`, `elapsed` (seconds), on each reading |

The fields of a check of kind `palette`:

- `method` is `GET` or `POST`.
- `path` takes `{{ jq }}` templates.
- `body` is a jq program that gives the body of a POST. The default is `.entity`.
- `each` is a jq program that gives a list: see [A request for each item](#a-request-for-each-item).
- `refusal_codes` lists the codes of a 5xx that are an answer: see
  [A refusal at a 5xx](#a-refusal-at-a-5xx).
- `saves` names what Palette saves at the request, when the route saves
  something. The one value is `console-address`: at `validate/repave`, Palette
  can save the address of the console of a pack of virtual machines. The line of
  the report for the request ends with a comment that says it. Each
  `reproduce` line that sends the request ends with
  `{{ saves_comment("console-address") }}`: see
  [The reproduce lines](#the-reproduce-lines).

The fields of a check of kind `exec`:

- `argv` is the command and its arguments. Each word takes templates.
- `needs` lists the tools. The check looks for them on the PATH.
- `timeout` is in seconds. The default is 60.
- `palette` is a `path` with templates: see
  [A read of Palette before the command](#a-read-of-palette-before-the-command).
- `also` is the second command: see [A second command](#a-second-command).

jq is jaq 3, which runs the programs of jq 1.7 with a small number of
differences. Two to know: `false // x` gives `x`, so read a boolean with
`if has("k") then .k else true end`; and `IN(...)` is not defined, so test a
value against a list with `(["a", "b"] | index($v)) != null`. A program that
does not compile makes the rule `ERROR`, and the message names it. So does a
program that gives no output, or `null`: a path that is not in the world is a
wrong program, not a pass.

strata-preflight adds four filters to jq: `semver_numbers`, `count_of`, `palette_curl`, and
`saves_comment`.

`semver_numbers` reads the input text as a version, as `strata` does, and gives the numbers
that the version writes. `"4.10.3-rc.1"` gives `[4, 10, 3]`, and `"v4.10"` gives `[4, 10]`. A
text that is no version gives `null`. A rule that compares two versions reads both with it, and
compares the arrays. Do not split a version at the dots.

A smart tag such as `1.32.x` is no version. To keep its first two parts between the dots, write
`semver_numbers // split(".")`. A test of the catalog refuses a split at the dots in a rule
that reads `.version` or `.tag`, unless `semver_numbers //` comes first.

`count_of` writes the input count with a noun. `2 | count_of("key pair")` gives `2 key pairs`.
`1` gives `1 key pair`, and `0` gives `no key pairs`.

Write the noun in the singular. Use it for each count in a message. A test of the
catalog refuses `(s)` after a noun. A number that is not a whole number of zero or more
writes the number and the plural noun: `-1 | count_of("CPU")` gives `-1 CPUs`. A count that
Palette gives can be such a number, and a message must not fail on it. The input must be a
number.

A program with `semver_numbers` or `count_of` does not run in the `jq` tool. The report writes a
`reproduce` line for such a program with a definition of each filter in front. So the line runs
in the tool.

`palette_curl` gives the start of a `curl` to Palette, for a `reproduce` line.
`saves_comment("console-address")` gives the end of a line that sends a request at which
Palette saves something: a comment that says what it saves. A name that is not a value of
`saves` is an error. See [The reproduce lines](#the-reproduce-lines).

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
list, when a template fails, or when Palette does not answer one request.

`$item` is a variable of `path` and `body` only. The loader compiles each
program of a rule file that you give with `--probes`, with the variables that
the program has when it runs. It refuses `$item` in `each`, `expr`, `when`, an
`argv`, and a `reproduce` line, and in `path` or `body` when the check has no
`each`. It refuses a POST with `each` and no `body`: with none, the check sends
the entity one time for each item. The tests compile each program of the
catalog in the binary.

To send one request for several items, group the items in `each`.
`vsphere-compute-resources` sends one request for each compute cluster, and
lists the pools of that cluster in the item.

The report keeps each request in `requests[]`. `--verbose` prints one line for
each request. For a GET, the report adds one `curl` line for each request to its
`reproduce` lines. A rule with `reproduce` lines of its own for the engine gets no
such line, so no request has two lines. See
[The static tier and the live tier](../commands/preflight/tiers.md#a-rule-over-a-list).

## A refusal at a 5xx

Palette gives some refusals the type of a server fault: HTTP 500 with a `code`. The
route read the request and refused what it says. A check reads a 5xx as an error by
default, because a 5xx can also come from the gateway or from a fault of Palette.

A check of Palette names the codes of such a refusal in `refusal_codes`. A 5xx with one
of these codes is an answer in `.response`, with `ok: false`, as a 4xx is. A 5xx with
another code, or with no code, stays `ERROR`. A 401, a 403, and a 429 stay `ERROR`
with every code, because they say that the key or the rate failed.

The library of strata sends a request again after a 5xx, except a 5xx with a code that it lists
as a refusal. A code that it does not list makes the check read the refusal only after the
retries. So the loader refuses a code in `refusal_codes` that the library does not list.

```yaml
check:
  kind: palette
  method: POST
  path: /v1/clouds/aws/properties/validate
  refusal_codes: [AwsKeyPairNotFound, FeatureNotSupported]
  expr: '...'
```

`iaas-pod-identity` names `AwsKeyPairNotFound` and `FeatureNotSupported`. The property route of AWS
answers HTTP 500 with the first code for a key pair that the region does not hold. It answers
HTTP 500 with the second code for a Pod Identity account. A live run shows the first answer. No
live run shows the second answer. Without the codes, the rule was `ERROR` for these faults, and
never `FAILED`.

The backend keeps the answer of a request for the run. A rule that does not name the code
reads the same 500 as an error. A fixture follows the same rule for each status
that is not 2xx. An entry of `_responses` with a 401, a 403, or a 429 is `ERROR`. An entry with a
5xx status is an answer only when the check names its code.

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
| `eks-ami-type` | The AMI type of an EKS pool is an AL2023 type. | `UnsupportedAmiType` for the type `AL2_x86_64`, and `IsValidValue` for a type that is not known. `palette-validate` reports both refusals. See [The validate routes](../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate). |

A run with `--rule` and one of these ids is a `usage` error with exit `2`, because the
id is not in the catalog.

### The checks that wait for a call of the route

Each rule in this table copies a check that strata expects the route to answer.
The rule stays in the catalog until [`validate-route.sh`](../development/live-runs.md#validate-routesh)
shows that the route refuses the fault. Then the part in the second column
leaves the rule, and its fixtures leave with it.

The lab of the live example has cloud accounts of AWS and GCP only. A case of
another cloud has no live result, and the third column says why.

| Rule | The part that strata expects the route to answer | Why the rule waits |
| --- | --- | --- |
| `vsphere-instance-floor` | All of it: the CPU, the memory, the disk, and the missing instance type. | The lab has no cloud account of vSphere. |
| `edge-two-node` | The Kubernetes pack that is not K3s. | The lab has no cloud account of Edge. |
| `ssh-key-format` | The vSphere and Edge engines. | The lab has no cloud account of vSphere or Edge. |
| `azure-ssh-key` | The empty key. | The lab has no cloud account of Azure. |
| `pack-state` | The `disabled` and `deleted` states. | No body can carry the pack. Palette refuses a new profile with such a pack: see [route-bodies.sh](../development/live-runs.md#route-bodiessh). |
| `vsphere-static-ip-pool` | All of it. | The lab has no cloud account of vSphere. |
| `vsphere-controlplane-endpoint` | The empty host. | The lab has no cloud account of vSphere. |
| `edge-vip-multi-node` | The empty host. | The lab has no cloud account of Edge. |
| `azure-subnets-in-vnet` | The empty `vnetCidrBlock`. | The lab has no cloud account of Azure. |
| `ntp-servers` | The format of a server. | The lab has no cloud account of vSphere, Edge, or MAAS. |

### The gaps that preflight fills

These checks are not copies of an answer of the route. The route does not report
them, or they need more than the body of the create. They stay in the catalog.

| Rule | What stays |
| --- | --- |
| `edge-two-node` | The count of the control plane hosts, and the priority of each. |
| `ssh-key-format` | The MAAS engine. |
| `vsphere-controlplane-endpoint` | The mapping of the dynamic DNS. |
| `edge-vip-multi-node` | The count of the hosts. |
| `azure-subnets-in-vnet` | The containment of the subnet CIDRs in the CIDR of the vnet. |
| `ntp-servers` | The warning when the cluster names no server. |
| `pack-cloud-support` | All of it. No route refuses the fault: see [The validate routes](../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate). |
| `template-outside`, the `edge-host-*` rules | They read a document of the tenant. No validate route answers them. |
| `instance-offering`, `ssh-key-pair` | They read a list of the cloud account: the instance types of a region, and its key pairs. No route refuses the fault, and Palette owns the rule. For `ssh-key-pair`, the gap is an EKS cluster with a dynamic VPC or with no subnet id. |
| `static-subnets`, `sg-name-collision` | They read a list of the cloud account: the VPCs of a region with their subnets, and the security groups of a VPC. No route refuses the fault, and Palette owns the rule. |
| `aws-quotas`, `gcp-quotas`, `azure-vcpu-quota` | They read the `cpu` of the instance types of a region from Palette, and a quota from the cloud. No route of Palette lists a quota, and Palette owns the rule. |
| `palette-validate`, `pack-dependency`, `ecr-oci-images`, `worker-cpu-floor`, `eks-public-subnet-mapping`, `iaas-pod-identity`, `sts-account-not-assumable`, `gcp-properties` | They call a route, and copy no answer of it. |

## A read of Palette before the command

A rule of the cloud tier can read one list from Palette, and then run the tool of the cloud. Ask
Palette first: use this for the part of the answer that a route lists, and use the command for the
rest. The key `palette` of an `exec` check holds the `path` of a `GET` request:

```yaml
check:
  kind: exec
  needs: [aws]
  palette:
    path: '/v1/clouds/aws/regions/{{ .entity.spec.cloudConfig.region }}/instancetypes?cloudAccountUid={{ .entity.spec.cloudAccountUid }}&cpuGtEq=0&memoryGtEq=0'
  argv: [aws, service-quotas, get-service-quota, --service-code, ec2, --output, json]
  expr: '...'
```

The program of `expr` reads `.response`, as a check of Palette does, and `.exec`. The templates of
`argv` read the world alone. The check runs in this order:

1. With no key, the result is `NOT RUN`, and no command runs.
2. A tool in `needs` that is not there makes the result `NOT CHECKED`, and the check sends no request.
3. The check sends the request. A 4xx is an answer in `.response`. A transport error, a 5xx, a 401, a 403, or a
   429 is `ERROR`, and the report names the request. A read has no `refusal_codes`, so each 5xx is `ERROR`.
4. The command runs, and the program judges both answers.

The report shows the command in `ran`, and the request in `requests[]`. The rule keeps the tier
`cloud`, because it needs the tool.

## A second command

A rule of the cloud tier can run two commands. An example is the policies of a role, and the quota
that limits them. The key `also` of an `exec` check holds the words of the second command:

```yaml
check:
  kind: exec
  needs: [aws]
  argv: [aws, iam, list-attached-role-policies, --role-name, '{{ .account.spec.namePrefix }}nodes', --output, json]
  also: [aws, service-quotas, get-service-quota, --service-code, iam, --quota-code, L-0DA4ABF3, --region, us-east-1, --output, json]
  expr: '...'
```

The program of `expr` reads `.also` beside `.exec`. Both have the shape `{exit, stdout, stderr}`.
The templates of `also` read the world alone, as the templates of `argv` do. The two commands share
`needs` and `timeout`. The check runs in this order:

1. A tool in `needs` that is not there makes the result `NOT CHECKED`. The message names both
   commands.
2. The first command runs. Then the second command runs, also when the first fails.
3. The program judges both outputs.

A second command that fails is an answer, as the first command is. The program reads `.also.exit`
and decides: the rule `iam-role-policy-quota` uses the default quota when the read of the quota
fails. A command that cannot start is an `ERROR`.

The report shows both commands in `ran`, with `; ` between them. `answer` has the exit code of each
command in the same order, such as `exit 0; exit 254`. The lines in `reproduce` put the same exit
codes in the world. A fixture holds an output for each command under `_exec`, and a fixture that
lacks one makes the check `ERROR`.

Here the second command of `iam-role-policy-quota` fails with exit 254. A script named `aws` in
`tools/` answers for the tool. The rule uses the default quota of 20, and the message says so:

```text
{{#include ../output/preflight-second-command.txt}}
```

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
  "routeCloud": "edge-native",        the cloud type of the engine in a route
  "edgeHosts": [ {pool, host} ],      each edge host of each pool, with the poolConfig of its pool
  "packFacts": { "<packUid>": ... }, what strata derives for each pack in packs
  "validateBody": { metadata, spec }  the body that the checks send to the validate route
}
```

`accountKind` is the kind of the cloud account of the engine. `eks` uses an
account of kind `aws`, `gke` one of kind `gcp`, and `aks` one of kind `azure`.
`routeCloud` is the cloud type of the engine in a route of Palette: the engine
itself, and `edge-native` for `edge`. A rule reads these two fields, and does not
map the engine itself.

`packFacts` holds three fields for each pack in `packs`, from the rules of strata for a pack
that a profile pins. `supportsCloud` is true when the cloud types of the pack include
`routeCloud`. A pack with no cloud type supports each cloud. It is `null` for a world with no
engine.

`state` is the `system_state` of the pack, or empty text. `retired` is true for
`disabled` and `deleted`. A rule reads these fields, and does not write the rules again.

`edgeHosts` is the list of the edge hosts of the entity, in the order of the
pools. Each item has `pool`, the `poolConfig` of the pool, and `host`, the host.
A rule that reads each host gives `each: .edgeHosts`, and writes no program of
its own. `validateBody` is the entity, with the suffix `-preflight` on the name
for `cluster:UID`: the validate route refuses the name of a cluster that Palette
holds. A rule that asks the validate route gives `body: .validateBody`. A run
sends the request one time, however many rules give it.

A world with a `clusterUid` must put the new name in `metadata`. If its
`entity`, or the `metadata` of its `entity`, is neither an object nor `null`, the
world is an `input` error. The name must not replace that value. A world with no
`clusterUid` sends its entity as it is, and the validate route answers for it.

A record is a `strata-snapshot/1` document: `source.{uid, name, version,
cloudType}` and `packs[].{layer, name, version, tag, packUid, registryUid,
values, valuesCanon, hash}`. `valuesCanon` is the values YAML as JSON, so a
program reads a value of a pack with a path. The fields of a cloud account that
hold a secret are `<redacted>`. The entity of the world holds the value of each
variable as the source gives it, and that value can be a secret. `--world`
writes the world as `world.json`: see
[The report on disk](../commands/preflight/report.md#the-report-on-disk).

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

A fixture, its base, and its `_patch` are JSON objects. If one is not, the load
gives an `input` error. The same error comes for a fixture with a `clusterUid` and
an `entity`, or a `metadata`, that is neither an object nor `null`.

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
company.

`4.10.3` and `4.10` are one release for the counts.
When a report names a release, only the runs of that release count. A run
with no release (`release: ""`) is a run of an older release, so it counts for
no release. See [The runs of one release](../commands/preflight/promotion.md#the-runs-of-one-release).
`strata preflight --schema ledger` prints the JSON Schema.
