# The server of the observer

`strata-observer serve` runs the two timers of [the checks of the
observer](./observer.md) and serves the answers on a loopback address. The API
has two tiers. The summary tier shows no value. The detail tier shows the old
and the new value of one pack and the text of its diff. It also shows the answers
of Palette to a click. The server takes one `POST`: see [The click](#the-click).

## Run it

```bash
strata-observer serve --cluster-uid UID --records DIR --work DIR --state DIR [--listen ADDR]
strata-observer health [--listen ADDR]
```

The flags `--alerts`, `--digest-every`, and `--alerts-allow-http` belong to
[Alerts](../automation/observer.md#alerts). `serve` reads the address of each channel
at the start, and it exits 2 when one cannot be used.

`serve` takes the [flags of `once`](../automation/observer.md#options) and one more
flag. It starts the first cycle of each timer at the start. It stops when it
gets `SIGINT` or `SIGTERM`. It stops the cycle that runs and each detail request
that runs, with the `strata` process of each. Then it ends the server after 5
seconds at most.

| Flag | Default | What it does |
| --- | --- | --- |
| `--listen` | `127.0.0.1:8080` | Names a loopback IP address and a port. A name such as `localhost` is not allowed. |

| Exit of `serve` | Meaning |
| --- | --- |
| `0` | The process stopped on a signal, also when a detail request ran. |
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
- Another pod cannot connect to the address of the pod. The check
  `other-pod-refused` prints `refused` for any failure of `wget`, so it does not
  name the cause.

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

The page reads the seven summary routes at the start, and again each 30 seconds.
It reads the detail route when a person opens a pack. The hash of the address
names the screen:

| Hash | Screen |
| --- | --- |
| `#overview` | The tiles, the findings, the events, what a check does not cover, and the profiles. |
| `#findings` | The findings, and the newer versions. |
| `#profiles` | The table of the profiles. |
| `#newer` | The newer versions, by profile. |
| `#notchecked` | The causes of "Not checked", and what a check does not cover. |
| `#images` | The images that the pods run, with the status of each. |
| `#i-<reference>` | One image: its digests, and the tabs "This image" and "What pods do catch". |
| `#p-<name@version>` | One profile: its two sides, its events, and its packs. |
| `#k-<name@version>/<layer>/<name>` | One pack: its comparison with the record, and its move to the newest version. |

Each pack page has one card of comparison with one dropdown "Compare". The dropdown
holds these options:

- **Record → Published now.** It is always there.
- **Record → This cluster's copy.** It is there when strata could read the copy.
- **Pack A → pack B.** It is there when a newer version of the pack exists.

The page opens on the first option that has a change, in this order, and on
"Published now" when none has one. The page decides this from the findings and the
newer versions, so it sends no request for a side with no change. That side shows
the splash "Undisturbed", with three facts: the published version, the copy of the
cluster, and the newer version. A published side that strata could not read or could
not verify shows "This side was not read". It never shows "Undisturbed".

A side with a change has four tabs:

- **Values.** It shows the settings as a table, and it folds "Follows the version"
  and "Repeats the lists" with their counts. Then it shows the masked note, and the
  text of the raw values as a diff block. A setting of many lines shows only the
  lines that differ: see [A value of many lines](../automation/observer.md#a-value-of-many-lines).
- **Manifests.** It shows the text of each changed, added, or removed manifest.
- **Images** and **Charts.** They show the entries that changed.

The card opens on the first tab that has a change. An empty tab says so, and it links
to the first other tab that has a change. A control "Unified / Split" on the tabs of
values and manifests chooses between two layouts of the same text. A pack with
`textTooLarge` shows the splash "Too much rock to carry", and it gives the command
`strata compare ... --full`. The table of the newer versions and the card "Move to"
of a pack show the button "Ask Palette". The text and the answers come from the
detail tier only.

### The five flags

The head of the card shows the five change flags. A flag keeps one color: lit or dim
is the signal. A color for each flag competes with the colors of the statuses.

- A hover or a focus on a flag shows what it means, lit or dim. The words come from
  the table in [Change flags and impact](../reference/impact.md), and `FLAG_WORDS` in
  `app.js` holds the one copy of them. A keyboard reaches the words too, because each
  flag takes the focus.
- A lit flag with a tab is a link to that tab. `AUTHORED` opens the first of Values and
  Manifests that has a change. `IMAGES` opens Images, and `CHARTS` opens Charts.
- `IDENTITY` and `UPSTREAM` have no tab, so they stay plain. A dim flag is no link.

### Check this yourself

The page of a profile and the page of a pack end with the card "Check this yourself".
Each row is a question, a command with a button Copy, and what the page says now. The
rows change with the page:

| Question | Command | The page says |
| --- | --- | --- |
| Get the record that this check used. | A loop over the four Secrets `strata-observer-records-0` to `3` that writes `record.json`. | The revision of the records. |
| Does Palette still have this version? | `strata verify record.json` | The status of the published version. |
| Who changed it? | `strata verify record.json --who` | The person and the age. The row is there when a finding names a person. |
| What changed? | `strata compare record.json <profile> --full` | The number of packs that changed. |
| Does this cluster still run the record? | `strata compare record.json cluster:<cluster>/<profile>` | The status of the copy. |
| What does the move change? | `strata compare pack:<registry>/<pack>@<old> pack:<registry>/<pack>@<new> --full` | The impact. A pack with a newer version has this row. |
| Which newer versions does Palette list? | `strata behind record.json --pin <layer>/<pack>` | The newest version. A pack with a newer version has this row. |

The page does not know which of the four Secrets holds a record, so the first command
reads the key in each one. The key is the name and the version of the profile, as
`scripts/ci/observer-records.sh` writes it. The names are real. A test of `internal/web`
fails when the Secrets, the namespace, or the rule of the key differ from the files of
the repository. `just observer-contract` draws the page from the routes of a real
server, and it runs each command against the stand-in of the chapter
`edited-in-place`. It also runs the two commands of a move against the chapter
`behind`.

### The state of the page

`page` from `/v1/cluster` sets the state. The top bar shows the two clocks:
the content check and the read of the versions. The page never shows one "last
checked".

| `page` | The page shows |
| --- | --- |
| `ok` | The data. |
| `first-load` | The splash "Reading the layers", with the tone `build`, on each screen that reads the checks. |
| `no-records` | The splash "Nothing to compare against yet", with the tone `off`, the command of the records script, and the command `--first-seen`. The page lists no newer version, because `behind` needs a record. |
| `stale` | An amber bar with the time of the last content check. The counts are from then, and the data stays under the bar. |
| `lost` | A red bar. The counts are from the last good check, no finding clears until a check proves it, and the data stays under the bar. |

A restart in an outage gives `stale` or `lost` with `checkedAt` null, because no
check has ended well. The memory holds no answer, so each count is 0. The page
then shows a splash with the error and the next step. A lost page shows "We lost the signal
from Palette", and a stale page shows "Checks stopped before one ended well". The nav shows `-`.
No screen says "Nothing changed", "Nothing needs attention", or "All 0 packs current".

A page that holds no data and cannot reach the observer shows the splash "This page lost its
line to the observer". The splash has the command `kubectl port-forward` and a button
"Try now". With data on the page, the same fault stays an amber bar over that data.
OLD

rep(<<'OLD', <<'NEW');
- A side that strata could not read says "Not read". It never reads "Identical".
  The published side of a pack with an `error` row has no answer, and the route
  gives 404 for it.
OLD
- A side that strata could not read says "Not read". It never reads "Identical" or
  "Undisturbed". The published side of a pack with an `error` row or an `unverified`
  status has no answer, and the page sends no request for it.

The baseline of the records adds to each screen. With `first-seen`, the top bar
has a pill, and each screen says "No person approved these records". With
`unknown`, the top bar says "Baseline unknown".

### The splash

One component draws each place that has nothing to show and each state that the page
tells: `splash(tone, title, body, extra, size)` in `app.js`. It draws the mark of the
logo, a headline, and a line that states the fact. The mark has the geometry of
`assets/logo/`: four paths, from the top slab down. `app.css` gives each slab its
color by class, because the policy of the page refuses an inline style. The function
`mark()` draws it through `h`, the one template of the script.

The tone sets the look of the mark and its animation:

| Tone | The mark | The animation | Use |
| --- | --- | --- | --- |
| `calm` | The four colors of the logo. | The slabs breathe. | The check ended well and found nothing. |
| `build` | The four colors. | The slabs stack. | A read is on its way. |
| `off` | An outline. | The dashes march. | Off by a choice of the team, or nothing to compare with. |
| `warn` | Amber. | The slabs pulse. | A read failed, or part of the layer is hidden. |
| `lost` | The top slab is red. | The top slab slips off. | Palette or the observer is out of reach. |

The size is the place: the whole content area (`lg`), a card (`md`), a card inside a
card (`sm`), and a line (`inline()`).

The rule of a state:

- The headline can have some fun. The line under it states the fact in plain words.
- It never reads calmer than the check allows. `calmOk()` is the one gate of the
  calm tone.
- It gives the next step when one exists, and the step has a command with a button
  Copy.
- A finding always shows its data and never a splash. A page that is lost or stale
  after a good check stays a bar over its findings.
- It states no time that the page does not hold. Each time comes from the data.
- `prefers-reduced-motion` stops each animation in `app.css`. Each mark holds its
  state.

`calmOk()` is true when all of these hold:

- The page holds data, and it is neither lost nor stale.
- No finding is open.
- No read failed.
- No cause is in "Not checked".

On a page that fails the gate, no screen shows a calm tone. A place that a limit touches
gets an amber mark, and any other place gets an outline.

These places have a splash:

| Place | Tone | Size | Headline |
| --- | --- | --- | --- |
| The first check runs | `build` | `lg` | Reading the layers |
| No records | `off` | `lg` | Nothing to compare against yet |
| Lost, and nothing was read | `lost` | `lg` | We lost the signal from Palette |
| Checks stopped, and nothing was read | `warn` | `lg` | Checks stopped before one ended well |
| The page cannot reach the observer | `lost` | `lg` | This page lost its line to the observer |
| A route that does not exist | `off` | `lg` | This profile is not here, This pack is not here, or No pod runs this image |
| No findings | `calm` or `warn` | `md` | All quiet on the cluster, or No finding |
| No newer versions | `calm`, `warn`, or `build` | `md` | Nothing newer to dig up, No newer version is listed, or Reading the registries |
| Nothing in "Not checked" | `calm` or `warn` | `md` | Every layer was read, or Every pack was checked in full |
| The pods are not read | `off` | `md` | Pods are not on the menu here |
| The read of the pods failed | `warn` | `md` | The pods stay silent |
| Alerts are not set | `off` | `md` | Nobody gets paged yet |
| A side with no change | `calm` or `off` | `md` | Undisturbed |
| A pack that was not checked | `warn` | `md` | Part of this layer is hidden |
| A side that was not read | `warn` | `md` | This side was not read |
| A diff that a cycle holds | `build` | `sm` | One moment, a check is running |
| A diff that failed | `warn` | `sm` | The observer could not read these changes |
| A text above the cap | `off` | `sm` | Too much rock to carry |
| An empty tab, no event, and a filter with no match | `calm` or `off` | line | The sentence of the line |

A test of `internal/web` fails for each rule. It draws each place and checks the tone,
the headline, the fact, and the next step. It fails when a page with a finding, a failed
read, or a cause shows a calm tone. Another test runs a copy of the script whose gate always
says yes, and it shows that the first test then fails. A test fails when `app.css` leaves an
animation that `prefers-reduced-motion` does not stop, and a test fails when the mark differs
from the logo. A test also fails when an inline style reaches a screen or the policy of the
page changes.

### What the page keeps

- Four kinds of news, and no single color of health. A drift is red. A newer
  version stays neutral at each level. "Not checked" is gray, and it never
  raises a severity.
- "Not checked" blocks the text "No change". A pack that was not checked never
  reads "current".
- A side that strata could not read says "Not read". It never reads "Identical".
  The published side of a pack with an `error` row has no answer, and the route
  gives 404 for it.
- A move that strata could not compare has no counts. Its row shows a dash and
  "Not read", and "Not checked" names its pack.
- A detail request that fails is not kept. A 503 is a normal answer while a
  cycle holds the slot. The page then shows the splash "One moment, a check is
  running" with a button "Try now". Another failure shows an amber splash. The
  next render that a person or the clock causes asks again. A render that an
  answer causes does not, so two requests that fail do not ask each other again
  without end.
- An answer of Palette has the words of its verdict: "Palette gave no error",
  `Palette refuses: <code>`, and "Palette gave no answer". The page never reads
  "accepts", because Palette checks the default values of the new version and not
  the values of the team. A "Palette gave no answer" is no finding, and it raises
  no severity.
- A pod that never started and a local image with no repository digest read "Not
  checked". They never read "OK".
- `re-baseline needed` sits beside a drift and does not replace it.
- `same-content` reads "Same content" on the row of the cluster copy. It is not
  a drift.
- The page says nothing that it cannot know: not what a pod pulls next, and not
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
`first-seen`, `stale-unchecked`, `lost-unchecked`, `read-error`, `images`,
`images-split`, `images-off`, `images-unread`, `asks`, `text-too-large`, `alerts`, and `alerts-unset`. The server
listens on `127.0.0.1:8080`. The flag `-listen` takes another loopback address,
and `-data` takes another directory of sets.

A set is a directory of `observer/internal/web/testdata/api/`. It holds the answer
of each route as a JSON file. The dev server serves them through the real server
of `internal/api`, so the headers are those of the real server. It moves each
time of a set by the gap between now and the time of the data. So the ages on
the page are those of the data.

- The set `ok` holds made-up data. It has 12 profiles, 29 packs, 1 finding, and 10
  newer versions in 6 profiles. It also has 2 causes of "Not checked", 26 of 29 packs
  in full, and 8 images that pods run. One text is real: the two texts of
  `kubeadmconfig.files[2].content` in the pack `kubernetes`, from a run of
  `strata compare`. The file has 21 lines, and three lines differ.
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
- The sets `images`, `images-split`, `images-off`, and `images-unread` differ in
  `images.json`, the document `strata-observed/1`. `images` holds one moved tag, and
  `images-split` holds one tag with two digests. `images-off` has the state `off`.
  `images-unread` has the state `not-read`, after a refused read. See [The
  images](../reference/observer-api.md#the-images).
- The set `asks` names `ok` in a file `base`. Its `asks.json` holds one answer of
  each verdict on three moves: no error, a refusal, and a 403. The set
  `text-too-large` names `ok` and marks the text of two packs as too large. In the
  set `ok`, a click on a move gets its answer from `replies.json`, and a click on a
  move with no reply gets 404.
- The file `alerts.json` is the answer of `/v1/alerts`. A set can lack it: the page is then
  the page of an observer that started without `--alerts`. The set `alerts` names `ok`
  and holds seven rules, in the four states. A test gives the same rules to the real
  engine, with the findings and the newer versions of `ok`, and makes sure that each
  state is the one that the engine gives. The set `alerts-unset` names `ok` and holds
  the answer of an observer with no alert.
- Each tone of [the splash](#the-splash) has a set. The set `quiet` shows `calm`: it is
  the one set with a good check, with no finding, no cause, and no failed read. The set
  `first-load` shows `build`. The sets `no-records` and `images-off` show `off`. The sets
  `stale-unchecked` and `images-unread` show `warn`, and `lost-unchecked` shows `lost`.
- The page shows "This page lost its line to the observer" only when the first read of the
  routes fails, so no set shows it. A test of `internal/web` makes the routes fail.

A test decodes each file of each set into the type of its route, and it refuses a
field that the type lacks. So the test data cannot drift from the API. When `node` is on the
`PATH`, `go test` also renders each screen of each set with `app.js`. It fails on
a screen that throws, and on a value that reaches the page as markup. It skips
these tests when `node` is not there. It cannot see the layout, so a person looks
at the render.

## The click

The server takes one `POST`: the click "Ask Palette" on one move. The code of the
observer sends no HTTP `POST` to Palette. Palette gets its question from `strata`,
which `internal/runner` starts.

- **A method for each route.** A `Route` holds a pattern, a method, and a tier. The
  table of routes in `internal/api/api.go` gives each route one method. It is `GET`,
  except for the click. A path with no route takes `GET`. A test sends the other method
  to the click and to `/v1/asks`.
- **The guard.** It checks the `Host` first (`421`), then the method (`405` with
  `Allow`), and then, for a `POST`, the header `Sec-Fetch-Site` (`403`). The body of
  each refusal has a fixed text. The header stops a page of another origin in the
  browser of a person: such a page sends another value, or none. It stops no other
  client, so the gate stays `create` on `pods/portforward`.
- **The slot.** `Observer.Ask` takes the slot of the worker with `acquire`, as the
  detail route does. One `strata` process runs at a time, so the click waits for a
  cycle. The limit is `detailTimeout`, 50 seconds. A stop or the limit gives `503`, and
  another error gives `502`. A click that waited looks at the answers again, so two
  clicks on one move send one question.
- **The question.** `Ask` finds the pin of the move in the last good versions cycle.
  Without one, the move is not known, and the answer is `404`. Then `askPalette` writes
  the record of the profile to the directory of a cycle. It runs `behind <record>
  --pin <layer>/<name> --json --compact -q` once, with no `--no-impact`.
- **The runner.** It allows `--pin` for `behind` only, and `--full` for `compare` and
  `verify` only. It refuses a `behind` run with neither `--no-impact` nor `--pin`. So
  a cycle cannot ask Palette.
- **The answer.** `model.AnswerOf` reads the pin of the document.
  `bump.paletteValidation` gives `no-error` when its field `accepts` is true, and
  `refuses` with its `errors` otherwise. A pin with an `error`, a
  `paletteValidationError`, a document without the pin, or a run that failed gives
  `no-answer`. Only the status of the failure leaves:
  `docs.StatusOf` reads `HTTP <status>` at the start of the reason and nothing else.
  The reason can hold the body of an answer of Palette.
- **The memory.** `Observer.asks` maps a profile and a move to its `Answer`. The key of
  the move holds the version of the `strata` binary, as the cache of moves does.
  `Answered` is true for `no-error` and `refuses` only, and `Ask` returns such an answer
  with no question. A `no-answer` is stored too, so that `/v1/asks` shows its status,
  and the next click replaces it.
- **The prune.** `pruneMoves` drops each answer whose move the versions cycle no longer
  reports, so a pin that moves drops its answer. The state holds no answer, and a
  restart forgets all of them.
- **The log.** One line `ask` holds the verdict and the status. It holds no profile, no
  pack, and no message.

## The text of a diff

- **The cycles ask for it.** The content cycle passes `--full` to `verify`, and to the
  `compare` of each cluster side. The versions cycle passes it to the `compare` of a
  move.
- **It stays in memory.** `docs.Values.Unified` and `docs.Manifest.Unified` hold the
  text. No type of the summary tier has a field for it, so the summary, the state, and
  the log hold none. A test plants a marker in the text. It looks for the marker in
  each summary route, in `/v1/asks`, in the state, and in the log.
- **The cap.** `maxText` in `internal/check` is 256 KiB. The pod has a memory limit of
  512 Mi, and the cap keeps the text inside it. After each read of a diff,
  `Diff.CapText` adds up, for each pack, the bytes of `values.unified` and of each
  `manifests[].unified`. A pack above the cap loses all of its text, and
  `Pack.TextTooLarge` is true. A pack at the cap keeps its text.
- **`TextTooLarge` is a field of the observer.** It has the tag `json:"-"`. A document
  of `strata` never sets it, and the observer never writes it to a document. The view
  of the API copies it to `textTooLarge`.

## What no answer carries

[The API of the observer](../reference/observer-api.md#what-no-answer-carries) lists the fields that no
answer carries. The allow-list of `internal/docs` has no field for them. The bytes
can reach memory, but they reach no type of the observer. So no answer and no log
carries them.

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
- **Four headers on each answer,** also on `403`, `404`, `405`, and `421`:

  ```text
  Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
  X-Content-Type-Options: nosniff
  Referrer-Policy: no-referrer
  Cache-Control: no-store
  ```

- **Four timeouts.** The server sets `ReadHeaderTimeout`, `ReadTimeout`,
  `WriteTimeout`, and `IdleTimeout`.
- **Three HTTP clients.** The `health` check in `internal/api` calls a loopback
  address. The read of the pods in `internal/kube` calls the API server. The alerts
  in `internal/alert` send a message to a webhook. The code sends no HTTP `POST` to
  Palette. Only `internal/api`, `internal/kube`, and `internal/alert/send.go` import
  `net` or `net/http`. A test makes sure of each rule.
- **Two files name the method `POST`.** The server takes one `POST`, and
  `internal/api/api.go` names the method for that route. `internal/alert/send.go`
  names it for the message to a webhook. No other file of the observer holds the
  method. A test makes sure of it.
