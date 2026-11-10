# Write the code

These rules apply to each change to the Rust code. A reviewer refuses a change
that breaks one. [Build and test](./build-test.md#where-the-code-goes) tells
where each module goes. The Go code of the observer follows
[its own rules](#the-go-of-the-observer).

## The layers

- `main.rs` and `cli/` parse the arguments, print, and give the exit code. They
  hold no rule and build no error kind for a decision of a command. Put that
  decision in `ops/`.
- `ops/` holds the work of each command. It is generic over `A: PaletteApi`,
  and it takes the connection as `connect: impl FnOnce() -> Result<Palette<A>>`,
  so that a ref to a file never connects.
- `api/` is the only code that talks to Palette. A new request is a method of
  `Palette<A>` in `api/palette/`. Escape each path segment with
  `query_value`. A pure change of a document, such as a mask, goes in its own
  module of `api/` and has unit tests. The methods of `Palette` fetch, and do
  not rewrite.
- `render/` prints through the `Renderer` trait. A line that two renderers show
  is one function in a shared module of `render/`: the module of its command,
  such as `render/verify.rs`, or `render/mod.rs`. Do not write that line again in
  `cli/`.
- A type that two modules share goes in a module of its own. It does not go in
  the module of one of its users. Re-export it where the public path must not
  change.
- The kind of the shared module says where it goes. A type of Palette goes in
  `model/`. A read of Palette goes in `api/`. A helper of the whole crate, such
  as `json` or `text`, stays at the crate root. `tests/crate_root.rs` lists the
  modules of the root. A new module of the root needs a line there, with its kind.

## The size of a module

- A module does one thing. When a module file has more than about 800 lines,
  divide it: `mod.rs` and one file for each part. A file that
  holds only tests can be longer.
- Move a file with `git mv`, so that `git log --follow` keeps its history. Move
  the file whole in one pull request, and divide it in the next. The squash of
  one pull request that does both loses the history.
- To divide a file in the same pull request, leave it as `name.rs` and put the
  new files in `name/`, such as `packs.rs` and `packs/tests.rs`. The file keeps its
  path and its history. A later pull request can move it to `name/mod.rs` whole.
- Put the unit tests of a module in `tests.rs` in its directory when they make the
  module file too long.
- When a public module moves under a new parent, keep a `pub use` at the old
  path for one minor release.
- `cargo test -p xtask` checks the limit of 800 lines for each module file of
  `src/`, `xtask/src/`, `crates/strata-preflight/src/`, `fuzz/src/`, `crates/strata-report-endpoint/src/`, and
  `benches/src/`. It checks the limit of 80 lines for each function of the six.
- The check parses each file, so it counts a function with any head, such as `async fn`.
  Code under `#[cfg(test)]` is out of the count, and the code after it is in. A file named
  `tests.rs` holds only tests, so it has no limit.

## Visibility

- `pub` is the library API. Give it only to what `examples/library`,
  `examples/library-preflight`, the tests, or another crate use.
- Use `pub(crate)` or `pub(super)` for each other item. Do not re-export a
  helper from `mod.rs` to make a call shorter.
- One entry point for one operation. A flag is an argument or a field of an
  options struct, not a second public function.

## Types and data

- Make a value that can be wrong impossible to write. Use an enum for a closed
  set of values, not a `String` or a `&'static str`. Use a struct with named
  fields, not a tuple of four values.
- Give each closed set its `ALL` and its `as_str` with `impl_closed_set!`, from one table.
  Do not write a list of the values or a `match` of their texts by hand. The test
  `tests/closed_set.rs` makes sure that each text is the one that serde writes.
  `crates/strata-preflight/` uses the same macro, `strata::impl_closed_set`.
  `crates/strata-preflight/tests/closed_set.rs` makes the same check there, with the same checks from
  `tests/common/closed_set.rs`.
- An enum that no document holds, such as a segment of a route or a layer, has no serde. Give
  it `ALL`, `as_str`, and `from_text` with `impl_texts!`, from one table. Do not write a
  `match` of its texts by hand, once to read them and once to write them.
- Do not store a field that comes from another field. Calculate it with a
  method.
- Palette documents stay `serde_json::Value`. Read them with the helpers of
  `src/json.rs`. They are `text`, `str_at`, `text_or`, `opt_text`, `opt_str`,
  `arr`, `strings`, `text_map`, `items`, `take_items`, `first_nonempty`, `flag`,
  `opt_flag`, `opt_set`, `object_at`, `value_at`, `value_or_null`, and
  `value_or_empty_object`.
- Do not write a pointer read of text, of an array, of an object, of a flag, or
  of a value in a module. The rule holds for `crates/strata-preflight/` and `crates/strata-knurl/` too.
  `text`, `str_at`, `text_or`, `opt_text`, `opt_str`, `arr`, `strings`,
  `text_map`, `flag`, `opt_flag`, `value_at`, `value_or_null`, and
  `value_or_empty_object` are public for them. `set_at` is public too, for the
  write below. Make another helper public when a package outside this crate
  needs it, not before.
- Write a field with `set_at`. The index of `serde_json` panics for a value that
  is not an object. `set_at` makes an object of that value. When the value must
  stay, check it first. An entity whose `metadata` is a number is an error, and
  the name must not replace it. A `json!` object that the function builds itself
  is an object, so it can take the index.
- The records of strata are typed structs. Give each one
  `#[serde(rename_all = "camelCase")]`, `skip_serializing_if` for an empty
  field, `#[serde(default)]` for a field that an older record does not have,
  and `JsonSchema`.
- Do not put a field of strata into a document of Palette to carry it to
  another function. Return a struct that holds both.
- A version order follows Semantic Versioning 2.0.0 where the version parses, as
  Palette orders versions. `src/reference/version.rs` is the one module that
  parses a version and orders versions. Use `version::compare`, `version::pick`,
  `version::numbers`, and `version::is_pre_release`. `xtask/` and
  `strata-preflight` use them too. Do not write a second parse.
- A string that does not parse reads as its first three numbers, as Palette
  reads it. `version::has_three_numbers` finds them. A string with no three
  numbers, such as `LTS__24.x`, is below each version. The natural order of the
  text orders only such strings. No script uses `sort -V` for a version. A
  script reads the direction of a move from strata, such as `versionMove` in
  `strata-diff/1`.
- One order of a version lives outside strata: `oldest_first` in
  `examples/fetch-examples.sh`. The script runs offline, so it cannot ask
  strata. It keeps only a version of plain numbers and orders it by the
  numbers. The test `the_examples_fetch_orders_plain_versions_as_strata_does`
  fails when this order differs from `version::compare`. Add no other
  exception.
- Give each limit, code, and fixed text of Palette a named `const`, in one
  place. Its comment says what Palette does. It does not cite the code of
  Palette: see [rule 7](./documentation.md#rules).

## Errors

- Use one error type, `crate::error::Error`, with `Result<T>`. A new variant
  has a `kind()`, a `to_json()`, a `class()`, and a sample in the tests of
  `error/`.
- Do not use `unwrap`, `expect`, or a slice index that can panic on input from
  a user or from Palette. Slice text at a `char` boundary only.
- Do not discard an error. `unwrap_or_default()`, `.ok()`, and `let _ =` are
  correct only when the empty value is the correct answer. A comment says why.
  An error must never become an empty value that strata records or hashes.
- A wrong use of the flags or the refs is `Error::Usage`. When the refs and the
  flags show it, `ops/` refuses it before the first request. Strata reads
  a record file first, because a read of a file is no request.
- A fault that only an answer shows is `Usage` too, when the person must change
  the command. Two refs that name two clusters are such a fault.
- A refusal that a record or an answer of Palette causes, and not the command,
  has an error kind of its own, such as `StaleRecord`. Do not give it `Usage`.
  Add the kind to `errors.md` and to `compatibility.md`.
- Check what a ref or a record shows before `connect`. A record that strata
  cannot use then gives its own error, and not `missing-credential`.

## Ownership

- Borrow. Take `&str`, `&[T]`, and `&Value`, not `String`, `Vec<T>`, or
  `&&Value`. Do not clone a document to read it.
- Use the `entry` API, not `contains_key` and then `insert`.
- Use a `BTreeSet` or a `BTreeMap` for a test of membership in a loop. The
  `BTree` types keep the output order stable.
- strata runs on one thread. Do not add a thread, a lock, or `async` without a
  measured need.

## Functions and repetition

- A function does one thing. Divide a function that has more than about 80
  lines, or has branches that build the same document. Write one function for
  each branch and one function for the shared part.
- Write a rule once. When two commands parse the same answer, or two functions
  sort and filter the same list, make one function and call it from both.
- Do not write a closure twice in one function under one name.

## Comments

The comments follow the conventions of the Rust standard library, in
ASD-STE100.

- The first paragraph of a doc comment says what the item does. Its first
  sentence is the summary. Rustdoc shows the paragraph in the index of the
  module, so it has 200 characters or fewer: `too_long_first_doc_paragraph`
  fails on a longer one. Put the details in the next paragraph.
- Name each argument whose role its name and type do not show. Write it in the
  prose and in backticks: "`fetched_at` is the time that the record states, in
  RFC 3339". Do not write a list of each argument.
- A public function that returns `Result` has a `# Errors` section: the error
  kinds that it gives, and when.
- A function that can panic has a `# Panics` section. A panic on input from a
  user or from Palette is a bug: see [Errors](#errors).
- A main entry point of the library has an `# Examples` section. The example
  runs as a test. `ops::snapshot_for`, `ops::compare`, `ops::verify`,
  `ops::snapshot_from_candidate`, and `candidate::resolve` have one.
- `Cargo.toml` of `strata` and of `strata-preflight` turns on four lints of
  clippy: `missing_errors_doc`, `missing_panics_doc`,
  `too_long_first_doc_paragraph`, and `doc_markdown`. They enforce the four
  rules above, so `just lint` fails on a comment that breaks one. Do not add an
  `allow` for them. Put a code item in backticks. Put a name that is no code
  item, such as `OpenTofu`, in `doc-valid-idents` of `clippy.toml`.
- Link each type, function, and module that a comment names, as
  `` [`Snapshot`] ``. `just lint` refuses a broken link, also in the comment of
  a private item.
- Write a code span on one line. Rustdoc reads a code span that breaks over a
  line as two halves. Then a placeholder such as `<status>` is an HTML tag, and
  `just lint` fails.
- An inline comment says why, not what. Write one above an algorithm, with its
  steps or the rule that it keeps. Write one at a constant, a fact of Palette,
  or a workaround. Do not write a comment that says again what the code says.
- Do not keep code in a comment. Git keeps the history.

## Tests

- Each bug fix has a test that fails without the fix.
- Each panic that input can cause has a case in `tests/fuzz.rs` or a unit test.
  A panic that a fuzz target finds also keeps its input in the `crashes`
  directory of that target: see [The fuzzing](./build-test.md#the-fuzzing).
- Each parser of text from a user or from Palette has a fuzz target in `fuzz/`.
- Put a helper that two test files use in `tests/common/mod.rs`. Compare
  against the `const` of the crate, not against a copy of its text. A test that
  pins the text of a schema id on purpose can stay, with a comment that says so.
- A test of another package uses a helper of `tests/common/` with `#[path]`. It does not copy the
  helper. Put such a helper in a file of its own, not in `mod.rs`, which only the root package
  uses. `palette.rs`, `closed_set.rs`, `walk.rs`, and `toml.rs` have such users. A file that uses
  some of the helpers allows `dead_code` on the include.
- A test serves the route that Palette serves, from a helper of `tests/common/palette.rs`. It
  does not build the route with a function of `strata::api`. `tests/routes.rs` makes sure that
  the two give the same route.
- A test that runs a script in a tree of its own copies the script with
  `copy_scripts`. It copies each file that the script sources too, such as
  `examples/live/common.sh`.
- The name of a test is a full sentence that states the behavior.
- Start the binary of strata with `strata_command`, and give it to a script with `give_strata`.
  Both stop the send of the report: see
  [The tests send no report](./build-test.md#the-tests-send-no-report).
- Start git with `git` or `isolate_git`, which give git its settings: see
  [The tests get no git setting of the machine](./build-test.md#the-tests-get-no-git-setting-of-the-machine).
- Start a process with `output_locked` or `spawn_locked` of `CommandExt`, never
  with `output`, `spawn`, or `status` of `Command`. To wait for the end of a
  process, call `wait` on the result of `spawn_locked`. Write a script that a test
  runs with `executable`. The tests run on many threads, and the two functions
  share a lock. Without it, a process can start during the write and keep the
  script open. The run of the script then fails with "Text file busy".
- `CommandExt` and `executable` are in `tests/common/process.rs`. The tests of
  `strata-preflight` and of `xtask` include that file with `#[path]`, so one lock
  covers the processes of the three packages. The runner of the book in `xtask`
  takes the same lock. `tests/spawn.rs` scans `tests/`, `crates/strata-preflight/tests/`, and
  `xtask/src/`, and makes sure of the rule. The other packages start no process, so
  they need no lock.
- Make a temporary directory with `tempfile::tempdir()`, never by hand from
  `std::env::temp_dir()`. The directory goes when the test ends, also when the
  test fails.

## The Go of the observer

The observer is the Go program in `observer/`. It runs the `strata` binary and
shows the answers, and it holds no rule of strata. With `--read-pods`, it also lists
the pods of its cluster. With `--alerts`, it sends a message to a webhook. On a
click, it asks Palette about one move, through `strata`. The rules above are for
the Rust code, and these rules are for the Go code.

- The observer uses the standard library only. `go.mod` has no `require` line,
  and `just lint` fails when it has one.
- The code has no cgo, no `unsafe`, no `init` function, and no `panic`. A
  function that fails returns an `error`. The tests keep the rule: a fake
  returns an `error`, and a test calls `t.Fatal`.
- `internal/runner` is the one place that starts a process. It refuses a
  command or a flag that is not in its lists. A process gets `STRATA_REPORT=0`,
  which stops the send of the report, and each variable of its list that the
  observer has. The list is `PALETTE_API`, `PALETTE_API_KEY`,
  `PALETTE_PROJECT_UID`, `PALETTE_PROJECT`, `HOME`, `TMPDIR`, `XDG_STATE_HOME`,
  and `PATH`. A process gets no other variable, and a test holds the list.
  `STRATA_QUIET` and `NO_COLOR` are not in the list, because each call has `-q`
  and no call writes a terminal report.
- Some flags have a command of their own in `internal/runner`. `--full` is for
  `compare` and `verify`, and `--pin` is for `behind`. The runner refuses a
  `behind` run that has neither `--no-impact` nor `--pin`. It refuses `--validate`
  and `--repave` of `compare`. So no cycle can send a `POST` to Palette.
- An error of a run holds a kind and a status. It never holds a message,
  because the message of the kind `api` can hold the body of an answer from
  Palette.
- `internal/docs` is the allow-list of the fields that the observer reads. A
  document never decodes into a map, and no code forwards a document. A test
  makes sure that code reads each field. The detail tier sends a few fields as
  they are, such as `Leaf.Old` and the fields of `Refusal`. The list `forwarded`
  of that test names them.
- `internal/check` reads a document of strata only with the schema id that
  `internal/docs` names for its command. Another id, or none, is a failed read
  of the kind `schema`, and it makes no finding. A test makes sure of it for
  `verify`, `compare`, `behind`, and the snapshot of the cluster.
- A test of `internal/docs` writes a marker at the path of a field that is not
  in the list. It fails when the marker is in the output.
  `just observer-contract` fails when a field of the list is not in the schema
  of the binary.
- A log line holds a verdict, an exit code, a duration, an error kind, and a
  status. One more line names a profile and the status of its `verify.whoError`:
  `HTTP <status>`, or `no answer`. The line for a click holds a verdict and a
  status, and it names no profile and no pack. The line for an alert holds the name
  of the channel, a verdict, an error kind, and a status, and no address and no
  text. A log line holds no output of a process. The rule has no exception.
- `internal/baseline` reads the records, and decodes each one only into
  `docs.Record`. `internal/model` holds the summary and the rules that build it.
  `internal/check` runs the cycles through its `Strata` interface, so a test
  gives it a fake that returns files from `testdata/`. See
  [The checks of the observer](./observer.md).
- The summary and the state hold no old value, no new value, and no changed path.
  They hold no value of a variable, no text of a diff, and no answer of Palette. A
  path can hold the name of a list item, and that name is a value of the cluster. A test
  plants a marker in a changed value, in a changed path, and in a variable. It
  fails when the marker is in the summary, the state, or the log. Other tests plant
  a marker in the text of a diff and in a refusal of Palette, and look in the same
  places.
- The state of the alerts holds the open subjects and the messages that did not go.
  They hold names, times, and fixed words only.
- A cycle makes its directory under `--work` and removes it at the end, also
  after an error. A test makes sure that `--work` is empty after a good cycle
  and after a failed cycle.
- The one compare of versions in the Go code is the major and minor number of
  `tool.version`, in `model.ReBaselineNeeded`. The observer does not order pack
  versions: `behind` gives the order in `newer[0]`.
- `internal/api` serves the summary and the detail tiers: see
  [The server of the observer](./observer-api.md). The summary tier uses types of
  its own, and none has a field of type `docs.Leaf`. Only `PackDiff` of the
  detail tier has one. A test walks the types. The detail tier also carries the
  text of a diff in `PackDiff`, and the answers of Palette in `AsksView`.
- `internal/api`, `internal/kube`, and `internal/alert` can import `net` and
  `net/http`. `internal/api/health.go`, `internal/kube/kube.go`, and
  `internal/alert/send.go` can hold the client and the request. The code of the
  observer sends no HTTP `POST` to Palette. The server takes one `POST`, the click
  "Ask Palette", and only `internal/api/api.go` names the method. The one other
  `POST` is the message to a webhook of your team, and only `internal/alert/send.go`
  names the method for it. A test makes sure of each rule.
- `internal/alert` sends a message when a finding opens and when it clears. It
  reads the summary tier only: `subject.go` is the one file that reads the types of
  `internal/model`, from `alert.SubjectsOf`. The other files name `internal/model`
  only for the kind of a finding and for `ShortDuration`. The package imports no
  other package of the observer. So a message holds a profile, its packs, a fixed
  sentence, and a time, and no value. A test makes sure of the imports.
- A test plants a marker in a changed value, in a changed path, in a variable, and
  in the audit log. It fails when the marker is in a message, the state, the log,
  or a route.
- The rules of the alerts are a file, and no route of the observer writes it. A
  file that does not parse stops the start, with its line. A mute needs an end
  date. The rule `newer` with `send: now` is refused, because a newer version never
  pages a person. A lower level overrides a higher one. In one level, a live mute
  wins over a rule that sends, and a mute can stand beside it.
- The address of a webhook is in a file that the observer reads again at each
  send. It must use `https`, and the observer follows no redirect. The flag
  `--alerts-allow-http` is for the tests only. A failed send keeps the message in
  the state, and the next content cycle sends it again. A cycle that failed or
  could not read a part clears no alert. A record file that cannot be read counts as
  a part that was not read.
- The click is the one question that the observer asks Palette. `internal/check`
  runs `behind` with `--pin` for one pack, and only when `internal/api` calls it for a
  click. Nothing calls it on a timer. The answer stays in memory. A request that
  failed is the verdict `no-answer`, and the next click asks again.
- A pack keeps 256 KiB of text at most for each side of a diff. `maxText` in
  `internal/check` holds the limit, and `Diff.CapText` drops all the text of a pack
  above it. The pod has a memory limit, and the cap keeps the text inside it. A test
  makes sure that a pack at the limit keeps its text.
- `internal/kube` is the one place that calls the Kubernetes API. It sends one `GET`
  on the list of pods, with paging, and no other request. It decodes no `env`. It reads
  the token and the CA again at each call, because the kubelet rotates the token. Its
  error holds a kind and a status, and never a message.
- The observer makes no Kubernetes call without `--read-pods`. The state keeps the
  first digest of an image under an id, and never the reference, a pod name, or a
  namespace. A test plants a marker in the `env` of a pod. It fails when the marker
  is in an answer, the state, or the log.
- `internal/web` carries the files of the page with `embed`, and `internal/api`
  serves them. A test fails when a file of `assets/` has an external
  address, an inline style, an inline script, or a second write of markup. It also
  fails on a `max-width` on `.page`. Other tests of the package fail on a copy of the
  logo that differs from `assets/logo/`. They also fail on a `prefers-color-scheme`
  rule in `app.css`, and on a drift that is not white on `#c9252f`. See [The page](./observer-api.md#the-page).
- A value from the data reaches a copied command only through `shq()` in `app.js`.
  A test of `internal/web` gives the page names that a shell reads as code. It runs
  each command with `bash` and a stand-in for `strata`. It fails when a name splits
  or writes a file. Another test fails when a builder of a command has no `shq()`.
- `internal/web/devdata` and `internal/web/devserver` serve test data for the page,
  with no `strata` binary. A test decodes each file of each set into the type of
  its route, so the data cannot drift from the API.
- A server answers on a loopback address only. It sets the four headers on each
  answer, it refuses a `Host` that is not loopback, and it logs no request. It
  refuses a `POST` that has no header `Sec-Fetch-Site: same-origin`.
- One worker holds the one slot for a `strata` process. A cycle, a request of the
  detail tier, and a click take the slot in turn, so two processes never run together.
- A test of a route has a line in the table of the sentinel test. The test fails
  when a route has none.
- A test uses a table when it has more than two cases. A test that needs the
  binary skips when `STRATA_BIN` is not set.
- The state that a wait of a test reads stays until the next step reads it. A
  count of the calls stays. The progress of a cycle stays only while the cycle
  runs, and a short cycle can end between two reads. So a test that acts while a
  cycle runs holds the cycle at a gate, such as `gatedStrata` of `internal/check`.
  A fixed sleep reads no state, so it is no wait. A test that must see two
  processes run together holds the first one with `testkit.Overlap`.
- A wait of a test that has a limit of time, also a wait on a channel, uses
  `testkit.WaitLimit`.
- A wait of a test, with `testkit.WaitFor`, reads the state that its next step
  reads. The content cycle and the versions cycle end in either order. So the end
  of one cycle does not show the end of the other. The state of the page reads the
  content cycle only.
- A test starts a server on `testkit.PortZero`, so the kernel gives it a free port.
  `testkit.StartServer` reads that port from the line `listening` of the log. A test
  never gives a server a port that it closed, and never asks a server that it
  closed. Another process can bind that port first, and answer in place of the
  server. A test of "no server" asks `testkit.PortZero`, because no server can
  listen on port 0. Two tests make sure that no test calls `net.Listen`, and that
  no test uses a server after it closes it.
- No local name hides a package that its file imports, such as a variable `api` in
  a file that imports `internal/api`. A test makes sure of it.
- `internal/testkit` holds each helper that the tests of two packages use, such
  as the script that stands for `strata`. Only test files import it. A helper
  that one package uses stays in that package.

`just lint` runs `gofmt` and `go vet` on `observer/`, and `just test` runs
`go test` there.

## Before the review

```bash
just lint && just test     # fmt, clippy -D warnings, rustdoc -D warnings of all items, gofmt and go vet of the observer, and the tests
```

Then read the diff against each section of this page, and against
[The style rules](./documentation.md#the-style-rules) for each comment and each
message.
