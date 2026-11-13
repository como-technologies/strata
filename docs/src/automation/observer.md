# The observer

`strata-observer` is a service that runs in one cluster. It runs the `strata`
binary on two timers, keeps the last answer of each, and shows the answers on a
page. It reads the records of your team, and it asks Palette what it holds now.
It writes nothing to Palette. A click on the page sends one question to Palette:
see [Ask Palette about a move](#ask-palette-about-a-move).

Your pipeline runs strata when a change happens: a pull request, a merge, a
night. The observer asks the same questions all day. A person who opens the page
sees the answer of the last content cycle. A cycle starts every 15 minutes, with a
random delay of 10 percent at most. The observer holds no rule about a profile.
Each answer about a profile is the answer of a `strata` command.

A team can also let the observer read the pods of the cluster. It then shows
which digest each tag runs. That read is off by default: see [The pods of the
cluster](#the-pods-of-the-cluster).

<div class="playground-demo">

`just observer-playground` runs the observer on this machine against the
stand-in for Palette, and serves the page at `http://127.0.0.1:8080`. It needs
Go, and it costs nothing. `just playground observer-2` serves the same tenant
alone, so that you can run the commands below.

</div>

## The seven questions

| Question | Where the page answers it | What the observer runs |
| --- | --- | --- |
| Does Palette still have the version that I recorded? | A finding of the kind `published-changed`, `overwritten`, `renamed`, or `gone`. | `strata verify` |
| Does this cluster run its release? | A finding `cluster-differs` or `image-id-changed`. The cluster copy of each profile. | `strata snapshot` and `strata compare` |
| Who changed it? | The field `who` of a finding. | `strata verify --who` |
| Does the registry have newer versions of what we pin? | The list of newer versions, and the button "Ask Palette" on each move. Each pack names the newer profile version that carries a newer version of it. | `strata behind`. The button runs `strata behind --pin`. |
| Does Palette have a newer version of the profile that we run? | The row of the profile: "Newer profile version", and its count on the tile "Newer versions". The card "Move to" of the profile page, and the choice "Record → Newest version" of the dropdown "Compare" of a pack page. | `strata behind`, and `strata compare` of the record and the newest version |
| What changed between two pack versions? | The detail of a newer version, with the text of the diff. | `strata compare pack:...` |
| Which digest does each tag run as on this cluster? | The Images screen, and a finding `image-moved` or `image-split`. Only with `--read-pods`. | No `strata` command. It lists the pods through the Kubernetes API. |

[The checks of the observer](../development/observer.md) gives each command, each
finding, and each state of the page.

## What the service does

- **A content cycle** runs every 15 minutes. It checks each record against
  Palette, reads the cluster, and compares each record with the copy that Palette
  holds for the cluster. With `--read-pods`, it lists the pods first.
- **A versions cycle** runs every 6 hours. It asks which pins have newer versions
  in the registry, and compares each pin with its newest version. It also asks which
  profile versions of the records have a newer published version, and compares the
  record of each one with the newest version.
- **A click** on "Ask Palette" runs `strata behind` for one pack. No timer starts it.
- **A click on the pill of a cycle** opens its menu. "Check the content now" or "Read
  the versions now" runs that cycle at once, and not at the end of its period.
- **A change of the records** starts a content cycle at once. So the page shows the
  records of your pipeline soon after the publish.
- Each cycle starts when the process starts. At most four `strata` processes run at
  a time.
- A cycle writes its files to a volume in memory, and removes them at its end.
- A cycle that fails keeps the last good answer, and the page says how old it is.

The records are the files that your pipeline writes. The observer compares them
with Palette, and it never writes a record. See [Install](#install).

## The baseline and the check

The baseline is the set of records that your team approved. The observer checks
Palette against it. Only a merge moves the baseline.

1. **Record a version.** A person runs `strata snapshot` on a profile version. Then
   the person opens a pull request with the record.
2. **Merge the record.** A review approves the file. The merge moves the baseline.
3. **Write the records.** The pipeline runs `scripts/ci/observer-records.sh` with
   `--revision`, and applies the four Secrets. See [Install](#install).
4. **Check.** The two cycles check Palette against the records. A difference opens a
   finding on the page.
5. **Close the finding.** Publish a new version with the change through review, and
   record it. Do not repair the edited version in place: that is a second edit. The
   finding also clears when the published version goes back to its record.

The observer never writes a record. An edit in Palette moves no baseline: it opens a
finding. To try the observer with no approved record, see [First-seen
records](#first-seen-records).

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
of the move to the newest version, as `strata compare` gives it:

```text
{{#include ../output/observer-newer.txt}}
```

The tenant also holds `aws-stack` 1.1.0, a newer published version of the
profile `aws-stack@1.0.0`. The row of the profile holds it in `newerVersion`, with
the time that Palette created it. `new` is true for a version of the last 7 days:

```text
{{#include ../output/observer-profile-version.txt}}
```

The page shows the two kinds of newer version with different words. The row of a
profile says "Newer profile version `aws-stack@1.1.0`" for a newer version of the
profile itself. A pack with a newer version shows the badge "Newer", and the move
of the pack. For both, the status of the profile is `newer`, unless it is `drift`.

The tile "Newer versions" counts the packs, and its last line counts the profiles with a
newer version, as `counts.newerProfileVersions`. The filter "Newer" of the
profiles holds both kinds. A newer profile version gives the event
`NewProfileVersion` at the time of its create. When strata cannot read the versions
of a profile, the row names no newer profile version.

The versions cycle also compares the record with the newer version, as
`strata compare record.json aws-stack@1.1.0` does. `newerVersion.move` holds the
impact level and the flags of the move. Then it holds each pack that the move
changes, adds, or removes, with its two versions and `valuesChanged`:

```text
{{#include ../output/observer-profile-move.txt}}
```

The page of the profile shows the move in the card "Move to", with the impact level
and the five flags. Its table lists each pack whose version changes, as `old → new`,
each pack that the move adds or removes, and each pack whose values change. A pack
of the record links to its page. There, the dropdown "Compare" has the choice
"Record → Newest version", with the full diff of the pack, as "Record → Published
now" has. The card "Check this yourself" gives the command of the same compare. When
the compare fails, the card says that strata could not compare the two, and the
dropdown has no such choice.

The summary also says how far the change of each pack went. Each row of the newer
versions holds `carriers`, as `strata behind` gives them. For each profile of the
row, a carrier names the newer profile version that carries a newer version of the
pack:

```text
{{#include ../output/observer-carried.txt}}
```

Read the chain from the pack to the cluster:

1. **The pack.** The row names the version that the record pins, and the newest
   version in the registry.
2. **The profile version.** `version` of a carrier is the newest version of the
   profile that pins a newer version of the pack. `pack` is that version of the
   pack. A carrier with no `version` says that no newer version of the profile pins
   a newer version of the pack. So the change stopped at the registry. `unread`
   says that strata could not read a newer version.
3. **The cluster.** The record of the profile is the version that the definition of
   the cluster pins. When a newer profile version carries the pack, the definition
   of the cluster is the step that waits.

The page shows the same words. The slab of a pack that is behind says "carried by
`aws-stack@1.1.0`", or "carried by no newer profile version". A version of the pack
below the newest one comes first, as in "1.65.0 carried by `aws-stack@1.1.0`". The
row of the pack in the table of the newer versions has the same line under the name
of the pack.

A pack row of the profile holds the same carrier in `carried`. A profile with no
newer version has no carrier, so its rows have no such line. See
[The newer profile version that carries a newer pack](../commands/behind.md#the-newer-profile-version-that-carries-a-newer-pack).

A pack page has two sides of a newer version in the dropdown "Compare": "Pack A →
pack B" and "Record → Newest version". When the pack changes, these sides show the
badge "Newer". A newer version is no drift, so they never show the red badge
"Changed".

The summary holds the status of each profile, and the limits of the check:

```text
{{#include ../output/observer-limits.txt}}
```

`imagesNamedByATag` counts the images that a tag names, `helmPacks` counts the
packs of type `helm`, and `maskedPaths` counts the masked paths in each diff. Each
of them is a place where `OK` says less: see [What the observer does not
see](#what-the-observer-does-not-see).

The log goes to standard error. A line holds a verdict, a duration, and an
error kind, and never a value. The example shows only the lines of the cycles, and
it drops the time and the duration, because they change at each run:

```text
{{#include ../output/observer-log.txt}}
```

A cluster that differs from its release gives the finding `cluster-differs`. The
cluster `ledger` runs `aws-stack@1.1.0`, and a person changed the copy that Palette
holds for the cluster. The observer checks each record, so the run for `ledger` also
shows the edit of `aws-stack@1.0.0`:

```text
{{#include ../output/observer-ledger.txt}}
```

### A version that the cluster no longer runs

A cluster moves to the next version of a profile, and the record of the old version
stays. The seed keeps its first-seen record, and a pipeline can publish a record for
each version. The observer still checks that record against Palette. The cluster
`payments` does not run `aws-stack@1.1.0`, so the summary above gives its record the
status `not-on-this-cluster`, the word of its `clusterCopy`.

- **A status of its own.** A record of a version that the cluster does not run is
  `not-on-this-cluster`. A finding wins: on `ledger`, the edit of `aws-stack@1.0.0`
  stays a drift.
- **No newer version.** The record and its packs have no newer version, because the
  cluster does not run the version. No row of the newer versions, no event, and no alert
  names the record.
- **The counts.** `counts.profiles` does not count the record, so `payments` has 2
  profiles for 3 records. The filters of the table of the profiles, the tile
  "Profiles", and the tile "Newer versions" do not count it.
- **The page.** A fold under the table of the profiles lists the record, with the
  filter "All" only. The fold says "1 record of a profile version that this cluster
  does not run". On the page of the version, no pack says "newer" or "current", and
  no card "Move to" shows. The row has the badge "Not on this cluster", and the
  message "This cluster does not run this version."

### Exit codes

| Exit | Meaning |
| --- | --- |
| `0` | No finding exists, and the two cycles ended well. |
| `1` | A finding exists, as with `strata verify`. The other cycle can have failed: `failedRuns` says so. |
| `2` | No finding exists and a cycle failed, or a flag is wrong. |

### Options

```bash
strata-observer once --cluster-uid UID --records DIR --work DIR --state DIR [--read-pods [--kube-dir DIR]] [--alerts FILE]
strata-observer serve --cluster-uid UID --records DIR --work DIR --state DIR [--read-pods [--kube-dir DIR]] [--alerts FILE [--digest-every DURATION]] [--listen ADDR]
```

`serve` takes the flags of `once` and the flag `--listen`. It keeps the answers, and it
serves them on a loopback address. [Run it](../development/observer-api.md#run-it) gives
`--listen` and the exit codes of `serve`. [The API of the observer](../reference/observer-api.md)
lists the routes.

| Flag | Default | What it does |
| --- | --- | --- |
| `--strata` | `strata`, on `PATH` | Names the binary, as a path or as a name on `PATH`. A relative path starts at the directory where the observer starts. The image has `/usr/local/bin/strata`. |
| `--cluster-uid` | required | Names the cluster. The name comes from the read of the cluster. |
| `--records` | required | The directory of `*.json.gz` records and `meta.json`. |
| `--work` | required | A volume in memory for the files of a cycle. |
| `--state` | required | A directory for `state.json`. It holds times and counts, and no value. |
| `--content-every` | `15m` | The period of the content cycle. |
| `--versions-every` | `6h` | The period of the versions cycle. |
| `--read-pods` | off | Reads the pods of the cluster at the start of each content cycle. With no flag, the observer makes no Kubernetes call. The process exits 2 when `KUBERNETES_SERVICE_HOST` or `KUBERNETES_SERVICE_PORT` is not set. |
| `--kube-dir` | `/var/run/secrets/kubernetes.io/serviceaccount` | Names the directory with the files `token` and `ca.crt` of the projected volume. It has an effect only with `--read-pods`. |
| `--alerts` | off | Names the file of the alert rules. `serve` sends the alerts. `once` only checks the file and sends nothing. With no flag, nothing sends: see [Alerts](#alerts). |
| `--digest-every` | `168h` | The period of a digest of the alerts. It has an effect only with `--alerts`. |
| `--alerts-allow-http` | off | Allows an `http` address for a webhook. The tests of the observer use it. Do not use it on a cluster. |

## Install with Helm

The chart `strata-observer` installs the observer, its roles, and a job that writes
the first-seen records. You need a kubeconfig with admin rights on the cluster, and
an API key with the role Project Viewer in the project of the cluster. You need no
tenant role.

1. **Make the namespace.** It enforces the Pod Security standard `restricted`.

   ```bash
   kubectl create namespace strata-observer
   kubectl label namespace strata-observer \
     pod-security.kubernetes.io/enforce=restricted
   ```

2. **Make the Secret of the key.** Send the key on standard input, so that it is in
   no argument and no file.

   ```bash
   printf %s "$PALETTE_API_KEY" \
     | kubectl -n strata-observer create secret generic \
         strata-observer-key --from-file=api-key=/dev/stdin
   ```

3. **Install the chart.** The address of the page of the cluster in Palette holds
   the uid of the cluster and the uid of its project.

   ```bash
   helm install strata-observer \
     oci://ghcr.io/como-technologies/charts/strata-observer \
     --version 0.33.0 -n strata-observer \
     --set cluster.uid=<cluster uid> \
     --set palette.projectUid=<project uid>
   ```

4. **Open the page.** Then open `http://127.0.0.1:18080`.

   ```bash
   kubectl -n strata-observer port-forward deploy/strata-observer 18080:8080
   ```

The job `strata-observer-seed-1` writes the first-seen records when the Secrets of
the records hold none, and the first cycle reads them. It never replaces records that
a pipeline published. `kubectl -n strata-observer logs job/strata-observer-seed-1`
shows what it did.

| Value | Default | Function |
| --- | --- | --- |
| `cluster.uid`, `palette.projectUid` | none, required | The cluster and its project. |
| `palette.api` | `https://api.spectrocloud.com` | The Palette address. |
| `palette.keySecret` | `strata-observer-key` | The Secret with the key `api-key`. |
| `image.pullSecret` | empty | A Secret that pulls the image. The image is public, so set it only for a copy in a registry with a login. |
| `image.tag` | the version of the chart | The tag of the image. |
| `contentEvery`, `versionsEvery` | `15m`, `6h` | The period of each cycle. |
| `readPods` | `false` | The read of the pods: see [The pods of the cluster](#the-pods-of-the-cluster). |
| `seed.enabled` | `true` | The job that writes the first-seen records. |
| `records.create` | `true` | The chart makes the four Secrets of the records, empty. Set `false` when a pipeline made them first. |
| `readerGroup` | empty | A group that gets the role that opens the page. |
| `publisher.enabled` | `false` | A service account for a pipeline, which can get and update the Secrets of the records. See [Approve the records with a pipeline](#approve-the-records-with-a-pipeline). |

The chart sets no value of a key or a token. The key stays a Secret that you make
with `kubectl`, so that no release and no profile holds it.

## Install

This section installs the same objects with `kubectl` and the manifests of
`deploy/observer/`, for a team that does not use Helm. The observer needs a
namespace, a ConfigMap with three settings, a Secret of the key, the records, and
the manifests. Make the settings first, because the pod does not start without the
ConfigMap and the key.

Get `deploy/observer/` and `scripts/ci/observer-records.sh` from the repository of
strata, at the tag of your release. Copy both into the repository of your team. The
steps below run from that repository. They need `jq`, `gzip`, and `base64`.

To try the observer on a test cluster, use one command instead: see [A test
cluster](../development/live-example.md#a-test-cluster).

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

4. **Write the records.** The folder `releases/` holds one record for each profile
   version that the cluster runs. The records script writes four Secrets on standard
   output. It refuses a record of a cluster and two records of one profile version.
   Use `--server-side`: a client-side apply copies each Secret to an annotation that
   has a limit of 256 KiB. The glob `releases/*.json` fits a folder with the records of
   one project.

   ```bash
   scripts/ci/observer-records.sh --revision "$GIT_SHA" releases/*.json \
     | kubectl apply --server-side -f -
   ```

   [The everyday loop](../loop.md#the-pipeline-publishes) keeps a folder for each version,
   with the records of `dev` and of `prod` in it. Its glob is `releases/*/dev.json`, so it
   differs from the glob above.

5. **Apply the manifests.** Write a `kustomization.yaml` that names the namespace, the
   folder, and the tag of the image. The tag is the tag of the release of strata. The line `namespace` is part of the step. An object that your file makes,
   such as the ConfigMap of the alerts, gets no namespace without it. `kubectl apply -k .`
   then puts that object in the namespace of your context, and the pod does not start.

   ```yaml
   apiVersion: kustomize.config.k8s.io/v1beta1
   kind: Kustomization
   namespace: strata-observer
   resources:
     - deploy/observer
   images:
     - name: ghcr.io/como-technologies/strata-observer
       newTag: <release tag>
   ```

   ```bash
   kubectl apply -k .
   ```

6. **Let your team open the page.** The role `strata-observer-reader` gives `create`
   on `pods/portforward`. It gives no Secret and no `exec`. Bind it to a group of
   your team.

   ```bash
   kubectl apply -f deploy/observer/reader-role.yaml
   kubectl -n strata-observer create rolebinding reader \
     --role=strata-observer-reader --group=<group of your team>
   ```

7. **Open the page.** The service listens on the loopback address of the pod. A
   person with the role reaches it with a port-forward, then opens
   `http://127.0.0.1:8080`.

   ```bash
   kubectl -n strata-observer port-forward deploy/strata-observer 8080
   ```

   When port 8080 is in use on your machine, give another local port, such as
   `18080:8080`, and open that port. The command that the page gives to open it again
   keeps the port of the address that you opened.

[The image and the manifests of the observer](../development/observer-deploy.md)
gives each file and the signature check of the image.

The records script shows what it writes. Each Secret holds some records, and the
first Secret also holds `meta.json`. The step shows the names, and cuts the
content:

```text
{{#include ../output/observer-records.txt}}
```

A Secret with no record is empty. The next apply then removes the old content.
The pipeline of your team runs the script in its publish step, so the records are
the files that the team approved.

A live run applied these steps to a cluster that Palette built. The run bound the
role to one user. It also set short periods for the two cycles, and it applied the
output of `kubectl kustomize`.

The pod passed the `restricted` standard and became Ready. Its first cycle reached
Palette, so a workload cluster reaches the API. A user with only the reader role
opened the page with the port-forward. A pod of another namespace could not connect.
After a restart, the page worked again.

## Install with a profile in Palette

An add-on profile can install the chart as a Helm layer, so that Palette installs the
observer and keeps it. The code of Palette shows these steps. No live run shows them
yet.

1. **A tenant admin adds the registry once.** It is an OCI registry of the type
   `basic`, with the provider `helm`, for `ghcr.io/orgs/como-technologies`. It uses a
   classic token with the scope `read:packages`. A live run shows that Palette refuses
   a registry with the auth type `noAuth` for this public chart. The create answers
   HTTP 500 with `InvalidRegistryCredentials`. `ghcr.io` gives no read of its catalog
   with no login, so the step keeps the token: see
   [observer-chart.sh](../development/live-runs.md#observer-chartsh).
2. **Make the Secret of the key with `kubectl`**, as in steps 1 and 2 of
   [Install with Helm](#install-with-helm). A value of a layer reaches the
   cluster in clear, so the profile holds no key and no token.
3. **Make the profile.** Add a Helm layer: the chart `strata-observer` of that
   registry, at the version of your release. In its values, set `cluster.uid` and
   `palette.projectUid`. The block `pack` of the values names
   the namespace and its label `restricted`, and Palette makes the namespace with it.
4. **Attach the profile to the cluster.**

The code of Palette deletes the namespace of a Helm layer when you remove the profile.
No run shows it yet. If the namespace stays, delete it with `kubectl`, so that the key
goes.

A live run applied the manifests of [Install](#install) as one manifest layer of a
profile. It shows what Palette does with that layer:

- **The nodes stay.** Palette replaces no node when it applies the layer. The check
  `no-repave` of [observer.sh](../development/live-runs.md#observersh) shows it.
- **The other profiles stay.** Each other profile of the cluster still agrees with
  its record.
- **The profile of the observer has no record.** The page lists it under "Not
  checked", and it shows no finding for it.
- **A removal deletes the Deployment.** When you remove the profile from the cluster,
  Palette deletes the Deployment. No run shows what happens to the namespace. Delete
  the namespace with `kubectl delete namespace strata-observer`, so that the key goes.

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

This is an exception to the rule of [Where each part belongs](../boundary.md#the-rule-that-decides):
a record in Git holds the content that the team approved. A first-seen record holds
the content that Palette gave at that time. No pipeline approved it. If somebody edited
a profile before that time, the record holds the edit, and the observer reports no
change.

Each screen of the page says "No pipeline approved these records yet" while the baseline
is `first-seen`.

While the baseline is `first-seen`, the observer also records a profile version that
the cluster starts to run after the install. At each content check, it records each
profile version that no record holds, as the seed does, and it checks it at once. It
keeps that record in memory, and it takes it again after a restart. A record in the
Secrets for the same profile version takes its place. With approved records, the
observer records nothing: your pipeline publishes the record of each version that it
applies. Until then, the version shows as `no-record` in Not checked.

### Approve the records with a pipeline

A review in Git approves the records, and a pipeline publishes them. No person edits
the Secrets. Strata gives two jobs for this:

- **`observer-propose`** runs on a schedule. It reads the first-seen records from the
  Secrets. It keeps one pull request open that adds them to `releases/observer/`.
- **`observer-publish`** runs after the merge. It writes the approved records into the
  Secrets, with the revision of the merge.

The checks of your repository, such as [the merge gate](./merge-gate.md), run on the
pull request before the merge. After the publish, `meta.json` says `approved` with the
revision, the top bar says `records: rev <revision>`, and the screens remove the
notice. The seed sees the records, and it never writes again.

1. **Add the publisher.** Add `--set publisher.enabled=true` to the install of
   the chart. The service account `strata-observer-publisher` can get and update the
   four Secrets of the records, and do nothing else.
2. **Keep a kubeconfig of the publisher** as the secret `OBSERVER_KUBECONFIG` of your
   repository. Run these commands with your admin kubeconfig. The runner of the
   pipeline must reach the address of the API server.

   ```bash
   ns=strata-observer
   sa=strata-observer-publisher-token
   kubectl -n $ns get secret $sa -o jsonpath='{.data.ca\.crt}' \
     | base64 -d > ca.crt
   tok=$(kubectl -n $ns get secret $sa -o jsonpath='{.data.token}' \
     | base64 -d)
   server=$(kubectl config view --minify \
     -o jsonpath='{.clusters[0].cluster.server}')
   export KUBECONFIG=publisher.kubeconfig
   kubectl config set-cluster c --server="$server" \
     --certificate-authority=ca.crt --embed-certs
   kubectl config set-credentials publisher --token="$tok"
   kubectl config set-context c --cluster=c --user=publisher \
     --namespace=$ns
   kubectl config use-context c
   gh secret set OBSERVER_KUBECONFIG < publisher.kubeconfig
   unset KUBECONFIG
   rm ca.crt publisher.kubeconfig
   ```

3. **Call the two jobs.** In `examples/ci/github/strata.yml`, remove the comment
   marks of the trigger `push` and of the two jobs of the observer. On GitLab, set `STRATA_OBSERVER_KUBECONFIG`: see
   [GitLab CI](./gitlab.md#the-variables).
4. **Give a token that starts the checks.** A pull request that the token of the
   workflow opens starts no workflow. So keep a token of a person or of a GitHub App
   as the secret `STRATA_PR_TOKEN`, with write access to contents and pull requests.

`scripts/ci/observer-propose.sh --dry-run` writes the files of the records and
pushes nothing. To publish by hand, give the records of the merge to a replace:

```bash
scripts/ci/observer-records.sh --revision "$GIT_SHA" \
  releases/observer/*.json | kubectl replace -f -
```

A replace removes each key that the records do not hold, also a key that the seed
wrote. An apply keeps the keys of the seed.

## The pods of the cluster

The observer can read the pods of its cluster. It then answers the last question:
which digest does each tag run as on this cluster? No route of Palette gives that
digest, so the read repeats nothing that Palette answers. See [The pods of a
cluster](../internals/palette/clusters.md#the-pods-of-a-cluster).

With `--read-pods`, each content cycle starts with a `list` of the pods of every
namespace. The observer takes the image of each container from the spec of its pod,
and the digest from the `imageID` that the node reports. It shows no other fact of a
pod: the page holds no pod name and no namespace. Then it counts the pods of each digest for each image reference. The Images
screen shows the result. [The API of the observer](../reference/observer-api.md#the-images)
lists the routes and the fields.

With no `--read-pods`, the overview shows four tiles, and the tile "Image changes"
is absent. The nav item Images stays and says "Off", as the nav item Alerts says "Not
set". It opens the offer of the read. With the read on, the tile shows, also after a
read that failed.

The observer matches a reference with the images that the packs of the records
name. The exact text wins. If it finds none, the observer compares the name and the
tag, after it removes `docker.io/library/` and `docker.io/`. The page labels this
second match "matched by name and tag". Each image has one status: `steady`,
`moved`, `split`, or `not-checked`. [The images](../development/observer.md#the-images)
gives the rule of each status.

### What the read catches and what it does not

A pod runs the digest that its node pulled. A registry can move a tag, but the move
reaches a pod only when a node pulls the tag again. So the read catches these cases:

- **A tag that a new node pulls from a mutable registry.** The new node runs the
  new digest. The old nodes keep the old one. The tag then has two digests, and the
  finding is `image-split`. On cluster A, the API server runs the admission plugin
  `AlwaysPullImages`. Each new pod there gets the policy `Always`, and it runs the new
  digest, also on an old node. See [The pods of a
  cluster](../internals/palette/clusters.md#the-pods-of-a-cluster).
- **A pull at a restart.** A pod with the pull policy `Always`, or with the tag
  `:latest`, pulls the tag again when it starts. When each pod runs the new digest,
  the finding is `image-moved`.
- **A mutable mirror.** A mirror can serve another digest under one tag. A pod that
  pulls from the mirror then runs the new digest.

The read does not catch these cases:

- **A move with no new pull.** A pod that runs keeps the digest that it pulled. The
  read shows no move until a pod pulls the new image.
- **An image that no pod runs.** The read sees the pods that exist now. A pack that
  names an image that no pod runs has no row.
- **A difference between clusters.** One observer reads one cluster. It compares no
  two clusters.

[A vendor rebuilds an image under one tag](../use-cases/image-tag.md) describes a
team that copies each image, by digest, to a private registry. The pods of that team
pull an unchanged digest from the mirror. The read then shows the status `steady`,
and it makes no finding. That is the right answer, because nothing that runs
changed. The read finds a rebuilt tag only when a pod pulls the rebuilt image.

A mirror can work in two ways. The pod can name the image string of the mirror.
Or the node can fetch from the mirror and keep the string of the pod. A string of
the mirror matches a pack only when the pack names the same string. Otherwise the
image shows as `no pack` and makes no finding.

A live run on kind used both ways. A mirror at the level of the node keeps the name
of the pack in `spec.image` and in `imageID`, so the match by text works. A
rewritten image string names the mirror in both, so it matches no pack. The checks
`mirror-image-string` and `mirror-image-id` of
[observer-kind.sh](../development/live-runs.md#observer-kindsh) show both ways.

### The findings

The read makes two kinds of finding:

- `image-moved`: a tag runs another digest than the first one that the observer saw.
- `image-split`: pods run one tag with two digests.

An image makes a finding only when it matches a pack and its reference names a tag.
A reference that names a digest cannot move, so it makes no finding. An image with
no pack, such as an agent of Palette, shows in the list and makes no finding. A
finding of the pods changes no status of a profile or of a pack, and the Drift tile
does not count it. The list "Needs attention" holds it. For the fix, see [What to
do](../use-cases/image-tag.md#what-to-do).

### Add the read

The base of `deploy/observer` has no role and no token. The component
`deploy/observer/pods/` adds what the read needs:

- A `ClusterRole` with `get` and `list` on `pods`, and its binding to the service
  account of the observer.
- A projected volume with a token that expires after one hour, and the CA of the
  cluster.
- The flag `--read-pods` in the arguments of the observer.
- An egress rule for TCP 6443, beside 443.

The read costs a role and a token: see [The role for the read of the
pods](#the-role-for-the-read-of-the-pods). To add the read, name the component in the
kustomization of step 5 of [Install](#install):

```yaml
components:
  - deploy/observer/pods
```

Then apply the kustomization again:

```bash
kubectl apply -k .
```

The kubelet sets `KUBERNETES_SERVICE_HOST` and `KUBERNETES_SERVICE_PORT`, and the
observer reads the address of the API server from them. `--kube-dir` names the
directory of the token and the CA. A CNI can apply the egress policy after the
address of the service changes to the address of the API server. The rule names TCP
6443, the port that the API server of a kubeadm cluster uses. When yours uses another
port, change the rule.

### What the state keeps

The observer keeps the first digest that it sees for each image in `state.json`. The
file holds an id for the image and the digest, and never the reference. The team
approved the content of the records, so a new revision of the records empties the
digests. `image-moved` clears when the digest comes back, or at a new revision.

`state` is an `emptyDir`. A new pod of the observer starts again from the digests
that it sees, so a tag that moved before that start reads `steady`. A new pod also
loses the first-seen dates of the findings.

### A failed read of the pods

A failed read of the pods is not a failed cycle, because the pods are not the
content. The cause `pods-not-read` appears under "Not checked", and no status of a
profile changes. The answer keeps the images of the last good read. [A failed read
of the pods](../development/observer.md#a-failed-read-of-the-pods) gives the rules.

## The text of a diff

The page of a pack shows the text of its diff, beside the table of settings. The
Values tab shows the raw values as a diff block. The Manifests tab shows the text of
each changed, added, or removed manifest. A control "Unified / Split" on both tabs
chooses between two layouts of the same text. A long line wraps, and a phone shows
the unified layout.

Each cycle passes `--full` to `strata compare` and to `strata verify`. The documents
then carry the text in the field `unified`: see [See the full
diff](../commands/compare/versions.md#see-the-full-diff). Only the detail tier shows
it.

- **The text is the text of the records.** Palette writes the mask `********` over a
  secret, and the text keeps that mask as a placeholder. The text holds no value of a
  cluster variable.
- **The text stays in memory.** It goes in no state file, no summary route, and no
  log. A restart drops it. The next cycle, or the next request for a move, reads it
  again.
- **A pack keeps 256 KiB of text at most for each side of a diff.** The limit counts
  the values and the manifests together. The pod has a memory limit of 512 Mi, and the
  limit keeps the text inside it. Above 256 KiB, the observer keeps none of the text of
  that pack. The page then shows the splash "Too much rock to carry", and it gives the
  command `strata compare record.json <profile> --full`. The table of settings stays.

The check `diff-text` of [What the live example verifies](../development/live-runs.md#observersh)
tests the text of a diff. It reads the text from the pod on a cluster that Palette built.

### A value of many lines

A setting can be a file of many lines. The table of settings then shows only the lines
that differ, on each side. Each line has its line number. A removed line is red, and an
added line is green, as in the text below the table.

- **A value of one line shows whole.** It reads as it did before.
- **The changed part lies between the common start and the common end.** The table does
  not match the lines in between. The text of the diff below the table is the full diff.
- **A line with no color is equal.** The table colors no line that the other side holds
  at the same place. This holds when both sides have the same number of lines.
- **Context is 3 lines at most.** The table takes these lines from the common start and
  the common end. It shows up to 2 above the changed part, and the rest below. A mark
  `…` shows where the table omits lines.
- **More than 40 lines that differ show as a count.** The row says `<n> lines differ`
  and links to the text of the values. The row has no link when the observer holds no
  text.
- **A mask stays a mask.** A line shows the text of the record, so a secret shows the
  mask `********`.

## The states of the page

A place that has nothing to show draws the mark of the logo, a headline, and one line that
states the fact. The line gives the next step when one exists, with a command that you
copy. A finding always shows its data. [The server of the
observer](../development/observer-api.md#the-splash) lists each place, and the motion of
each mark.

The page never reads calmer than the check allows. A page with a finding, a failed read,
or a cause in "Not checked" shows no calm mark. A page that is lost, stale, or cannot
reach the observer shows none either. The tile "Newer versions" says `All <n> packs
current` only after a read of the versions. The tile "Image changes" is green only when the last
read of the pods worked.

| The page says | What it means | The next step |
| --- | --- | --- |
| Reading the layers | The first check runs. | Wait. The page fills in when the check ends. |
| Nothing to compare against yet | No record exists. | Write the records with `observer-records.sh`. On a test cluster, `--first-seen` records what each profile holds now. |
| We lost the signal from Palette | No check ended well, and the last ones failed. The line names the error. | For HTTP 401 and 403, check the key in the Secret `strata-observer-key`, then read the log. For another error, check that the observer reaches Palette, then read the log. |
| Checks stopped before one ended well | The content check did not end well since the start. | Read the log. |
| This page lost its line to the observer | The port-forward ended, or the pod restarted. | Run the port-forward command that the page gives. It keeps the local port of this page. Or click "Try now". |
| All quiet on `<cluster>` | The last check ended well and found nothing. The line gives the number of packs checked in full and the age of the check. Three facts follow: the profiles that match their records, the causes not checked, and the newer versions. | None. |
| Pods are not on the menu here | The read of the pods is off. This is a choice, and no fault. | [Add the read](#add-the-read). |
| The pods stay silent | The read of the pods failed. | For HTTP 401 and 403, ask the API server if the role of the observer can list pods. For another error, read the log. |
| Nobody gets paged yet | The observer sends no alert. This is a choice, and no fault. | [Turn the alerts on](#turn-them-on). |
| Undisturbed | A side of a pack holds what the record holds. | None. |
| Part of this layer is hidden | Strata did not check the pack in full, or the last read of its profile failed in part. A side with no change then keeps the last good answer, and the page does not call it unchanged. The line gives the cause. | Read the page "Not checked". |
| One moment, a check is running | The read of a diff waited for a process of the observer. | Wait, or click "Try now". |
| Too much rock to carry | The text of the pack is above 256 KiB. | Run the command that the page gives. |

## Ask Palette about a move

A newer version is not a fault, and the page keeps it neutral. Before a move, a team
wants one more fact: does Palette give an error for it? The button "Ask Palette"
answers that. It sits in the table of the newer versions. It also sits on the card
"Move to" of a pack, where it reads "Ask Palette about this move".

A click runs `strata behind` for the record of the profile, with
`--pin <layer>/<name>`. Strata sends one `POST /v1/clusterprofiles/{uid}/validate/packs`
for each record that pins the pack. A record of a profile version gives one `POST`. The
request asks a question and changes nothing. [The page of `behind`](../commands/behind.md#palette-accepts-the-move-or-refuses-it)
says what the answer covers.

Before a click, the card says what the button asks, and the head of the column "Palette"
says it on a hover. The column shows the short word of the answer. The card "Move to"
shows the long one:

| The column "Palette" | The card "Move to" | Meaning |
| --- | --- | --- |
| Ask Palette | Ask Palette about this move | No click yet. |
| No error | Palette gave no error. | Palette checked the default values of the new version and gave no error. It did not check your own values. The card says so in these words. |
| Refuses | `Palette refuses: <code>` | Palette gave the error `<code>`. The card shows the message of Palette and the clusters that the message names. |
| No answer | Palette gave no answer. | A request failed. The card adds the HTTP status, when the failure has one. A button "Ask again" follows. |

- **A failed request is no finding.** A request that Palette refuses for the role of
  the key is a failed request. The answer is "Palette gave no answer", and it raises no
  severity.
- **The observer keeps an answer in memory.** It keeps "Palette gave no error" and
  "Palette refuses" until the pin moves or the pod restarts. A second click on that move
  sends nothing.
- **The observer asks again after "Palette gave no answer".** The next click sends a
  new question, because the role or the network can change.
- **Nothing asks on a timer.** A cycle sends no `POST`, because the observer refuses
  each option of `strata` that asks Palette in a cycle. A click costs one question for
  one move.
- **Only the detail tier holds the answer.** A refusal holds the message of Palette
  and the names of clusters. The summary and the state hold none, and the log has one
  line for a click, with a verdict and a status.

[The API of the observer](../reference/observer-api.md#ask-palette-about-a-move) gives
the route and the header that the server needs for the click. The checks
`ask-one-post` and `ask-words` of [What the live example
verifies](../development/live-runs.md#observersh) test the click. `ask-one-post` runs the
observer on the machine of the script, behind a request counter, because the counter
cannot see the requests of a pod. `ask-words` clicks on the pod on a cluster that Palette
built, through the port-forward.

## Alerts

The observer can send a message to a webhook. It sends one message when a finding
opens, and one when the finding clears. The send is off by default. With no
`--alerts` flag, the observer sends nothing, and the nav item "Alerts" says "Not set".

Alerts are a stopgap. Palette has alerts for the health of a cluster, and no event for
a published version that changed. Palette is the owner of that event. When Palette
adds it, strata removes the news `drift` of the published kinds from the alerts. [An event
when a published version changes](../internals/palette/profiles.md#an-event-when-a-published-version-changes)
gives the owner of each kind of news and what removes it. The observer sends nothing about
health.

### What a message holds

A message holds the cluster, the kind of news, the profile, its packs, one fixed
sentence, and the time of the first sight. It holds no value, no path, and no
name of a person. The observer builds it from the summary tier of the page. The
Alerts screen shows a sample.

The body of the request is `{"text": "<message>"}`, which Slack and Teams both read.
The observer sends it with `POST` to the address of the channel.

### The rules

A file of rules says which news goes where. The file is JSON. Your team keeps it in
Git, and a ConfigMap carries it to the pod. The page only reads it, and no route of
the observer writes a rule. A mute is a line of the file, so Git shows who muted what.

```json
{{#include ../../../examples/observer-alerts/alerts.json}}
```

| Field | Meaning |
| --- | --- |
| `channels` | A name for each webhook. `urlFile` is the path of the file that holds its address. |
| `level` | `cluster`, `profile`, or `pack`. |
| `match` | A profile name or `name@version`, for `profile`. A pack name or `layer/name`, for `pack`. A `cluster` rule has none. |
| `news` | The kind of news: `drift`, `image`, `lost`, or `newer`. |
| `send` | `now` sends at once. `digest` waits for the next digest. A rule with no `send` is a mute. |
| `to` | The name of a channel. |
| `muteUntil` | The last day of a mute, as `YYYY-MM-DD`, in UTC. |

| News | What it follows |
| --- | --- |
| `drift` | A finding of the kinds `published-changed`, `overwritten`, `renamed`, `gone`, `cluster-differs`, and `image-id-changed`. |
| `image` | A finding `image-moved` or `image-split`. These need `--read-pods`. |
| `lost` | The page state `lost`: the observer cannot reach Palette. It is a rule of the level `cluster`. |
| `newer` | A newer version of a pack in a profile. Each profile that pins the pack makes one item. |

- **A lower level overrides a higher one.** The level `pack` wins over `profile`, and
  `profile` wins over `cluster`. If two rules of one level cover the item, a live mute
  wins. Else the first rule in the file wins.
- **A mute always has an end date.** A mute with no end date stops the start. A mute
  holds through the day of its end date. After that day, it counts as no mute, and
  the rule of the next level decides.
- **A mute can stand beside a rule that sends.** Write the mute as a second rule with
  the same level, match, and news. The mute wins until its end date. Then the rule that
  sends decides again, and the item sends its message.
- **A newer version never pages a person.** The rule `newer` with `send: now` stops the
  start. Use `digest`.
- **A file that does not parse stops the start of `serve`.** The error names the line
  of the fault. Two rules that send, with the same level, match, and news, also stop
  the start. A mute is not part of this check. `once` reads the file the same way, and
  it sends nothing. It is a check of the file:

```text
{{#include ../output/observer-alerts-fault.txt}}
```

### The states

The Alerts screen shows one state for each rule, with the words of Prometheus:

| State | Meaning |
| --- | --- |
| Firing | An item is open, and its message went. |
| Pending | An item is open. It waits for its digest or for a retry. |
| Inactive | No item is open for this rule. |
| Muted | A mute holds. |

- **One message goes when a finding opens, and one when it clears.** A clear needs a
  check that proves it. A failed read clears nothing, so a finding that the observer
  could not check again stays open. A cycle that read no pod clears no finding of an
  image. A mute stops the clear of a message that went.
- **A failed send keeps the message.** The state holds it, and the next content cycle
  sends it again. When the send works, the message goes once. The log names the
  channel, the error kind, and the status, and never the address or the text. The
  Alerts screen shows a bar while a message waits.
- **A digest goes at most once in each period of `--digest-every`.** The period is 7
  days. A digest lists each open item of the digest rules of one channel, so an item
  that stays open is in each digest. A digest lists 30 items, and then the line
  `and <n> more.` A digest has no clear message. With no item open,
  the digest waits for the first item. A channel that got its digest gets no second one
  in the period, also when the send to another channel failed.
- **`once` sends nothing.** With no `--alerts`, nothing sends.

### What alerts do not do

- **A rebuild makes no finding until a pod pulls it.** An alert fires on a finding. With
  `--read-pods`, the pull opens a finding, and the news `image` sends it. A mirror that
  serves the old digest stays `steady`, and it sends nothing. See [A vendor rebuilds an
  image under one tag](../use-cases/image-tag.md).
- **A dead observer sends nothing.** The message comes from the pod. Alert on the
  Deployment with the monitoring of your team. The observer sends no heartbeat, and it
  never reads the health of a workload.
- **A new pod sends the open findings again, once.** The volume `state` is an
  `emptyDir`, so the new pod knows nothing of the old pod. One message too many is
  better than one too few.

### Turn them on

The component `deploy/observer/alerts` adds the flag and two volumes. It adds no
object. These steps run from the repository of your team, after [Install](#install).

1. **Write the rules.** `examples/observer-alerts/alerts.json` is a start. Each
   `urlFile` is `/alert-channels/` and the name of the channel.
2. **Make the Secret of the addresses.** The address of a webhook holds a token, so
   it is a Secret. Use one key for each channel, named as the channel. The process
   substitution keeps each address out of an argument and out of a file.

   ```bash
   kubectl -n strata-observer create secret generic strata-observer-alert-channels \
     --from-file=platform-oncall=<(printf %s "$ONCALL_WEBHOOK") \
     --from-file=platform-planning=<(printf %s "$PLANNING_WEBHOOK")
   ```

3. **Add the component and the ConfigMap.** Add these lines to the `kustomization.yaml`
   of step 5 of [Install](#install). Copy `deploy/observer/alerts` with the other
   manifests. Keep the line `namespace: strata-observer` of that file: it puts the
   generated ConfigMap in the namespace of the pod. The volume `alert-rules` is not
   optional, so the pod does not start without the ConfigMap in its namespace.

   ```yaml
   components:
     - deploy/observer/alerts
   configMapGenerator:
     - name: strata-observer-alerts
       files:
         - alerts.json
   generatorOptions:
     disableNameSuffixHash: true
   ```

   ```bash
   kubectl apply -k .
   ```

4. **Look at the Alerts screen.** It lists each rule with its state. A rule that
   names a channel without an address stops the start, and the log says which
   channel.

The address must use `https`. The observer follows no redirect, and it reads the
address again at each send, because the kubelet renews a Secret. The network policy
of the base lets the observer connect to port 443. To narrow that rule, see [What you give
the observer](#what-you-give-the-observer). The flag `--alerts-allow-http` allows an
`http` address for the tests of the observer. Do not use it on a cluster.

## What the observer does not see

Each limit below is a fact of this release. The page shows the numbers of `limits`,
and it never reads "No change" for a pack that it did not check.

1. **A tag that moves at its source.** A record names a tag, and strata reads no
   registry. So a rebuilt tag shows no change, and the profile stays `ok`. With
   `--read-pods`, the observer sees the move after a pull only: see [What the read
   catches](#what-the-read-catches-and-what-it-does-not). A pin in the pack, or the
   copy step of your pipeline, closes this. See [What OK does not cover](../commands/verify.md#what-ok-does-not-cover)
   and [A vendor rebuilds an image under one tag](../use-cases/image-tag.md). A rebuild
   makes no finding until a pod pulls it, so it sends no alert before then.
2. **A change on the cluster with `kubectl edit`.** The service compares what
   Palette holds. Without `--read-pods`, it reads no object in the cluster. With the
   flag, it reads the pods and names no edit. So it does not see such an edit.
   No run shows what Palette does with it: see [What no run
   covers](../internals/palette/limits.md).
3. **What the workload runs.** "This cluster's copy in Palette" is the copy that
   Palette holds for this cluster. It is not the objects that run in the cluster.
   The page does not show the health of a workload: `OK` means that the content
   matches. A container in a crash loop keeps its digest and makes no finding. See the
   check `image-crash-loop` of
   [observer-images.sh](../development/live-runs.md#observer-imagessh).
4. **The value of a cluster variable.** It is outside the root, so a change of one
   does not show. The service also never shows the value: see
   [The API of the observer](../reference/observer-api.md#what-no-answer-carries).

## What you give the observer

1. **An API key in the cluster.** This is the first part of strata that holds a
   credential in a workload cluster. A key has each permission of its user in Palette,
   not only those for this cluster. Whoever can read the Secret `strata-observer-key`
   has those permissions. Use a dedicated user with the least role.
   [The least role of the key](#the-least-role-of-the-key) says which role.
2. **Never a profile variable for the key.** A profile variable is in clear in the
   copy of the cluster. So the key stays there in clear. Make the Secret with `kubectl`.
3. **Egress to Palette** from the cluster. The network policy of the observer allows
   DNS and HTTPS only. A team that narrows the HTTPS rule must list Palette, each
   webhook of the alerts, and, with the read of the pods, the API server. A rule that
   names Palette alone stops the alerts and the read of the pods.
   A pod on a cluster that Palette built reaches the API, and it ends its first cycle.
   The read of the pods adds TCP 6443: see [Add the read](#add-the-read).
4. **Requests.** A cycle sends `GET` requests and no other method. A content cycle
   reads the cluster and the profile version of each record, and the audit log for a
   record that drifted. A versions cycle reads the registries and the packs that the
   records pin. A click sends one `POST`: see [Ask Palette about a move](#ask-palette-about-a-move).
   With `--alerts`, each message is one `POST` to your webhook: see [Alerts](#alerts).
   [The requests of a cycle](#the-requests-of-a-cycle) gives the counts.
5. **The page holds pack values.** The detail tier shows the old and the new value of
   a pack, and the text of its diff. A secret typed into a value that the schema does
   not mark passes through. `create` on `pods/portforward` is the gate: whoever has
   that verb opens the page. The reader role gives no Secret and no `exec`.

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

With `--read-pods`, a content cycle also sends one request to the Kubernetes API
for each 500 pods. The request lists the pods, and it never reads one pod alone.

A click sends the reads of `strata behind` for one pack, and one `POST` to Palette.
[One pack](../commands/behind.md#one-pack) says what `behind` reads for a pack.

### The least role of the key

The key reads the cluster, its profiles, the registries, and the audit log.
**Project Viewer** in the project of the cluster is the least role that gives each
answer. The role is read-only, and the observer needs no tenant role, because
each request names the project. [The role that a key needs](../internals/palette/clusters.md#the-role-that-a-key-needs)
lists each permission and each refusal.

- **Keep the audit log in the role, to name the writer.** Cluster Viewer alone does
  not hold `audit.list`. The audit log then answers HTTP 403 with the code
  `AccessDenied`. With that role, the verdict and the finding stay, and `who`
  is empty. The page lists an event that says the audit log could not be read. A
  custom role needs `audit.list` beside `cluster.get`, `clusterProfile.get`,
  `packRegistry.get`, and `packRegistry.list`.
- **A read-only key can click.** The route that the click calls checks
  `clusterProfile.get` on the profile, and no write permission. Four built-in roles
  hold it: Cluster Profile Viewer, Cluster Viewer, Project Viewer, and Tenant Viewer.
  A key without it gets "Palette gave no answer", with the status of the refusal. A
  live run asked with Project Viewer, and Palette answered HTTP 200. The other three
  roles and the refusal come from the code of Palette.
- **Make the user for the observer alone.** Give it no other role, and make the key
  in the name of that user.
- **A live run used both roles.** `just live-observer-role` sent each request of the
  cycles with a second key. With Project Viewer, each request answered HTTP 200, and
  so did the request of the click "Ask Palette". With Cluster Viewer, each request
  answered HTTP 200 but the audit log, and no run sent the request of the click.
  [observer-role.sh](../development/live-runs.md#observer-rolesh) lists the checks.

### The role for the read of the pods

The read adds a role and a token to the pod. The base of `deploy/observer` has
neither. [Add the read](#add-the-read) tells how to ask for them.

- **`list` on `pods` gives the whole spec of each pod, with its `env`.** Whoever holds
  the token reads every `env` of the cluster. A value there is in clear. A team must
  ask for this on purpose. The observer decodes no `env`.
- **The role holds `get` and `list` on `pods`, and no other verb.** It is a
  `ClusterRole`, so it covers every namespace. It gives no write, and it gives no
  Secret.
- **The token expires after one hour.** The kubelet renews it, and the observer reads
  the file again at each call.
- **The egress rule adds TCP 6443 beside 443.** The pod can then reach the API server
  of the cluster.

### The Secret for the alerts

The alerts add a Secret and a webhook to the cluster. [Turn them on](#turn-them-on) tells
how to ask for them.

- **The address of a webhook holds a token.** Whoever can read the Secret
  `strata-observer-alert-channels` can post to your channel. The observer shows the name
  of a channel on the page and in the log, and never its address.
- **The observer sends only to an `https` address.** It follows no redirect. It sends
  the message, and it reads nothing of the answer but its status.
- **The component adds no role.** The alerts need no Kubernetes call.

## `re-baseline needed`

A new minor version of strata can move a root with no change of content. The
finding then has `reBaselineNeeded`, and the page shows `re-baseline needed` beside
the drift. The finding stays. The observer never writes a record. Write the records
again with the new version of strata. See [Versions and
compatibility](../reference/compatibility.md) for each change that moves a root.

## Next

- [The API of the observer](../reference/observer-api.md): the routes, the two
  tiers, and the fields that never leave.
- [The observer shows an edit in place](../use-cases/observer.md): the problem, the
  commands, and the fix.
- [The checks of the observer](../development/observer.md): the command of each
  step, each finding, and each state of the page.
- [Alerts](#alerts): the rules, the states, and the limits of the message.
