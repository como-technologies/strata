# The observer shows an edit in place

## The problem

A person edits a published profile version through the API, and the number of the
version stays. The scheduled verify job of your pipeline finds the edit at its next
run, and it opens an issue. Between the edit and that run, a new cluster can take
the edited content. See [A published version is edited in place](./edited-in-place.md).

The observer runs in the cluster and checks every 15 minutes. It also keeps a page
that answers three more questions: who made the edit, whether this cluster took it,
and which pins have newer versions. It does this with the `strata` commands that
the other pages show. It adds no rule. The page shows the text of each diff, and a
button asks Palette about a move in step 5. An optional read of the pods adds a
fourth question in step 6: which digest each tag runs. An optional alert in step 7
sends a message to a webhook when a finding opens.

In this example, `dev` has `aws-stack@1.0.0` and `aws-stack@1.1.0`. The cluster
`payments` runs `aws-stack@1.0.0` and the add-on profile `platform-addons@1.0.0`.
The cluster `ledger` runs `aws-stack@1.1.0`.

**Who runs this.** The observer runs it in the cluster, and a person opens the
page. [The observer](../automation/observer.md) gives the install.

<div class="playground-demo">

`just playground observer-2` serves this tenant on this machine, with no key and
no cost. `just observer-playground` also runs the observer against it, and serves
its page. An edit in place changes the tenant, so the page has two seeds.
`observer-1` is the tenant before the edit.

</div>

## 1. The pipeline records what the clusters run

