# The API of the observer

`strata-observer serve` serves its answers on a loopback address. The API has two
tiers. The summary tier shows no value. The detail tier shows the old and the new
value of one pack and the text of its diff. It also shows the answers of Palette to
a click. [The observer](../automation/observer.md) tells what the service does and
how to install it.

**The API has no promise yet.** A release can change a route, a field, or a status.
The Rust API of strata has no promise at version 0.x either: see
[Versions and compatibility](./compatibility.md). The promise of the command line
does not cover this API. A script that reads it must expect a change in any
minor release.

## How to reach it

The server listens on `127.0.0.1`, and the pod has no code for authentication. A
person reaches the port with a port-forward:

```bash
kubectl -n strata-observer port-forward deploy/strata-observer 8080
```

The Kubernetes verb `create` on `pods/portforward` is the gate. Another pod cannot
connect to the address of the pod. A person without that verb cannot open the page.
[The server of the observer](../development/observer-api.md#the-gate) shows how to test
the verb.

The page and `curl` both work. A `GET` needs nothing more than the port-forward. The
clicks "Ask Palette" and "Check now" are each a `POST`, and each needs the header
`Sec-Fetch-Site: same-origin`.
A browser sets that header by itself for a request that the page sends. A `curl` call
must send it:

```bash
curl -s -X POST -H 'Sec-Fetch-Site: same-origin' \
  http://127.0.0.1:8080/v1/packs/aws-stack@1.1.0/k8s/kubernetes/ask
```

## The routes

Each route takes `GET`, except the clicks "Ask Palette" and "Check now", which take
`POST`. Another method gets `405` and the header `Allow` with the method of the route.

| Tier | Route | What it carries |
| --- | --- | --- |
| Health | `/healthz` | The text `ok`. It stays `200` when the data is stale, so that a restart does not hide a stale page. |
| Summary | `/v1/cluster` | The name and the uid, `page`, `strataVersion`, the two clocks, `records`, and `counts`. |
| Summary | `/v1/findings` | `findings`. |
| Summary | `/v1/profiles` | `profiles`. Each profile also holds its `findings` and its `who`. |
| Summary | `/v1/profiles/{name}` | One profile, as in the list. |
| Summary | `/v1/newer` | `newer`. Each row holds the counts of its move. |
| Summary | `/v1/notchecked` | `notChecked` and `limits`. |
| Summary | `/v1/events` | `events`: the last 7 days. |
| Summary | `/v1/images` | The document `strata-observed/1`: the images that the pods run, with the digest of each. |
| Summary | `/v1/images/{ref}` | One image of that document. `{ref}` is the reference, as one escaped segment. |
| Summary | `/v1/alerts` | The rules of the alerts with their states, the names of the channels, and a sample message. It holds no address. |
| Detail | `/v1/packs/{profile}/{layer}/{name}/diff?side=` | The changes of one pack, with the old and the new value and the text of the diff. |
| Detail | `/v1/asks` | `asks`: the answers of Palette that the observer holds in memory. |
| Detail | `POST /v1/packs/{profile}/{layer}/{name}/ask` | The click "Ask Palette": the answer of Palette about the move of one pack. |
| Cycle | `POST /v1/cycles/{cycle}` | The click "Check now": the content cycle or the versions cycle starts now. See [Check now](#check-now). |

Each summary route serves a part of [the summary](../development/observer.md#the-summary),
with the same field names. The two image routes are the exception: they serve
[`strata-observed/1`](#the-images). The route `/v1/alerts` is the other exception: it
serves [the alerts](#the-alerts). Each answer is JSON, except `/healthz`, which is
plain text. The route `/` serves the page of the dashboard.

- `{name}` and `{profile}` are `name@version` of a profile. A name with no version
  works when only one profile has that name.
- `/v1/cluster` gives the fields of the summary up to `counts`. `page` has one value
  of [the page state](../development/observer.md#the-page-state).
- A profile holds `status`, `published` and `clusterCopy` with their roots, its packs
  with their `status` and `flags`, its `findings`, and `who`. A profile with a newer
  published version also holds `newerVersion`: `version`, `created`, and `new`.
- `newerVersion.move` is what a move from the record to that version changes, from
  the compare of the two: `impact`, `flags`, and `packs[]`. Each pack that the move
  changes, adds, or removes has `key`, `status`, `from`, `to`, `flags`, and
  `valuesChanged`. `from` is absent for a pack that the move adds, and `to` for a
  pack that it removes. When the compare failed, `move` is absent and `unread` is
  true. An observer of 0.32.2 and before gives neither.
- `counts` of `/v1/cluster` holds `newerProfileVersions`, the profiles with a newer
  published version. An observer of 0.32.2 and before does not give it.

| Status | When |
| --- | --- |
| `400` | The detail route has a `side` that is not `published`, `cluster`, `newer`, or `newest`, or has no `side`. |
| `403` | The click has no header `Sec-Fetch-Site: same-origin`, or the header has another value. The body has a fixed text. |
| `404` | A profile, a pack, a diff, a move, or an image does not exist. The body has a fixed text and names no part of the request. |
| `405` | The method is not the method of the route. The header `Allow` names it. |
| `421` | The `Host` header is not `localhost` or a loopback address. A person can choose the local port, so the port can be any port. |
| `502` | The detail route asked for a compare of the `newer` side, and the compare failed. Or the click failed in the observer. The next request asks again. |
| `503` | The detail route or the click did not end in 50 seconds. The usual cause of a click is a versions cycle that holds its slot. |

## The images

`strata-observed/1` is the document of the pods. It is no tier of a record, and it
moves no root. It holds no pod name, no namespace, and no `env`. `GET /v1/images`
serves the whole document. `GET /v1/images/{ref}` serves `schema`, `pods`, `readAt`,
and `error`, with one row in the field `image`. It gives `404` for a reference that
no pod runs.

A reference holds `/` and `:`, so a client escapes the whole reference as one
segment of the path. `encodeURIComponent` does it in a browser. Without
`--read-pods`, no reference exists, so every `/v1/images/{ref}` gives `404`.

| Field | Type | Content |
| --- | --- | --- |
| `schema` | text | `strata-observed/1`. |
| `pods` | text | The state of the read: `off`, `not-read`, or `read`. |
| `readAt` | time or `null` | The end of the last good read. It is `null` when no read ended well. |
| `error` | object | The newest read, when it failed: its `kind`, and its `status` when the kind has one. The field is absent when the newest read worked. |
| `counts` | object | `images`, `matched`, and `changed`. |
| `images` | list | One row for each image reference. It is empty when `pods` is `off` or `not-read`. |

| `pods` | When |
| --- | --- |
| `off` | The flag `--read-pods` is not set. The observer makes no Kubernetes call. `images` is empty, and each count is 0. |
| `not-read` | The flag is set, and no read ended well. `images` is empty, and each count is 0. `error` names the failure. |
| `read` | A read ended well. A later read that fails keeps this state. `error` names that failure, and the rows are those of the last good read. |

`counts.images` is the number of rows. `counts.matched` is the number of rows whose
`match` is not `none`. `counts.changed` is the number of rows that make a finding.

A row of `images` has these fields:

| Field | Type | Content |
| --- | --- | --- |
| `ref` | text | The image of the spec of the pod, as the pod names it. It is never the `image` of the status, because the runtime can rewrite that one. |
| `status` | text | The status of the image: see the table below. |
| `match` | text | `exact`, `name-and-tag`, or `none`. The page labels `name-and-tag` "matched by name and tag", and it labels `none` "no pack". |
| `byDigest` | boolean | True when the reference names a digest. Such an image cannot move, so it makes no finding. |
| `packs` | list | Each pack that the image matches: `profile` as `name@version`, and `key` as `<layer>/<name>`. The list is empty when `match` is `none`. |
| `digests` | list | One entry for each digest, and one for the pods that have none. |

An entry of `digests` has these fields:

| Field | Type | Content |
| --- | --- | --- |
| `digest` | text | The digest, as `sha256:` and hex digits. The entry has no such field when its `status` is `no-repo-digest` or `not-started`. |
| `status` | text | The status of the digest: see the table below. |
| `pods` | number | The number of pods that run the digest. A pod counts once for an image, also when two of its containers run it. |

| `status` of an image | Meaning |
| --- | --- |
| `split` | Two digests or more run at one time. |
| `moved` | One digest runs, and it is not the first digest that the observer saw. |
| `steady` | One digest runs, and it is the first digest. |
| `not-checked` | No pod gave a digest. |

| `status` of a digest | Meaning |
| --- | --- |
| `first-digest` | The first digest that the observer saw for this reference. |
| `another-digest` | A digest other than the first one. |
| `no-repo-digest` | The image has an id and no repository digest, as a local image does. The page reads "Not checked", never "OK". |
| `not-started` | The container never started, so its `imageID` is empty. The page reads "Not checked", never "OK". |

The rows come in the order `split`, `moved`, `steady`, and `not-checked`, then by
`ref`. The digests come in the order `first-digest`, `another-digest`,
`no-repo-digest`, and `not-started`, then by digest. [The images](../development/observer.md#the-images)
gives the rule of each status.

### The error of a read

A read of the pods that fails is not a failed cycle. The document names the failure
by its kind and, for `kube-api`, its HTTP status. It never holds the text of the
answer, because that text can name an object.

| `kind` | `status` | When |
| --- | --- | --- |
| `kube-api` | The HTTP status | The API server answered with a status other than 200, such as 401 or 403. |
| `kube-transport` | none | A request got no answer, or the CA did not sign the certificate of the server. |
| `kube-config` | none | The file of the token or of the CA cannot be read, is empty, or holds no certificate. |
| `not-json` | none | An answer is not a list of pods. |
| `timeout` | none | A request took more than 30 seconds. |

A failed read also adds the cause `pods-not-read` to `/v1/notchecked`. Its row has an
empty `profiles`, an empty `packs`, and an empty `registry`, and it changes no
status of a profile.

### The findings of the pods

`/v1/findings`, `/v1/profiles`, and `/v1/events` carry two more kinds of finding:
`image-moved` and `image-split`. A finding of these kinds holds the profile, the pack
keys in `packs`, and the references of the images in the new field `images`. Other
kinds omit `images`. `impact` is empty, and `reBaselineNeeded` is false. Each kind
and profile gives one finding. The events have the reasons `ImageMoved` and
`ImageSplit`.

A finding of the pods changes no status of a profile or of a pack. `counts.drift` of
`/v1/cluster` does not count it, and `counts.findings` does.

## The alerts

`GET /v1/alerts` shows the rules of the alerts. It is a summary route: it names the
channels and holds no address and no value. The rules are a file that no route
writes. [Alerts](../automation/observer.md#alerts) tells what the observer sends.

| Field | Type | Content |
| --- | --- | --- |
| `set` | flag | `true` when the observer started with `--alerts`. When it is `false`, the other fields are empty. |
| `channels` | list of text | The names of the channels, in order. Never an address. |
| `digestEvery` | text | The period of a digest, as the other periods are: `168h` for 7 days. |
| `waiting` | number | The messages that did not go. The next content cycle sends them again. |
| `rules` | list | The rules, in the order of the file. |
| `sample` | text | The message of a finding for a made-up profile. It shows what a channel gets. |

A rule has these fields. `match`, `send`, `to`, and `muteUntil` are absent when the
rule has none.

| Field | Content |
| --- | --- |
| `level` | `cluster`, `profile`, or `pack`. |
| `match` | The profile or the pack that the rule is about. A `cluster` rule has none. |
| `news` | `drift`, `image`, `lost`, or `newer`. |
| `send` | `now` or `digest`. A mute has none. |
| `to` | The name of the channel. A mute has none. |
| `muteUntil` | The last day of a mute, as `YYYY-MM-DD`. Only a mute has it. |
| `state` | `firing`, `pending`, `inactive`, or `muted`. A mute that has ended is `inactive`. |

## The detail route

`side` is `published`, `cluster`, `newer`, or `newest`.

| `side` | What it compares | Where the values come from |
| --- | --- | --- |
| `published` | The record with the version that Palette holds now. | The last good content cycle. |
| `cluster` | The record with the copy of the profile in the cluster. | The last good content cycle. |
| `newer` | The pinned version of the pack with its newest version. | The last good versions cycle. |
| `newest` | The record with the newest published version of its profile. | The last good versions cycle. An observer of 0.32.2 and before does not have this side. |

The answer holds the `profile`, the `key`, the `side`, the `status`, the `impact`,
and the `flags` of the pack. It also holds these lists:

| Field | Content |
| --- | --- |
| `values.settings`, `values.followsTheVersion`, `values.repeatsTheLists` | Each changed value with its `op`, `path`, `old`, and `new`, in the group of the fold that the summary counts. |
| `values.unified` | The text of the raw values, as a unified diff. One string, empty when the pack has no text. |
| `imageChanges`, `chartChanges` | The `op` and the `name` of each change, and the old and the new tag or version. |
| `masked` | The paths that Palette masks. They have no value. |
| `manifests` | The `op`, the `name`, and the `unified` text of each changed, added, or removed manifest. `unified` is empty when the pack has no text. |
| `paramChanges` | The `op` and the `name` of each change. |
| `build` | Each field of the build of the pack that differs: `version`, `tag`, `packUid`, or `upstream`, with its value `a` before and `b` after. Empty for an added or a removed pack. |
| `textTooLarge` | True when the pack had text and the observer dropped all of it. |

A pack with no diff on a side gets `404`.

### The text of a diff

`values.unified` and `manifests[].unified` hold the text that `strata compare --full`
prints, and the cycles read it with `--full`: see [See the full
diff](../commands/compare/versions.md#see-the-full-diff). It is the text of the
records. The mask `********` that Palette writes over a secret stays a placeholder.
The text holds no value of a cluster variable.

A pack keeps 256 KiB of text at most for each side of a diff. The limit counts the
values and the manifests of the pack together. Above it, the observer drops all the
text of the pack: `values.unified` and each `manifests[].unified` are empty, and
`textTooLarge` is true. The settings, the `op` of each manifest, and its `name` stay.
Half a diff misleads, so a pack keeps all of its text or none.

When `textTooLarge` is true, the page says that the text is too large. It gives the
command `strata compare ... --full`, which prints the whole diff.

The text stays in the memory of the detail tier. It goes in no state file, no summary
route, and no log.

The state keeps the counts of a move and never its values. After a restart, the
memory holds no values for a move that the state knows. The route then asks for one
`compare pack:... pack:...`, and keeps the answer in memory. This route sends no
`POST`: only [the click](#ask-palette-about-a-move) does.

## Ask Palette about a move

`POST /v1/packs/{profile}/{layer}/{name}/ask` asks Palette whether it gives an error
for the move of one pack to its newest version. `{profile}` is `name@version`, and
the key is `{layer}/{name}`, as in the detail route. It reads no body.

The request needs these two things:

- A loopback `Host`, as for every route. Another `Host` gets `421`.
- The header `Sec-Fetch-Site: same-origin`. A browser sets it by itself for a request
  that the page sends. A page of another origin sends another value, or none, so it
  cannot send the `POST`. Another value, or no header, gets `403`.

The server checks the `Host` first, then the method, then the header. A `GET` on the
route gets `405` with `Allow: POST`. A profile, a pack, or a move that the observer
does not know gets `404`, and the observer sends nothing to Palette. The move must
be one that the last good versions cycle found for that profile.

The click runs `strata behind <record> --pin <layer>/<name>` once, on the record of
the profile. That sends one `POST /v1/clusterprofiles/{uid}/validate/packs` to
Palette for each record that pins the pack. A record of a profile version gives one.
It sends no other write. The click waits for the versions slot, so it waits for a
versions cycle that runs. [The page of `behind`](../commands/behind.md#palette-accepts-the-move-or-refuses-it)
says what the answer of Palette covers.

The reply is one answer, also when the request to Palette failed. The reply has the
status `200` then, and the verdict `no-answer`.

| Field | Type | Content |
| --- | --- | --- |
| `profile` | text | The profile, as `name@version`. |
| `key` | text | The pack, as `<layer>/<name>`. |
| `newest` | text | The version that the question names: the newest version of the pack. The registry can gain a version after the last versions cycle, and the version is then newer than the one of the row. A `no-answer` names the newest version of the row. |
| `verdict` | text | `no-error`, `refuses`, or `no-answer`: see the table below. |
| `refusals` | list | One entry for each error that Palette gave. It is empty unless the verdict is `refuses`. |
| `status` | number | The HTTP status of the request that failed. It is 0 when the request did not fail, or when the failure has no status. |

An entry of `refusals` has these fields:

| Field | Type | Content |
| --- | --- | --- |
| `pack` | text | The pack that the error names. |
| `code` | text | The code of Palette, such as `DependencyPackUpgradeNotSupported`. |
| `message` | text | The message of Palette. |
| `clusters` | list | The clusters that the message names. It is empty when the message names none. |

| `verdict` | Meaning | The page reads |
| --- | --- | --- |
| `no-error` | Palette gave no error. It checked the default values of the new version, and it did not check your own values. | Palette gave no error |
| `refuses` | Palette gave one error or more. | `Palette refuses: <code>` |
| `no-answer` | A request failed, so Palette gave no answer. `status` holds its HTTP status, or 0. | Palette gave no answer |

A request that failed, such as a refusal for the role of the key, is no finding and
raises no severity. The observer holds an answered move, with the verdict `no-error` or
`refuses`, in memory. It holds the answer until the pin moves or the pod restarts. A
second click on that move sends nothing, and the reply is the first answer. The
observer asks again at the next click after a `no-answer`. Nothing asks on a timer.

`GET /v1/asks` lists the answers that the observer holds, sorted by profile and key.
The page reads it to show the answers again after a reload. The answer is `{"asks": [...]}`
with the fields above, and it is an empty list when nobody asked. The list holds a
`no-answer` until the next click replaces it, or the move goes.

A refusal holds the message of Palette and the names of clusters. So only the detail
tier carries an answer. No summary route, no state file, and no log line holds it. The
log has one line for a click, with the verdict and the status.

## Check now

`POST /v1/cycles/{cycle}` starts a cycle now, and not at the end of its period.
`{cycle}` is `content` or `versions`. Another name gets `404`. The route reads no
body. It needs a loopback `Host` and the header `Sec-Fetch-Site: same-origin`, as the
click "Ask Palette" does.

The click runs the same reads as the timer of the cycle, with the same key. It writes
nothing. The reply is `{"cycle": "content", "state": "started"}`, with one of these
statuses:

| Status | `state` | Meaning |
| --- | --- | --- |
| `202` | `started` | The cycle starts now. After it, its timer waits a full period again. |
| `202` | `running` | A cycle of that kind runs now. The click joins it, and no second cycle starts. |
| `429` | `wait` | A cycle of that kind ended less than one minute ago. `retryAfter` and the header `Retry-After` give the seconds to wait. |

So a person who clicks again and again starts at most one cycle of each kind in one
minute and the time of its run. The clicks cannot flood Palette or the registries.

The observer also starts a content cycle when the records change. Every 15 seconds,
it compares the names, the sizes, and the times of change of the record files. It
reads no file for this, and it sends no request. The kubelet updates the files of a
mounted Secret after a delay that its sync period sets: one minute by default.

## What no answer carries

No tier carries `a`, `b`, `cluster.variables`, `clusterVariables`,
`variables.diff[]`, a whole record, the raw directory, the `env` of a pod, a pod
name, or a namespace. The observer reads the documents of `strata` through an
allow-list that has no field for them. The bytes can reach memory, but they reach
no type of the observer. So no answer and no log carries them.

The read of the pods works in the same way. Its types have no field for `env`, so the
bytes of `env` reach no type. The process holds the namespace and the name of a pod
in memory, to count the pods of an image. They reach no answer, no log, and no state.

The API key stays in the environment of the process. It goes to no answer and to no
log. No route shows the environment. The server writes no request log, because a
path can hold the name of a profile.

The address of a webhook stays in a file that the observer reads at each send. It goes
to no answer, no log, and no state. `/v1/alerts` names the channels only.

The detail tier is the one place where a value leaves the process. It shows the old
and the new value of a pack, the text of its diff, and the answers of Palette. A
secret in a value that the schema does not mark as masked passes through. See [What
you give the observer](../automation/observer.md#what-you-give-the-observer).
