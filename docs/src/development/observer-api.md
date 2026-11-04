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

`serve` takes the [flags of `once`](../automation/observer.md#options) and one more
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

[The API of the observer](../reference/observer-api.md) lists each route, each tier, the detail route, and
the status codes. This page tells how the code keeps them.

## The page

The route `/` serves the dashboard. The page is four files, two icons, and two fonts,
and the program carries them. A person opens it through the port-forward of
[the gate](#the-gate). `observer/internal/web/assets/` holds these files:

| File | What it holds |
| --- | --- |
| `index.html` | The frame: the nav, the control of the theme, the top bar, and the place of each screen. |
| `app.js` | The routes, the screens, and the read of the API. |
| `theme.js` | The choice of a theme. It runs in the head, before the first paint. |
| `app.css` | The style, the colors, and the two `@font-face` rules. |
| `favicon-light.svg`, `favicon-dark.svg` | The mark of the logo, one file for each color scheme. |
| `fonts/` | Public Sans and Roboto Mono, as variable Latin files. |
| `LICENSES.md` | The version, the checksum, and the license of each font. |

The page reads the six summary routes at the start, and again each 30 seconds.
It reads the detail route when a person opens a pack. The hash of the address
names the screen:

| Hash | Screen |
| --- | --- |
| `#overview` | The tiles, the findings, the events, what a check does not cover, and the profiles. |
| `#findings` | The findings, and the newer versions. |
| `#profiles` | The table of the profiles. |
| `#newer` | The newer versions, by profile. |
| `#notchecked` | The causes of "Not checked", and what a check does not cover. |
| `#p-<name@version>` | One profile: its two sides, its events, and its packs. |
| `#k-<name@version>/<layer>/<name>` | One pack: its comparison with the record, and its move to the newest version. |

The comparison of a pack shows the settings as a table. It folds "Follows the
version" and "Repeats the lists" with their counts. It lists the operation and
the name of each manifest, and does not show the text of a manifest.

### The state of the page

`page` from `/v1/cluster` sets the state. The top bar shows the two clocks:
the content check and the read of the versions. The page never shows one "last
checked".

| `page` | The page shows |
| --- | --- |
| `ok` | The data. |
| `first-load` | A violet bar. The tiles show `-`. |
| `no-records` | A violet bar with the text about records. The tiles show `-`, and the page lists no newer version, because `behind` needs a record. |
| `stale` | An amber bar with the time of the last content check. The counts are from then. |
| `lost` | A red bar. The counts are from the last good check, and no finding clears until a check proves it. |

A restart in an outage gives `stale` or `lost` with `checkedAt` null, because no
check has ended well. The memory holds no answer, so each count is 0. The page
then shows `-` and "Not read yet". No screen says "Nothing changed", "Nothing
needs attention", or "All 0 packs current".

The baseline of the records adds to each screen. With `first-seen`, the top bar
has a pill, and each screen says "No person approved these records". With
`unknown`, the top bar says "Baseline unknown".

### What the page keeps

- Four kinds of news, and no single color of health. A drift is red. A newer
  version stays neutral at each level. "Not checked" is grey, and it never
  raises a severity.
- "Not checked" blocks the text "No change". A pack that was not checked never
  reads "current".
- A side that strata could not read says "Not read". It never reads "Identical".
  The published side of a pack with an `error` row has no answer, and the route
  gives 404 for it.
- A move that strata could not compare has no counts. Its row shows a dash and
  "Not read", and "Not checked" names its pack.
- A detail request that fails is not kept. A 503 is a normal answer while a
  cycle holds the slot. The next render that a person or the clock causes asks
  again. A render that an answer causes does not, so two requests that fail do
  not ask each other again without end.
- `re-baseline needed` sits beside a drift and does not replace it.
- `same-content` reads "Same content" on the row of the cluster copy. It is not
  a drift.
- The page says nothing that it cannot know: not what a new pod pulls, and not
  what the pipeline of a team does.

### The colors and the theme

The page has a light theme and a dark theme. A control at the foot of the nav
has three buttons: Auto, Light, and Dark. Each button says its state with
`aria-pressed`. Auto follows the system, and it is the default. Light and Dark
override the system.

The choice stays in the `localStorage` of this browser. It goes nowhere else.
When a browser refuses the storage, the choice lasts for the page. When a person
chooses Auto again, the page removes the stored value.

`theme.js` is a file of its own in the head, and it is not part of `app.js`. It
runs before the first paint, so a person who chose Dark sees no white flash. The
policy allows it because it is a file of the server. It sets `data-theme` on the
root, and `app.css` reads that attribute.

Each color is one line of `app.css`, in the form `light-dark(light, dark)`. The
file has no `prefers-color-scheme` rule, so no color has a second copy.
`data-theme` sets `color-scheme` and no color. The colors have these meanings:

| Color | Use |
| --- | --- |
| Purple | The accent: links, the selected filter, and the focus. |
| Violet | A newer version. It stays neutral. |
| Amber | A warning: stale, first-seen, and repave. |
| Pink | The chip "new", the logo, and the brand line over the top bar. It is no status. |
| Red | A drift. The pill is white on `#c9252f` in both themes. |
| Green | OK. |

The logo is the mark of four tilted slabs in the colors of the brand line. The
nav is dark in both themes, so the nav shows the dark mark.

### The files

The server sends each file with the type of its kind: `html`, `js`, `css`,
`svg`, `woff2`, and `md`. A file of another kind is not served. The page links
two SVG icons, one for each color scheme. A browser that does not use them asks
for `/favicon.ico`. That path gives `204`, and the console of the browser shows
no error.

The policy of the server allows the script, the style, the fonts, and the icons
from the server itself. A cluster can have no egress, so the fonts and the icons
are files, and the page asks no other host for anything. The policy stops an inline
style, so an icon is two files and not one file with a `<style>`. So the files of
`assets/` keep these rules, and a test of `internal/web` fails when one breaks
them:

- No address of another host: no `http://` and no `https://`, except in `LICENSES.md`
  and in the `xmlns` of an icon.
- No inline style and no inline script: no `style=`, no `<style>`, and no `<script>`
  with content. No event attribute in `index.html`, in an icon, or in a string of `app.js`.
- One write of markup. `app.js` writes in one function, `put`, and every value
  goes through `e()`, which escapes `&`, `<`, `>`, and both kinds of quote. A name
  that holds `<img src=x>` shows as text. `theme.js` writes no markup, and it sets
  only the attributes `data-theme` and `aria-pressed`.
- No `max-width` on `.page`, `.main`, `.app`, or `body`. The content fills the window.
- No copy of the logo that differs from its source. `assets/logo/` at the root
  of the repository holds the six files of the logo. The two icons, the favicon of
  the book, and the inline mark in `index.html` copy the mark from it.

### The layout

- At 1024 pixels and below, the nav becomes a bar at the top. Its items scroll
  sideways.
- At 760 pixels and below, the compact layout starts. A table drops its columns
  with the class `narrow-hide`. In the events table, the object moves under the
  message.
- The tiles fill each row. The page never scrolls sideways: a table that does
  not fit scrolls inside its own card.

### Try each state

`just observer-dev` serves the page from test data. It needs no `strata` binary,
no records, and no Palette.

```bash
just observer-dev          # the set ok
just observer-dev stale    # the set stale
```

The recipe runs `go -C observer run ./internal/web/devserver -state STATE`. The
sets are `ok`, `quiet`, `first-load`, `no-records`, `stale`, `lost`,
`first-seen`, `stale-unchecked`, `lost-unchecked`, and `read-error`. The server
listens on `127.0.0.1:8080`. The flag `-listen` takes another loopback address,
and `-data` takes another directory of sets.

A set is a directory of `observer/internal/web/testdata/api/`. It holds the answer
of each route as a JSON file. The dev server serves them through the real server
of `internal/api`, so the headers are those of the real server. It moves each
time of a set by the gap between now and the time of the data. So the ages on
the page are those of the data.

- The set `ok` holds made-up data: 12 profiles, 29 packs, 1 finding, 10 newer
  versions in 6 profiles, 2 causes of "Not checked", and 26 of 29 packs in full.
- The sets `stale`, `lost`, and `first-seen` name `ok` in a file `base`. They differ in
  `cluster.json` only.
- The sets `stale-unchecked` and `lost-unchecked` name `first-load` in a file
  `base`. They differ in `cluster.json` only. They show a page that no good
  content check has ended for.
- The set `read-error` holds 3 profiles. One has a `verify` row that strata could
  not read, and one has a cluster side that it could not read. One has a move
  that it could not compare.
- The set `no-records` is the output of the observer with no record. A test runs
  the observer on a stand-in and makes sure of this.

A test decodes each file of each set into the type of its route, with no field
to spare. So the test data cannot drift from the API. When `node` is on the
`PATH`, `go test` also renders each screen of each set with `app.js`. It fails on
a screen that throws, and on a value that reaches the page as markup. It skips
these tests when `node` is not there. It cannot see the layout, so a person looks
at the render.

## What no answer carries

[The API of the observer](../reference/observer-api.md#what-no-answer-carries) lists the fields that no
answer carries. The allow-list of `internal/docs` has no field for them, so they never
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
  Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Cache-Control: no-store
  ```

- **Four timeouts.** The server sets `ReadHeaderTimeout`, `ReadTimeout`,
  `WriteTimeout`, and `IdleTimeout`.
- **One HTTP client.** The only client is the `health` check. It calls a
  loopback address. The code sends no `POST`, and only `internal/api` imports
  `net` or `net/http`. A test makes sure of both rules.