The pipeline of the team records each profile that its clusters run, and it writes
the records into the cluster as Secrets. The observer reads them as files:
`aws-stack_1.0.0.json.gz`, `aws-stack_1.1.0.json.gz`, and
`platform-addons_1.0.0.json.gz`, with `meta.json`. See
[Install](../automation/observer.md#install).

At this time, nobody has edited a version. The page shows no finding.

## 2. A person edits the version

A person changes the CSI reclaim policy of `aws-stack@1.0.0` from `Delete` to
`Retain`, through the API. The cluster `payments` has not taken the update. The
next content cycle shows the edit:

```text
{{#include ../output/observer-once.txt}}
```

Exit `1`, as with `strata verify`. The finding has the kind `published-changed`.
It names the pack `csi/csi-aws-ebs`, the impact, and who made the write. The
`firstSeen` time is the end of the first cycle that found it.

`clusterCopy` of the profile is `same`: the copy that Palette holds for `payments` has
the content of the record. The edit reaches that copy only when somebody accepts the
update.
[Read the two verdicts as a pair](./edited-in-place.md#read-the-two-verdicts-as-a-pair)
gives the choices for each case.

The page also lists the pins that have newer versions. They are not a fault, and
the page keeps them neutral:

```text
{{#include ../output/observer-newer.txt}}
```

See [The registry has newer versions](./behind.md).

## 3. A cluster differs from its release

The cluster `ledger` runs `aws-stack@1.1.0`, and a person changed the copy that Palette
holds for the cluster. The observer for `ledger` checks each record, so it reports the
edit of `aws-stack@1.0.0` too:

```text
{{#include ../output/observer-ledger.txt}}
```

The finding `cluster-differs` says that the copy of the cluster differs from the
record. See [A cluster does not run its release](./cluster-release.md).

## 4. Read the page

A person opens the page with a port-forward:

```bash
kubectl -n strata-observer port-forward deploy/strata-observer 8080
```

The overview shows the finding, the newer versions, and what the check did not
cover. The page of the profile shows its two sides, its events, and its packs. The
page of the pack shows the old and the new value of the reclaim policy in a table.
[The API of the observer](../reference/observer-api.md) names the routes that carry each of them.

The Values tab then shows the text of the same edit as a diff block. It is the text
that `strata verify --full` prints, and the cycle reads it with `--full`:

```text
{{#include ../output/observer-text.txt}}
```

The Manifests tab shows the text of each changed, added, or removed manifest. A
control "Unified / Split" on both tabs chooses between two layouts of the same text.
Palette writes the mask `********` over a secret, and the text keeps it as a
placeholder.

A pack with more than 256 KiB of text for a side shows no text. The page says that
the text is too large, and it gives the command `strata compare ... --full`. [The
text of a diff](../automation/observer.md#the-text-of-a-diff) gives the rules. The
check `diff-text` of [What the live example verifies](../development/live-runs.md)
tests the text on a cluster that Palette built.

## 5. Ask Palette about a move

The newer versions of step 2 are not a fault. Before a person moves a pin, one more
fact helps: does Palette give an error for the move? The button "Ask Palette" answers
it. It sits in the table of the newer versions. It also sits on the card "Move to" of
a pack, where it reads "Ask Palette about this move".

A click sends one `POST` to Palette. The observer runs `strata behind` with `--pin`,
on the record of the profile, so the question is about one pack. The answer reads in
one of three ways:

- **Palette gave no error.** Palette checked the default values of the new version.
  It did not check the values of your team: see [Palette accepts the move, or refuses
  it](../commands/behind.md#palette-accepts-the-move-or-refuses-it).
- **`Palette refuses: <code>`.** Palette gave an error, such as
  `DependencyPackUpgradeNotSupported`. The page shows the message of Palette and the
  clusters that it names. See [The upgrade
  dependency](../internals/palette/validate-routes.md#the-upgrade-dependency).
- **Palette gave no answer.** A request failed. The page shows the HTTP status. This
  is no finding, and it raises no severity.

A second click on an answered move sends nothing. The observer asks again after
"Palette gave no answer". Nothing asks on a timer. The checks `ask-one-post` and
`ask-words` of [What the live example verifies](../development/live-runs.md) test
the click on a cluster that Palette built.

## 6. Read the digest of each tag

The steps above use Palette alone. A vendor can also rebuild an image under the tag
that a pack names. Palette gets no notice, and no record changes: see [A vendor
rebuilds an image under one tag](./image-tag.md). The pods show the rebuild
once a node pulls the image.

The read of the pods is optional. It needs a role and a token, and
[The pods of the cluster](../automation/observer.md#the-pods-of-the-cluster) gives
the cost. To add it, name the component in the kustomization of the observer:

```yaml
components:
  - deploy/observer/pods
```

Apply the kustomization again. Then open the page as in step 4:

```bash
kubectl apply -k .
```

The Images screen lists each image that the pods run. A row shows the reference, the
pack that it matches, and each digest with the number of pods that run it. An image
that matches no pack, such as an agent of Palette, shows as "no pack". Two findings
can follow:

- **`image-split`.** Pods run one tag with two digests. A new node pulled the rebuilt
  image, and the old nodes keep the old one.
- **`image-moved`.** A tag runs another digest than the first one that the observer
  saw. Each pod that started runs the rebuilt image.

A finding of the pods is not a drift of the content. The pack text did not change, so
the Drift tile does not count it, and the status of the profile stays the same. The
list "Needs attention" shows it.

## 7. Send an alert when a finding opens

A page shows a finding to a person who opens it. An alert tells your team without
that visit. Palette has no event for a published version that changed, so the observer
sends one as a stopgap: see [Alerts](../automation/observer.md#alerts). The send is
off until you turn it on.

Write the rules in a file in Git. This file sends a finding at once to the team on
call, and it sends the newer versions in a weekly digest. A mute has an end date:

```json
{{#include ../../../examples/observer-alerts/alerts.json}}
```

Make the Secret with the address of each webhook, as in
[Turn them on](../automation/observer.md#turn-them-on). Then name the component and
the ConfigMap in the kustomization of the observer, and apply it again:

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

The nav item "Alerts" lists each rule with its state: Firing, Pending, Inactive, or
Muted. In this example, the edit of `aws-stack@1.0.0` opens a finding, and the rule
`drift` sends one message to the channel `platform-oncall`. The next cycle sends
nothing, because the finding stays open. When the version goes back to its record, a
check proves it, and one message tells that the finding cleared. A cycle that failed
clears nothing.

A message holds the cluster, the profile, its packs, one fixed sentence, and the time
of the first sight. It holds no value, so a channel of your team can show it.

## What to do

- Do not repair the edited version in place: that is a second edit. Publish a new
  version with the change through review, and move the clusters to it. See
  [What to do](./edited-in-place.md#what-to-do).
- Read `who` first. It names the person and the time of the write.
- When the version goes back to its record, the next content cycle clears the
  finding. A new record, written after a review, ends the finding too.
- A cluster that differs needs its own decision: see
  [A cluster does not run its release](./cluster-release.md).
- For `image-split` or `image-moved`, fix the pin, not the finding. Name the digest
  where the chart or the manifest reads it, or copy each image, by digest, to a
  registry with immutable tags. See [What to do](./image-tag.md#what-to-do).
- `image-moved` clears when the digest comes back, or when you write new records.
  `image-split` clears when each pod runs one digest.
- Give each mute an end date, and keep the file in Git. The file shows who muted what.
  A mute that has ended counts as no mute.
- Read "Palette gave no error" as one fact, not as a promise. Palette did not check
  your own values. When Palette refuses a move, pick a closer version: see [What to
  do](./behind.md#what-to-do).
- After "Palette gave no answer", make sure that the key can read the profile, then
  click again. The role of the key does not need more than the read role: see [The
  least role of the key](../automation/observer.md#the-least-role-of-the-key).

## Limits

- The observer sees what Palette holds. It does not see an edit on the cluster with
  `kubectl`, or the health of a workload. See [What the observer does not see](../automation/observer.md#what-the-observer-does-not-see).
- The read of the pods sees a rebuilt tag only after a pod pulls the image. It stays
  silent when the pods pull an unchanged digest from a mirror, and that is right. See
  [What the read catches and what it does not](../automation/observer.md#what-the-read-catches-and-what-it-does-not).
- An alert fires on a finding. The vendor case of step 6 makes no finding, so it fires
  no alert. A dead observer sends nothing: alert on the Deployment with the monitoring
  of your team.
- A new pod sends the open findings again, once, because it starts with an empty state.
  One message too many is better than one too few.
- The check is as old as the last content cycle. The page shows the time of that
  cycle, and it shows `stale` when the cycle is more than two periods old.
- The click asks about the newest version of a pin only. The observer keeps the
  answers in memory, so a restart of the pod forgets them.
