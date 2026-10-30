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
| `palette` | P | One request to Palette. | `palette` |
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
| `palette` | `method` (`GET`, `POST`), `path` (with `{{ jq }}` templates), `body` (a jq program that gives the body of a POST; default `.entity`), `expr` | `response`: `{ok, status, body}`. A 4xx is an answer with `ok: false` and its `V1Error` body; a 204 is `ok: true` with a `null` body. A 5xx or a transport error is `ERROR`. |
| `exec` | `argv` (each with templates), `needs` (the tools, checked on the PATH), `timeout` (seconds, default 60), `expr` | `exec`: `{exit, stdout, stderr}`. `stdout` is JSON when it parses. A tool that is not there is `NOT CHECKED`. |
| `watch` | `expr` | `cluster`, `summary`, `events`, `elapsed` (seconds), on each reading |

jq is jaq 3, which runs the programs of jq 1.7 with a small number of
differences. Two to know: `false // x` gives `x`, so read a boolean with
`if has("k") then .k else true end`; and `IN(...)` is not defined, so test a
value against a list with `(["a", "b"] | index($v)) != null`. A program that
does not compile makes the rule `ERROR`, and the message names it. So does a
program that gives no output, or `null`: a path that is not in the world is a
wrong program, not a pass.

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
  "clusterUid": "<uid>" | null        for cluster:UID
}
```

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
says no key. A fixture can say `_base` and hold only what differs:

```json
{ "_base": "clean.json",
  "_note": "the one worker pool has size 0",
  "_patch": { "entity": { "spec": { "machinepoolconfig": [ { "poolConfig": { "name": "worker-pool", "size": 0 } } ] } } },
  "_delete": [ "/account/spec/sts" ] }
```

`_patch` merges as a change of a cluster merges: an object key by key, a pool
by its name, a profile by its uid, a variable by its name; another list
replaces. `_delete` removes each JSON pointer. The test of the catalog runs
every fixture, and refuses a fixture that no rule names.

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
      - {rule: gke-pool-multiple-3, result: caught}    # the run made the fault, and the check found it
      - {rule: gke-api-off, result: passed}            # a clean build, and the check passed
      - {rule: eks-one-zone, result: missed}           # the run made the fault, and the check passed: wrong
      - {rule: worker-cpu-floor, result: false-fail}   # a clean build, and the check failed: wrong
```

The binary carries the ledger of the lab. `--ledger FILE` adds the runs of a
company. `4.10.3` and `4.10` are one release for the counts.
`strata preflight --schema ledger` prints the JSON Schema.
