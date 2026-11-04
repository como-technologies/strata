# The API of the observer

`strata-observer serve` serves its answers on a loopback address. The API has two
tiers. The summary tier shows no value. The detail tier shows the old and the new
value of one pack. [The observer](../automation/observer.md) tells what the service
does and how to install it.

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

The Kubernetes verb `create` on `pods/portforward` is the gate. Another pod that
connects to the address of the pod gets `Connection refused`. A person without that
verb cannot open the page. [The API of the observer](../development/observer-api.md#the-gate)
shows how to test the verb.

## The routes

Only `GET` is allowed. Another method gets `405` and the header `Allow: GET`.

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
| Detail | `/v1/packs/{profile}/{layer}/{name}/diff?side=` | The changes of one pack, with the old and the new value. |

Each summary route serves a part of [the summary](../development/observer.md#the-summary),
with the same field names. Each answer is JSON, except `/healthz`, which is plain text.
The route `/` serves the page of the dashboard.

- `{name}` and `{profile}` are `name@version` of a profile. A name with no version
  works when only one profile has that name.
- `/v1/cluster` gives the fields of the summary up to `counts`. `page` has one value
  of [the page state](../development/observer.md#the-page-state).
- A profile holds `status`, `published` and `clusterCopy` with their roots, its packs
  with their `status` and `flags`, its `findings`, and `who`.

| Status | When |
| --- | --- |
| `400` | The detail route has a `side` that is not `published`, `cluster`, or `newer`, or has no `side`. |
| `404` | A profile, a pack, or a diff does not exist. The body has a fixed text and names no part of the request. |
| `405` | The method is not `GET`. |
| `421` | The `Host` header is not `localhost` or a loopback address. A person can choose the local port, so the port can be any port. |
| `502` | The detail route asked for a compare of the `newer` side, and the compare failed. The next request asks again. |
| `503` | The detail route waited too long for a cycle to end. |

## The detail route

`side` is `published`, `cluster`, or `newer`.

| `side` | What it compares | Where the values come from |
| --- | --- | --- |
| `published` | The record with the version that Palette holds now. | The last good content cycle. |
| `cluster` | The record with the copy of the profile in the cluster. | The last good content cycle. |
| `newer` | The pinned version of the pack with its newest version. | The last good versions cycle. |

The answer holds the `profile`, the `key`, the `side`, the `status`, the `impact`,
and the `flags` of the pack. It also holds these lists:

| Field | Content |
| --- | --- |
| `values.settings`, `values.followsTheVersion`, `values.repeatsTheLists` | Each changed value with its `op`, `path`, `old`, and `new`, in the group of the fold that the summary counts. |
| `imageChanges`, `chartChanges` | The `op` and the `name` of each change, and the old and the new tag or version. |
| `masked` | The paths that Palette masks. They have no value. |
| `manifests`, `paramChanges` | The `op` and the `name` of each change. |

A pack with no diff on a side gets `404`.

The state keeps the counts of a move and never its values. After a restart, the
memory holds no values for a move that the state knows. The route then asks for one
`compare pack:... pack:...`, and keeps the answer in memory. It sends no `POST`.

## What no answer carries

No tier carries `a`, `b`, `cluster.variables`, `clusterVariables`,
`variables.diff[]`, a whole record, the raw directory, or the environment of a pod.
The observer reads the documents of `strata` through an allow-list that has no
field for them, so they never enter the process.

The API key stays in the environment of the process. It goes to no answer and to no
log. No route shows the environment. The server writes no request log, because a
path can hold the name of a profile.

The detail tier is the one place where a value leaves the process. It shows the old
and the new value of a pack. A secret in a value that the schema does not mark as
masked passes through. See [What you give the
observer](../automation/observer.md#what-you-give-the-observer).
