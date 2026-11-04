# The observer

`strata-observer` is a service that runs in one cluster. It runs the `strata`
binary on two timers, keeps the last answer of each, and shows the answers on a
page. It reads the records of your team, and it asks Palette what it holds now.
It writes nothing to Palette.

Your pipeline runs strata when a change happens: a pull request, a merge, a
night. The observer asks the same questions all day. A person who opens the page
sees an answer that is a few minutes old. The observer holds no rule of its own.
Each answer is the answer of a `strata` command.

<div class="playground-demo">

`just observer-playground` runs the observer on this machine against the
stand-in for Palette, and serves the page at `http://127.0.0.1:8080`. It needs
Go, and it costs nothing. `just playground observer-2` serves the same tenant
alone, so that you can run the commands below.

</div>

## The five questions

| Question | Where the page answers it | What the observer runs |
| --- | --- | --- |
| Does Palette still have the version that I recorded? | A finding of the kind `published-changed`, `overwritten`, `renamed`, or `gone`. | `strata verify` |
| Does this cluster run its release? | A finding `cluster-differs` or `image-id-changed`. The cluster copy of each profile. | `strata snapshot` and `strata compare` |
| Who changed it? | The field `who` of a finding. | `strata verify --who` |
| Does the registry have newer versions of what we pin? | The list of newer versions. | `strata behind` |
| What changed between two pack versions? | The detail of a newer version. | `strata compare pack:...` |

[The checks of the observer](../development/observer.md) gives each command, each
finding, and each state of the page.

## What the service does

- **A content cycle** runs every 15 minutes. It checks each record against
  Palette, reads the cluster, and compares each record with the copy that the
  cluster holds.
- **A versions cycle** runs every 6 hours. It asks which pins have newer versions
  in the registry, and compares each pin with its newest version.
- Each cycle starts when the process starts. One `strata` process runs at a time.
- A cycle writes its files to a volume in memory, and removes them at its end.
- A cycle that fails keeps the last good answer, and the page says how old it is.

