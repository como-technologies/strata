# The API of the observer

`strata-observer serve` runs the two timers of [the checks of the
observer](./observer.md) and serves the answers on a loopback address. The API
has two tiers. The summary tier shows no value. The detail tier shows the old
and the new value of one pack.

## Run it

```bash
strata-observer serve --cluster-uid UID --records DIR --work DIR --state DIR [--listen ADDR]
strata-observer health [--listen ADDR]
```

`serve` takes the flags of [`once`](./observer.md#run-it-once) and one more
flag. It starts the first cycle of each timer at the start. It stops when it
gets `SIGINT` or `SIGTERM`: it stops the cycle that runs, and it ends the server
after 5 seconds at most.

| Flag | Default | What it does |
| --- | --- | --- |
| `--listen` | `127.0.0.1:8080` | Names a loopback IP address and a port. A name such as `localhost` is not allowed. |

| Exit of `serve` | Meaning |
| --- | --- |
| `0` | The process stopped on a signal. |
| `2` | A flag is wrong, the address is not a loopback address, or the server failed. |

`health` makes one `GET` to `/healthz` and waits 3 seconds at most. It exits 0
when the answer is `200`. It exits 1 when the server does not answer or gives
another status. It exits 2 when a flag is wrong, or when `--listen` is not a
loopback address. The probes of the Deployment run it.

## The gate

The server listens on a loopback address, and the pod has no code for
authentication. A person reaches the port with `kubectl port-forward`. The
Kubernetes verb `create` on `pods/portforward` is the gate.

- A process that listens on `127.0.0.1` in a pod answers through
  `kubectl port-forward`.
- Another pod that connects to the address of the pod gets `Connection
  refused`.
- The API proxy of the pod gives `ServiceUnavailable`.

A role that has `pods/portforward` and `get` and `list` on pods gives `yes` for
`create` on the subresource. It gives `no` for `get secrets` and for `exec`.
Test it with the flag `--subresource`:

```bash
kubectl auth can-i create pods --subresource=portforward
```

Do not write `pods/portforward` as the resource. `kubectl` then reads it as a
pod with that name.

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

Each summary route serves a part of the summary in [The
summary](./observer.md#the-summary), with the same field names. Each answer is
JSON, except `/healthz`, which is plain text. The route `/` is for the page of
the dashboard. This version has no page, so `/` gives `404`.

- `{name}` and `{profile}` are `name@version` of a profile. A name with no
  version works when only one profile has that name.
- `/v1/cluster` gives the fields of the summary up to `counts`. `page` has one
  value of [the page state](./observer.md#the-page-state).
- A profile holds `status`, `published` and `clusterCopy` with their roots, its
  packs with their `status` and `flags`, its `findings`, and `who`.
- A route with a profile, a pack, or a diff that does not exist gets `404`. The
  body has a fixed text and names no part of the request.

### The detail route

`side` is `published`, `cluster`, or `newer`. A request with another value, or
with no `side`, gets `400`.

| `side` | What it compares | Where the values come from |
| --- | --- | --- |
| `published` | The record with the version that Palette holds now. | The last good content cycle. |
| `cluster` | The record with the copy of the profile in the cluster. | The last good content cycle. |
| `newer` | The pinned version of the pack with its newest version. | The last good versions cycle. |

The answer holds the `profile`, the `key`, the `side`, the `status`, the
`impact`, and the `flags` of the pack. It also holds these lists:

| Field | Content |
| --- | --- |
| `values.settings`, `values.followsTheVersion`, `values.repeatsTheLists` | Each changed value with its `op`, `path`, `old`, and `new`, in the group of the fold that the summary counts. |
| `imageChanges`, `chartChanges` | The `op` and the `name` of each change, and the old and the new tag or version. |
| `masked` | The paths that Palette masks. They have no value. |
| `manifests`, `paramChanges` | The `op` and the `name` of each change. |

A pack with no diff on a side gets `404`.

### The newer side after a restart

The state keeps the counts of a move and never its values. After a restart, the
memory holds no values for a move that the state knows. The detail route then
asks the worker for one `compare pack:... pack:...`, and keeps the answer in
memory. The request waits when a cycle runs, because one `strata` process runs
at a time. It sends no `POST`.

A request that waits too long gets `503`. A compare that fails gets `502`, and
the next request asks again.

## What no answer carries

No tier carries `a`, `b`, `cluster.variables`, `clusterVariables`,
`variables.diff[]`, a whole record, the raw directory, or the environment of a
pod. The allow-list of `internal/docs` has no field for them, so they never
enter the process.

The summary tier uses types of its own. None of them has a field of the type
`docs.Leaf`, so the compiler keeps a value out. A test walks the types.

The API key stays in the environment of the process. It goes to no answer and
to no log. No route shows the environment, and there is no `/debug`. A request
log is not written, because a path can hold the name of a profile.

## The rules of the server

- **Loopback only.** `--listen` takes an IP address that is a loopback address.
  Any other value exits 2 with a message, and the server binds nothing.
- **A loopback Host.** The server refuses a request whose `Host` is not
  `localhost` or a loopback address, with any port. It answers `421`. A person
  can choose the local port of a port-forward, so the port can be any port. The
  rule stops DNS rebinding from a page in the browser of the person.
- **Four headers on each answer,** also on `404`, `405`, and `421`:

  ```text
  Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Cache-Control: no-store
  ```

- **Four timeouts.** The server sets `ReadHeaderTimeout`, `ReadTimeout`,
  `WriteTimeout`, and `IdleTimeout`.
- **One HTTP client.** The only client is the `health` check. It calls a
  loopback address. The code sends no `POST`, and only `internal/api` imports
  `net` or `net/http`. A test makes sure of both rules.
