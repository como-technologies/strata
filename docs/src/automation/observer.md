# The observer

`strata-observer` is a service that runs in one cluster. It runs the `strata`
binary on two timers, keeps the last answer of each, and shows the answers on a
page. It reads the records of your team, and it asks Palette what it holds now.
It writes nothing to Palette. A click on the page sends one question to Palette:
see [Ask Palette about a move](#ask-palette-about-a-move).

Your pipeline runs strata at an event: a pull request, a merge, or a scheduled run.
The observer asks the same questions on a timer, also between those events. A person
who opens the page sees the answer of the last content cycle. A cycle starts every 15
minutes, with a random delay of 10 percent at most. The observer holds no rule about a
profile. Each answer about a profile is the answer of a `strata` command.

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
  profile versions of the records have a newer published version. Then it compares
  the record of each one with the newest version.
- **A click** on "Ask Palette" runs `strata behind` for one pack. No timer starts it.
- **A click on the pill of a cycle** opens its menu. "Check the content now" or "Read
  the versions now" runs that cycle immediately, and not at the end of its period.
- **A change of the records** starts a content cycle immediately. So the page shows
  the records of your pipeline soon after the publish.
- Each cycle starts when the process starts. At most four `strata` processes run at
  a time.
- A cycle writes its files to a volume in memory, and removes them at its end.
- A cycle that fails keeps the last good answer, and the page says how old it is.

The records are the files that your pipeline writes, one for each profile version
of the cluster. The observer compares them with Palette, and it never writes a
record.

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

2. **Make the Secret of the key.** Send the key on standard input. Then no argument
   and no file holds the key.

   ```bash
   printf %s "$PALETTE_API_KEY" \
     | kubectl -n strata-observer create secret generic \
         strata-observer-key --from-file=api-key=/dev/stdin
   ```

3. **Install the chart.** The address of the cluster page in Palette holds the uid
   of the cluster and the uid of its project.

   ```bash
   helm install strata-observer \
     oci://ghcr.io/como-technologies/charts/strata-observer \
     --version 0.36.0 -n strata-observer \
     --set cluster.uid=<cluster uid> \
     --set palette.projectUid=<project uid>
   ```

4. **Open the page.** Run the port-forward.

   ```bash
   kubectl -n strata-observer port-forward deploy/strata-observer 18080:8080
   ```

   Open `http://127.0.0.1:18080` in a browser.

The page says "Reading the layers" until the first check ends. [What each state
means](../development/observer-api.md#what-each-state-means) gives the headline of each
screen and the next step.

The job `strata-observer-seed-1` writes the first-seen records when the Secrets of
the records hold none, and the first cycle reads them. It never replaces records that
a pipeline published. `kubectl -n strata-observer logs job/strata-observer-seed-1`
shows what it did.

| Value | Default | Function |
| --- | --- | --- |
| `cluster.uid`, `palette.projectUid` | none, required | The cluster and its project. |
| `palette.api` | `https://api.spectrocloud.com` | The Palette address. |
| `palette.keySecret` | `strata-observer-key` | The Secret with the key `api-key`. |
| `image.pullSecret` | empty | A Secret for the pull of the image. The image is public, so set it only for a copy in a registry with a login. |
| `image.tag` | the version of the chart | The tag of the image. |
| `contentEvery`, `versionsEvery` | `15m`, `6h` | The period of each cycle. |
| `readPods` | `false` | The read of the pods: see [The pods of the cluster](#the-pods-of-the-cluster). |
| `seed.enabled` | `true` | The job that writes the first-seen records. |
| `records.create` | `true` | The chart makes the four Secrets of the records, empty. Set `false` when a pipeline made them first. |
| `readerGroup` | empty | A group that gets the role that opens the page. |
| `publisher.enabled` | `false` | A service account for a pipeline, which can get and update the Secrets of the records. See [Approve the records with a pipeline](#approve-the-records-with-a-pipeline). |

The chart sets no value of a key or a token. The key stays a Secret that you make
with `kubectl`, so that no release and no profile holds it.

## The baseline and the check

The baseline is the set of records that your team approved. The observer checks
Palette against it. Only a merge moves the baseline.

1. **Record a version.** A person runs `strata snapshot` on a profile version. Then
   the person opens a pull request with the record.
2. **Merge the record.** A review approves the file. The merge moves the baseline.
3. **Write the records.** The pipeline runs `scripts/ci/observer-records.sh` with
   `--revision`, and applies the four Secrets. See [Install with
   kubectl](#install-with-kubectl).
4. **Check.** The two cycles check Palette against the records. A difference opens a
   finding on the page.
5. **Close the finding.** Publish a new version with the change through review.
   Then record the new version. Do not repair the edited version in place: that is
   a second edit. The finding also clears when the published version holds the
   content of its record again.

The observer never writes a record. An edit in Palette moves no baseline: it opens a
finding. To try the observer with no approved record, see [First-seen
records](#first-seen-records).

### The card "Published version"

The page of a profile has the card "Published version". It shows the status that
`strata verify` gives the record, with a badge and a line of words:

| Status | Badge | Words |
| --- | --- | --- |
| `unchanged` | "OK" | "Unchanged since the record" |
| `unchanged`, and the last read of the profile failed in part | "Not checked" | "The last read failed, so strata cannot say that nothing changed." |
| `drifted` | "Drift" | No words. The card shows the root of the record and the root that Palette gives now. |
| `unverified` | "Not checked" | "Strata read this version, but not the upstream digest of each pack." |
| `error` | "Not checked" | "Strata could not read this version." |

Strata read an `unverified` version, and its content is the same as the record. But
a pack has no upstream digest, so a change of the archive of that pack does not show:
see [The verdicts](../commands/verify.md#the-verdicts). Strata could not read a
version with the status `error`. A profile with no record gives the badge "No record"
and the words "Nothing to compare with."

### A version that the cluster no longer runs

A cluster moves to the next version of a profile, and the record of the old version
stays. The observer still checks that record against Palette. The page lists the record
in a fold under the table of the profiles, as "Not on this cluster". It names no newer
version for the record. A record with a finding stays in the table as a drift.

A cause of "Not checked" can name such a record, for example when a pack has no
upstream digest. The tile "Not checked", the screen "Not checked", and the filter "Not
checked" then count the cause. The filter shows the row of the record in the table,
with the badge "Not on this cluster". The filter "All" shows the record in the fold
only. [A version that the cluster no longer
runs](../development/observer.md#a-version-that-the-cluster-no-longer-runs) gives each
rule.

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
[The observer shows an edit in place](../use-cases/observer.md) shows the page for the
same run.

[Run it once](../development/observer.md#run-it-once) gives the exit codes and the
flags. [The summary](../development/observer.md#the-summary) gives each part of the
summary, with an example of the newer versions and of the limits.

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
is `first-seen`. In that time, the observer also records each profile version that the
cluster starts to run after the install. With approved records, the observer records
nothing. A new version then shows as `no-record` under "Not checked" until your
pipeline publishes its record. [The seed](../development/observer.md#the-seed) gives
the rules.

### Approve the records with a pipeline

A review in Git approves the records, and a pipeline publishes them. No person edits
the Secrets. Strata gives two jobs for this:

- **`observer-propose`** runs on a schedule. It reads the first-seen records from the
  Secrets. It keeps one pull request open that adds them to `releases/observer/`. The body
  names the cluster and the path of each record in code spans that no backtick of a name ends.
- **`observer-publish`** runs after the merge. It writes the approved records into the
  Secrets, with the revision of the merge.

The checks of your repository, such as [the merge gate](./merge-gate.md), run on the
pull request before the merge. After the publish, `meta.json` says `approved` with the
revision, and the top bar says `records: rev <revision>`. The screens remove the
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
   marks of the trigger `push` and of the two jobs of the observer. On GitLab, set
   `STRATA_OBSERVER_KUBECONFIG`: see [GitLab CI](./gitlab.md#the-variables).
4. **Give a token that starts the checks.** A pull request that the token of the
   workflow opens starts no workflow. So keep a token of a person or of a GitHub App
   as the secret `STRATA_PR_TOKEN`. The token needs write access to contents and pull
   requests.

`scripts/ci/observer-propose.sh --dry-run` writes the files of the records and
pushes nothing. To publish by hand, give the records of the merge to a replace:

```bash
scripts/ci/observer-records.sh --revision "$GIT_SHA" \
  releases/observer/*.json | kubectl replace -f -
```

A replace removes each key that the records do not hold, also a key that the seed
wrote. An apply keeps the keys of the seed.

## Install with kubectl

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
   Use `--server-side`: a client-side apply copies each Secret to an annotation with
   a limit of 256 KiB. The glob `releases/*.json` fits a folder with the records of
   one project.

   ```bash
   scripts/ci/observer-records.sh --revision "$GIT_SHA" releases/*.json \
     | kubectl apply --server-side -f -
   ```

   [The everyday loop](../loop.md#the-pipeline-publishes) keeps a folder for each
   version, with the records of `dev` and of `prod` in it. Its glob is
   `releases/*/dev.json`, so it differs from the glob above. [The
   records](../development/observer-deploy.md#the-records) shows what the script writes.

5. **Apply the manifests.** Write a `kustomization.yaml` that names the namespace,
   the folder, and the tag of the image. The tag is the tag of the release of strata.
   The line `namespace` is part of the step. Without it, an object that your file
   makes, such as the ConfigMap of the alerts, gets no namespace. `kubectl apply -k .`
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
   person with the role connects with a port-forward, then opens
   `http://127.0.0.1:8080`.

   ```bash
   kubectl -n strata-observer port-forward deploy/strata-observer 8080
   ```

   When port 8080 is in use on your machine, give another local port, such as
   `18080:8080`. Then open the page on that port. The page then gives a command that
   opens it again on the same port.

[The image and the manifests of the observer](../development/observer-deploy.md)
gives each file, the signature check of the image, and the live runs of these steps.

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
   [Install with Helm](#install-with-helm). A value of a layer is in clear on the
   cluster, so the profile holds no key and no token.
3. **Make the profile.** Add a Helm layer: the chart `strata-observer` of that
   registry, at the version of your release. In its values, set `cluster.uid` and
   `palette.projectUid`. The block `pack` of the values names the namespace and its
   label `restricted`, and Palette makes the namespace with it.
4. **Attach the profile to the cluster.**

When you remove the profile, Palette deletes the namespace of a Helm layer. The code
of Palette shows it; no live run shows it yet. If the namespace stays, delete it with
`kubectl`, so that the cluster no longer holds the key.

A live run applied the manifests of [Install with kubectl](#install-with-kubectl) as
one manifest layer of a profile. It shows what Palette does with that layer:

- **The nodes stay.** Palette replaces no node when it applies the layer. The check
  `no-repave` of [observer.sh](../development/live-runs.md#observersh) shows it.
- **The other profiles stay.** Each other profile of the cluster still agrees with
  its record.
- **The profile of the observer has no record.** The page lists it under "Not
  checked", and it shows no finding for it.
- **A removal deletes the Deployment.** When you remove the profile from the cluster,
  Palette deletes the Deployment. No run shows what happens to the namespace. Delete
  the namespace with `kubectl delete namespace strata-observer`, so that the cluster
  no longer holds the key.

An edit of the profile that drops an object from the manifest does not remove the
object from the cluster. Delete it with `kubectl`. See [An object that an edit drops
from a manifest](../internals/palette/clusters.md#an-object-that-an-edit-drops-from-a-manifest).

## The pods of the cluster

The observer can read the pods of its cluster. It then answers the last question:
which digest does each tag run as on this cluster? No route of Palette gives that
digest, so the read repeats nothing that Palette answers. See [The pods of a
cluster](../internals/palette/clusters.md#the-pods-of-a-cluster).

With `--read-pods`, each content cycle starts with a `list` of the pods of every
namespace. The observer takes the image of each container from the spec of its pod,
and the digest from the `imageID` that the node reports. It shows no other fact of a
pod: the page holds no pod name and no namespace. The Images screen shows the number
of pods of each digest for each image reference.

An image makes a finding only when it matches a pack and its reference names a tag.
When the tag runs one digest that is not the first digest that the observer saw, the
finding is `image-moved`. When the tag runs two digests or more, the finding is
`image-split`.

[The read of the pods](../development/observer.md#the-read-of-the-pods) gives the
match with the packs, the status of each image, what the read catches, and a failed
read. [The images](../reference/observer-api.md#the-images) gives the routes and the
fields.

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
kustomization of step 5 of [Install with kubectl](#install-with-kubectl):

```yaml
components:
  - deploy/observer/pods
```

Then apply the kustomization again:

```bash
kubectl apply -k .
```

The rule names TCP 6443, the port of the API server of a kubeadm cluster. When your
API server uses another port, change the rule. [The component that reads the
pods](../development/observer-deploy.md#the-component-that-reads-the-pods) gives each
file of the component.

## Ask Palette about a move

A newer version is not a fault, and the page keeps it neutral. Before a move, a team
wants one more fact: does Palette give an error for it? The button "Ask Palette"
answers that. It is in the table of the newer versions. It is also on the card "Move
to" of a pack, with the label "Ask Palette about this move".

A click runs `strata behind` for the record of the profile, with
`--pin <layer>/<name>`. Strata sends one `POST /v1/clusterprofiles/{uid}/validate/packs`
for each record that pins the pack. The request asks a question and changes nothing.
[The page of `behind`](../commands/behind.md#palette-accepts-the-move-or-refuses-it)
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

The question does not check two more rules of a move. So strata checks them, and `behind`
gives each check that finds something in `bump.updateChecks`:
see [What the update checks](../internals/palette/validate-routes.md#what-the-update-checks).
The page shows each check beside the answer of Palette, with each verdict. The column
"Palette" adds a short line under the word of the answer. The card "Move to" adds an amber note
under the answer:

| The column "Palette" | The card "Move to" | Meaning |
| --- | --- | --- |
| Skips a minor version | `The move skips a minor version: <pack>@<from> → <pack>@<to>` | The move of the Kubernetes pack skips a minor version. The card names the rule of Kubernetes: `Kubernetes moves one minor version at each upgrade`. It names the next step: the newest version of the next minor version in the registry, or `a version of <minor>`. The card gives no code of Palette. See [A move that skips a minor version](../commands/behind.md#a-move-that-skips-a-minor-version). |
| Adds a required variable | `The move adds a required variable: <name>` | The move adds a required variable with no default value. The card says that Palette refuses an update that gives it no value, with `InvalidVariable`. With more variables, the words name the count and each variable. |

- **A check comes from strata, and not from Palette.** The note says so. A check keeps its line
  beside "Palette gave no answer" too, because strata makes it with no request. It raises no
  severity. Only the detail tier holds it, as it holds the answer.
- **The bump of a pack names a required variable only when strata gives one.** A variable
  belongs to the profile. So for the move of a pack, `behind` gives the skip of a minor version
  alone. It names the variables that a move adds in the bump of a profile version: see
  [The move to the newest profile version](../commands/behind.md#the-move-to-the-newest-profile-version).
  The page shows them on the card "Move to" of a profile: see
  [The move to a newer profile version](#the-move-to-a-newer-profile-version).

[Ask Palette about a move](../reference/observer-api.md#ask-palette-about-a-move) gives
the route, how long the observer keeps an answer, and when it asks again.

## The move to a newer profile version

The page of a profile with a newer published version has the card
`Move to <profile>@<version>`. The versions cycle compares the record with the newest
version, by its uid. The card shows the impact level, the change flags, and each pack
that the move changes, adds, or removes.

The comparison of two versions of one profile also gives the checks of the move: see
[The checks of a move](../commands/compare/versions.md#the-checks-of-a-move). Strata
makes them with no request. The card names each check that finds something in an amber
note above its table of packs:

| The card "Move to" | Meaning |
| --- | --- |
| `The move adds a required variable: <name>` | The newest version adds a required variable with no default value. The note says that Palette refuses an update that gives it no value, with `InvalidVariable`. With more variables, the words name the count and each variable. |
| `The move skips a minor version: <pack>@<from> → <pack>@<to>` | The move of the Kubernetes pack skips a minor version. The note names the rule of Kubernetes, `Kubernetes moves one minor version at each upgrade`. It names the next step, `a version of <minor>`. The comparison reads no registry, so the note names no version of that minor version. The note gives no code of Palette. |

- **The card asks Palette nothing.** So its note ends with "Strata checks this rule,
  because no question to Palette checks it."
- **A move with no check has no note.**
- **The summary tier holds the checks**, in `newerVersion.move.updateChecks`. They hold
  the names of the variables and the versions of the pack, and no value. See
  [The routes](../reference/observer-api.md#the-routes).

## Alerts

The observer can send a message to a webhook. It sends one message when a finding
opens, and one when the finding clears. The send is off by default. With no
`--alerts` flag, the observer sends nothing, and the nav item "Alerts" says "Not set".

Palette has alerts for the health of a cluster, and no event for a published version
that changed. Until Palette has that event, the observer sends a message for it.
Palette is the owner of that event. When Palette adds it, strata removes the news
`drift` of the published kinds from the alerts. [An event when a published version
changes](../internals/palette/profiles.md#an-event-when-a-published-version-changes)
gives the owner of each kind of news and what removes it. The observer sends nothing
about health.

[The alerts](../reference/observer-api.md#the-alerts) gives the file of the rules, the
states, what a message holds, and what an alert does not do.

### Add the alerts

The component `deploy/observer/alerts` adds the flag and two volumes. It adds no
object. These steps run from the repository of your team, after [Install with
kubectl](#install-with-kubectl).

1. **Write the rules.** Use `examples/observer-alerts/alerts.json` as the first
   version of the file. Each `urlFile` is `/alert-channels/` and the name of the
   channel.
2. **Make the Secret of the addresses.** The address of a webhook holds a token, so
   it is a Secret. Use one key for each channel, with the name of the channel. With
   the process substitution, no argument and no file holds an address.

   ```bash
   kubectl -n strata-observer create secret generic strata-observer-alert-channels \
     --from-file=platform-oncall=<(printf %s "$ONCALL_WEBHOOK") \
     --from-file=platform-planning=<(printf %s "$PLANNING_WEBHOOK")
   ```

3. **Add the component and the ConfigMap.** Add these lines to the `kustomization.yaml`
   of step 5 of [Install with kubectl](#install-with-kubectl). Copy
   `deploy/observer/alerts` with the other manifests. Keep the line `namespace:
   strata-observer` of that file: it puts the generated ConfigMap in the namespace of
   the pod. The volume `alert-rules` is not optional, so the pod does not start without
   the ConfigMap in its namespace.

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

4. **Open the Alerts screen.** It lists each rule with its state. When a rule names a
   channel without an address, the observer does not start, and the log names the
   channel.

The address must use `https`. The observer follows no redirect, and it reads the
address again at each send, because the kubelet renews a Secret. The network policy
of the base lets the observer connect to port 443. To narrow that rule, see [What you give
the observer](#what-you-give-the-observer). The flag `--alerts-allow-http` allows an
`http` address for the tests of the observer. Do not use it on a cluster.

## What the observer does not see

Each limit below is a fact of this release. The page shows the numbers of `limits`,
and it never shows "No change" for a pack that it did not check.

1. **A tag that moves at its source.** A record names a tag, and strata reads no
   registry. So a rebuilt tag shows no change, and the profile stays `ok`. With
   `--read-pods`, the observer sees the move only after a pull, and no finding or alert
   comes before it: see [What the read
   catches](../development/observer.md#what-the-read-catches-and-what-it-does-not). A
   pin in the pack, or the copy step of your pipeline, removes this limit. See [What OK
   does not cover](../commands/verify.md#what-ok-does-not-cover) and [A vendor rebuilds
   an image under one tag](../use-cases/image-tag.md).
2. **A change on the cluster with `kubectl edit`.** The service compares what
   Palette holds. Without `--read-pods`, it reads no object in the cluster. With the
   flag, it reads the pods and names no edit. So it does not see such an edit.
   A live run shows that Palette gives no condition and no event for it: see [An
   object that an edit drops from a
   manifest](../internals/palette/clusters.md#an-object-that-an-edit-drops-from-a-manifest).
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
   DNS and HTTPS only. A narrow HTTPS rule must list Palette, each webhook of the
   alerts, and the API server for the read of the pods. A rule that names Palette
   alone stops the alerts and the read of the pods. A pod on a cluster that Palette
   built reaches the API, and it ends its first cycle. The read of the pods adds TCP
   6443: see [Add the read](#add-the-read).
4. **Requests.** A cycle sends `GET` requests. The search of a set of clusters is a
   `POST` that reads. A content cycle reads the cluster, the profile version of each
   record, and the audit log for a record that drifted. A versions cycle reads the
   registries and the packs that the records pin. A click sends one `POST` to Palette,
   and each message of the alerts is one `POST` to your webhook. [The requests of a
   cycle](../development/observer.md#the-requests-of-a-cycle) gives the counts.
5. **The page holds pack values.** The detail tier shows the old and the new value of
   a pack, and the text of its diff. A secret in a value that the schema does not mark
   as masked shows on the page. The verb `create` on `pods/portforward` controls the
   access: whoever has that verb opens the page. The reader role gives no Secret and
   no `exec`.

### The least role of the key

The key reads the cluster, its profiles, the registries, and the audit log.
**Project Viewer** in the project of the cluster is the least role that gives each
answer. The role is read-only, and the observer needs no tenant role, because
each request names the project. [The role that a key needs](../internals/palette/clusters.md#the-role-that-a-key-needs)
lists each permission and each refusal.

- **Keep the audit log in the role, to name the writer.** Cluster Viewer alone does
  not hold `audit.list`. The audit log then answers HTTP 403 with the code
  `AccessDenied`. With that role, the verdict and the finding stay, and `who`
  is empty. The page lists the event `AuditUnread`: strata could not read the
  audit log. A custom role needs `audit.list` beside `cluster.get`,
  `clusterProfile.get`, `packRegistry.get`, and `packRegistry.list`.
- **A read-only key can click.** The route that the click calls checks
  `clusterProfile.get` on the profile, and no write permission. Four predefined roles
  hold it: Cluster Profile Viewer, Cluster Viewer, Project Viewer, and Tenant Viewer.
  A key without it gets "Palette gave no answer", with the status of the refusal. A
  live run asked with Project Viewer, and Palette answered HTTP 200. The code of
  Palette shows the other three roles and the refusal; no live run shows them yet.
- **Make the user for the observer alone.** Give it no other role. Make the key for
  that user.
- **A live run used both roles.** `just live-observer-role` sent each request of the
  cycles with a second key. With Project Viewer, each request answered HTTP 200, and
  so did the request of the click "Ask Palette". With Cluster Viewer, each request
  except the audit log answered HTTP 200, and no run sent the request of the click.
  [observer-role.sh](../development/live-runs.md#observer-rolesh) lists the checks.

### The role for the read of the pods

The read adds a role and a token to the pod. The base of `deploy/observer` has
neither. [Add the read](#add-the-read) tells how to ask for them.

- **`list` on `pods` gives the whole spec of each pod, with its `env`.** Whoever holds
  the token reads every `env` of the cluster. A value there is in clear. So add the
  read only when your team decides to give this access. The observer decodes no `env`.
- **The role holds `get` and `list` on `pods`, and no other verb.** It is a
  `ClusterRole`, so it covers every namespace. It gives no write, and it gives no
  Secret.
- **The token expires after one hour.** The kubelet renews it, and the observer reads
  the file again at each call.
- **The egress rule adds TCP 6443 beside 443.** The pod can then connect to the API
  server of the cluster.

### The Secret for the alerts

The alerts add a Secret and a webhook to the cluster. [Add the alerts](#add-the-alerts)
tells how to ask for them.

- **The address of a webhook holds a token.** Whoever can read the Secret
  `strata-observer-alert-channels` can post to your channel. The observer shows the name
  of a channel on the page and in the log, and never its address.
- **The observer sends only to an `https` address.** It follows no redirect. It sends
  the message, and it reads only the status of the answer.
- **The component adds no role.** The alerts need no Kubernetes call.

## `re-baseline needed`

A new minor version of strata can move a root with no change of content. The
finding then has `reBaselineNeeded`, and the page shows `re-baseline needed` beside
the drift. The finding stays. The observer never writes a record. Write the records
again with the new version of strata. See [Versions and
compatibility](../reference/compatibility.md) for each change that moves a root.

## Next

- [The observer shows an edit in place](../use-cases/observer.md): the problem, the
  commands, and the fix.
- [The observer of a set of clusters](./observer-clusters.md): one observer for the clusters of some labels.
- [The API of the observer](../reference/observer-api.md): the routes, the two
  tiers, the fields that no answer carries, and the alerts.
- [The checks of the observer](../development/observer.md): the command of each
  step, each finding, the summary, and the flags.
- [The server of the observer](../development/observer-api.md): the screens of the
  page, each state, and the text of a diff.