The records are the files that your pipeline writes. The observer compares them
with Palette, and it never writes a record. See [Install](#install).

## Run it once

`strata-observer once` runs one content cycle and one versions cycle, and prints
the summary. The tenant below is the tenant of the playground chapter `observer`. The
cluster `payments` runs the profiles `aws-stack@1.0.0` and `platform-addons@1.0.0`.
A person edited `aws-stack@1.0.0` through the API after the team recorded it.

```text
{{#include ../output/observer-once.txt}}
```

The finding `published-changed` says that Palette holds other content than the
record. It names the pack, and `who` names the person and the time of the write.
`clusterCopy` of the profile is `same`, because `payments` has not taken the
update. Both facts are on the page of the profile.

The summary also lists the pins that have newer versions. `impact` is the impact
of the move to the newest version, as `strata behind` gives it:

```text
{{#include ../output/observer-newer.txt}}
```

The summary holds the status of each profile, and the limits of the check:

```text
{{#include ../output/observer-limits.txt}}
```

`imagesNamedByATag` counts the images that a tag names, `helmPacks` counts the
packs of type `helm`, and `maskedPaths` counts the values that Palette masks. Each
of them is a place where `OK` says less: see [What the observer does not
see](#what-the-observer-does-not-see).

The log goes to standard error. A line holds a verdict, a duration, and an
error kind, and never a value. The example drops the time and the duration, because they
change at each run:

```text
{{#include ../output/observer-log.txt}}
```

A cluster that differs from its release gives the finding `cluster-differs`. The
cluster `ledger` runs `aws-stack@1.1.0`, and a person changed its copy on the
cluster. The observer checks each record, so the run for `ledger` also shows the edit of
`aws-stack@1.0.0`:

```text
{{#include ../output/observer-ledger.txt}}
```

### Exit codes

| Exit | Meaning |
| --- | --- |
| `0` | No finding exists, and the two cycles ended well. |
| `1` | A finding exists, as with `strata verify`. The other cycle can have failed: `failedRuns` says so. |
| `2` | No finding exists and a cycle failed, or a flag is wrong. |

### Options

```bash
strata-observer once --cluster-uid UID --records DIR --work DIR --state DIR
strata-observer serve --cluster-uid UID --records DIR --work DIR --state DIR [--listen ADDR]
```

`serve` takes the flags of `once` and the flag `--listen`. It keeps the answers, and it
serves them on a loopback address. [Run it](../development/observer-api.md#run-it) gives
`--listen` and the exit codes of `serve`. [The API of the observer](../reference/observer-api.md)
lists the routes.

| Flag | Default | What it does |
| --- | --- | --- |
| `--strata` | `strata`, on `PATH` | Names the binary. The image has `/usr/local/bin/strata`. |
| `--cluster-uid` | required | Names the cluster. The name comes from the read of the cluster. |
| `--records` | required | The directory of `*.json.gz` records and `meta.json`. |
| `--work` | required | A volume in memory for the files of a cycle. |
| `--state` | required | A directory for `state.json`. It holds times and counts, and no value. |
| `--content-every` | `15m` | The period of the content cycle. |
| `--versions-every` | `6h` | The period of the versions cycle. |

## Install

The observer needs a namespace, two settings, a key, the records, and the
manifests. Make the settings before the Deployment, because the pod does not
start without the ConfigMap and the key. Copy `deploy/observer/` into the
repository of your team. The steps below run from that repository.

1. **Make the namespace.** It enforces the Pod Security standard `restricted`.

   ```bash
   kubectl apply -f deploy/observer/namespace.yaml
   ```

2. **Make the ConfigMap.** It names the cluster, the API of Palette, and the project.

   ```bash
   kubectl -n strata-observer create configmap strata-observer \
     --from-literal=cluster-uid=<cluster uid> \
     --from-literal=palette-api=https://api.spectrocloud.com \
     --from-literal=project-uid=<project uid>
   ```

3. **Make the Secret of the key.** Send the key on standard input. It then never
   appears in an argument or in a file.

   ```bash
   printf %s "$PALETTE_API_KEY" \
     | kubectl -n strata-observer create secret generic strata-observer-key \
         --from-file=api-key=/dev/stdin
   ```

4. **Write the records.** The records script writes four Secrets on standard output.
   Use `--server-side`: a client-side apply copies each Secret to an annotation
   that has a limit of 256 KiB.

   ```bash
   scripts/ci/observer-records.sh --revision "$GIT_SHA" releases/*.json \
     | kubectl apply --server-side -f -
   ```

5. **Apply the manifests.** Write a `kustomization.yaml` that names the folder and
   the tag of the image. The tag is the tag of the release of strata.

   ```yaml
   apiVersion: kustomize.config.k8s.io/v1beta1
   kind: Kustomization
   resources:
     - deploy/observer
   images:
     - name: ghcr.io/como-technologies/strata-observer
       newTag: <release tag>
   ```

   ```bash
   kubectl apply -k .
   ```

6. **Open the page.** The service listens on the loopback address of the pod. A
   person reaches it with a port-forward, then opens `http://127.0.0.1:8080`.

   ```bash
   kubectl -n strata-observer port-forward deploy/strata-observer 8080
   ```

The image is in a private repository. Name a pull Secret in `imagePullSecrets`.
Its token needs only the scope `read:packages`. [The image and the manifests of
the observer](../development/observer-deploy.md) gives the files, the signature
check, and the role that lets a person open the page.

The records script shows what it writes. Each Secret holds some records, and the
first Secret also holds `meta.json`. The step shows the names, and cuts the
content:

```text
{{#include ../output/observer-records.txt}}
```

A Secret with no record is empty. The next apply then removes the old content.
The pipeline of your team runs the script at each merge that changes a record, so
the records are the files that the team approved.

A live run applied these steps to a cluster that Palette built. The pod passed the
`restricted` standard and became Ready. Its first cycle reached Palette, so a
workload cluster reaches the API. A user with only the reader role opened the page
with the port-forward. A pod of another namespace could not connect. After a
restart, the page worked again.

## Install with a profile in Palette

You can put the manifests in an add-on profile, and attach the profile to the
cluster. The profile holds the rendered manifests as one manifest layer, and Palette
applies them to the cluster. Make the namespace, the ConfigMap, the Secret of the
key, and the records with `kubectl` first, as in [Install](#install). The profile
holds no key, because a profile variable is in clear in the copy of the cluster.

A live run shows what Palette does with the layer:

- **The nodes stay.** Palette replaces no node when it applies the layer.
- **The other profiles stay.** Each other profile of the cluster still agrees with
  its record.
- **The profile of the observer has no record.** The page lists it under "Not
  checked", and it shows no finding for it.
- **A removal is clean.** When you remove the profile from the cluster, Palette deletes
  the Deployment.

An edit of the profile that drops an object from the manifest leaves the object on
the cluster. To remove it, delete it with `kubectl`. See [An object that an edit
drops from a manifest](../internals/palette/clusters.md#an-object-that-an-edit-drops-from-a-manifest).

## First-seen records

You can start with no approved record. `--first-seen` records what each profile
of the cluster holds now:

```bash
scripts/ci/observer-records.sh --first-seen <cluster uid> | kubectl apply --server-side -f -
```

The script writes the baseline in `meta.json`:

```text
{{#include ../output/observer-first-seen.txt}}
```

This bends the rule of [Where each part belongs](../boundary.md#the-rule-that-decides):
a record in Git holds the content that the team approved. A first-seen record holds
the content that Palette gave at that time. No person approved it. If somebody edited
a profile before that time, the record holds the edit, and the observer reports no
change.

Each screen of the page says "No person approved these records" while the baseline
is `first-seen`.

To move to approved records, record each profile with `strata snapshot`. Let a review
approve the files, and commit them. Then run the script with `--revision`. `meta.json`
then says `approved`, and the screens drop the notice.

## What the observer does not see

Each limit below is a fact of this release. The page shows the numbers of `limits`,
and it never reads "No change" for a pack that it did not check.

1. **A tag that moves at its source.** A record names a tag. Strata reads no
   registry, and the service reads no pod in this release. So a rebuilt tag shows no
   change, and the profile stays `ok`. A pin in the pack, or the copy step of your
   pipeline, closes this. See [What OK does not cover](../commands/verify.md#what-ok-does-not-cover)
   and [A vendor rebuilds an image under one tag](../use-cases/image-tag.md).
2. **A change on the cluster with `kubectl edit`.** The service compares what
   Palette holds. Palette does not report such an edit, and the service does not
   either.
3. **What the workload runs.** "This cluster's copy in Palette" is the copy that
   Palette holds for this cluster. It is not the objects that run in the cluster.
4. **The value of a cluster variable.** It is outside the root, so a change of one
   does not show. The service also never shows the value: see
   [The API of the observer](../reference/observer-api.md#what-no-answer-carries).

## What you give the observer

1. **An API key in the cluster.** This is the first part of strata that holds a
   credential in a workload cluster. A key has the reach of its user, and not the
   reach of this cluster. Whoever can read the Secret `strata-observer-key` has that
   reach. Use a dedicated user with the least role. [The least role of the
   key](#the-least-role-of-the-key) says which role.
2. **Never a profile variable for the key.** A profile variable is in clear in the
   copy of the cluster. So the key stays there in clear. Make the Secret with `kubectl`.
3. **Egress to Palette** from the cluster. The network policy of the observer allows
   DNS and HTTPS only. A team can narrow the HTTPS rule to the address of Palette.
   A pod on a cluster that Palette built reaches the API, and it ends its first cycle.
4. **Requests.** A cycle sends `GET` requests and no other method. A content cycle
   reads the cluster and each profile that it runs. For a record that drifted, it
   reads the audit log too. A versions cycle reads the registries and the packs that
   the records pin. [The requests of a cycle](#the-requests-of-a-cycle) gives the
   counts of both cycles.
5. **The page holds pack values.** The detail tier shows the old and the new value of
   a pack. A secret typed into a value that the schema does not mark passes through.
   `create` on `pods/portforward` is the gate: whoever has that verb opens the page.
   The reader role gives no Secret and no `exec`.

### The requests of a cycle

A live run counted the requests of each cycle on a cluster with two profiles and
five packs. The counts grow with the number of profiles and packs.

| Command | At the baseline | With a finding |
| --- | --- | --- |
| A content cycle | 12 | 13 |
| `strata behind`, the first command of a versions cycle | 13 | 13 |
| The moves of the first versions cycle after a start | 28 | 28 |

The one extra request reads the audit log. `verify --who` sends it for a record
that drifted. A later versions cycle sends the 13 requests of `behind`, and
the requests of the moves that are new. [What the live example
verifies](../development/live-runs.md#observersh) holds the counts.

### The least role of the key

The key reads the cluster, its profiles, the registries, and the audit log. The code
of Palette shows that **Project Viewer** in the project of the cluster gives every
answer. The role is read-only, and the observer needs no tenant role, because each
request names the project. [The role that a key needs](../internals/palette/clusters.md#the-role-that-a-key-needs)
lists each permission and each refusal.

- **Keep the audit log in the role.** The observer reads the audit log to tell who
  changed a version. Cluster Viewer alone does not hold `audit.list`. A custom role
  needs `audit.list` beside `cluster.get`, `clusterProfile.get`,
  `packRegistry.get`, and `packRegistry.list`.
- **Make the user for the observer alone.** Give it no other role, and make the key
  in the name of that user.
- **This is the reading of the code of Palette.** No run has used a key with that
  role yet. `just live-observer-role` sends each request of the cycles with a second
  key, and [observer-role.sh](../development/live-runs.md#observer-rolesh) lists the
  checks. Making the user is for brett, so the live confirmation is left for brett
  until a user with the role exists.

## `re-baseline needed`

A new minor version of strata can move a root with no change of content. The
finding then has `reBaselineNeeded`, and the page shows `re-baseline needed` beside
the drift. The finding stays. The observer never writes a record. Write the records
again with the new version of strata. See [Versions and
compatibility](../reference/compatibility.md) for each change that moves a root.

## Next

- [The API of the observer](../reference/observer-api.md): the routes, the two
  tiers, and the fields that never leave.
- [The observer finds an edit in place](../use-cases/observer.md): the problem, the
  commands, and the fix.
- [The checks of the observer](../development/observer.md): the code, for a
  maintainer.
