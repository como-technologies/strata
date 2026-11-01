# Write the code

These rules apply to each change to the Rust code. A reviewer refuses a change
that breaks one. [Build and test](./build-test.md#where-the-code-goes) tells
where each module goes.

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
  `src/`, `xtask`, `preflight/src/`, and `fuzz/src/`. It checks the limit of 80
  lines for each function of the four, with the unit tests out of the count. A
  file named `tests.rs` holds only tests, so it has no limit.

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
  `preflight/` uses the same macro, `strata::impl_closed_set`.
  `preflight/tests/closed_set.rs` makes the same check there.
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
  of a value in a module. The rule holds for `preflight/` and `adapter/` too.
  `text`, `str_at`, `text_or`, `opt_text`, `opt_str`, `arr`, `strings`,
  `text_map`, `flag`, `opt_flag`, `value_at`, `value_or_null`, and
  `value_or_empty_object` are public for them. `set_at` is public too, for the
  write below. Make another helper public when a package outside this crate
  needs it, not before.
- Write a field with `set_at`. The index of `serde_json` panics for a value that
  is not an object. `set_at` makes an object of that value. When the value must
  stay, check it first. An entity whose `metadata` is a number is an error, and
  the name must not replace it.
- The records of strata are typed structs. Give each one
  `#[serde(rename_all = "camelCase")]`, `skip_serializing_if` for an empty
  field, `#[serde(default)]` for a field that an older record does not have,
  and `JsonSchema`.
- Do not put a field of strata into a document of Palette to carry it to
  another function. Return a struct that holds both.
- A version order follows Semantic Versioning 2.0.0 where the version parses, as
  Palette orders versions. `src/reference/version.rs` is the one module that
  parses a version and orders versions. Use `version::compare`, `version::pick`,
  and `version::is_pre_release`. Do not write a second parse.
- The natural order of the text is only the fallback for a string that does not
  parse, such as `LTS__24.x`. No script uses `sort -V` for a version. A script
  reads the direction of a move from strata, such as `versionMove` in
  `strata-diff/1`.
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
  flags show it, `ops/` refuses it before the first request. A record file is
  read first, because a read of a file is no request.
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
  against the `const` of the crate, not against a copy of its text.
- The name of a test is a full sentence that states the behavior.
- Start a process with `output_locked` or `spawn_locked` of `CommandExt`, never
  with `output`, `spawn`, or `status` of `Command`. Write a script that a test
  runs with `executable`. The tests run on many threads, and the two functions
  share a lock. Without it, a process can start during the write and keep the
  script open. The run of the script then fails with "Text file busy".
  `tests/spawn.rs` makes sure of the rule.

## Before the review

```bash
just lint && just test     # fmt, clippy -D warnings, rustdoc -D warnings of all items, and the tests
```

Then read the diff against each section of this page, and against
[The style rules](./documentation.md#the-style-rules) for each comment and each
message.
