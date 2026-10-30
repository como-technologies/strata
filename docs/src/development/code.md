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

## The size of a module

- A module does one thing. When a module file has more than about 800 lines,
  divide it: `mod.rs` and one file for each part. A file that
  holds only tests can be longer.
- Move a file with `git mv`, so that `git log --follow` keeps its history. Move
  the file whole in one pull request, and divide it in the next. The squash of
  one pull request that does both loses the history.
- When a public module moves under a new parent, keep a `pub use` at the old
  path for one minor release.

## Visibility

- `pub` is the library API. Give it only to what `examples/library`, the tests,
  or another crate use.
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
- Do not store a field that comes from another field. Calculate it with a
  method.
- Palette documents stay `serde_json::Value`, read with the helpers of
  `src/json.rs`: `text`, `str_at`, `text_or`, `opt_text`, `opt_str`, `arr`,
  `strings`, `text_map`, `items`, `take_items`, `first_nonempty`, `flag`,
  `opt_flag`, `value_at`, and `value_or_null`. Do not write a pointer read of
  text, of an array, of a flag, or of a value of your own in a module. The rule
  holds for `preflight/` and `adapter/` too. `text`, `str_at`, `text_or`,
  `opt_text`, `opt_str`, `arr`, `strings`, `text_map`, `flag`, `opt_flag`,
  `value_at`, and `value_or_null` are public for them. Make another helper public when a
  package outside this crate needs it, not before.
  The records of strata are typed structs with
  `#[serde(rename_all = "camelCase")]`, `skip_serializing_if` for an empty
  field, `#[serde(default)]` for a field that an older record does not have,
  and `JsonSchema`.
- Do not put a field of strata into a document of Palette to carry it to
  another function. Return a struct that holds both.
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
- A wrong use of the flags is `Error::Usage`, before any request.

## Ownership

- Borrow. Take `&str`, `&[T]`, and `&Value`, not `String`, `Vec<T>`, or
  `&&Value`. Do not clone a document to read it.
- Use the `entry` API, not `contains_key` and then `insert`.
- Use a `BTreeSet` or a `BTreeMap` for a test of membership in a loop. The
  `BTree` types keep the output order stable.
- strata runs on one thread. Do not add a thread, a lock, or `async` without a
  measured need.

## Functions and repetition

- A function does one thing. When a function has more than about 80 lines, or
  has branches that build the same document, divide it into one function for
  each branch and one function for the shared part.
- Write a rule once. When two commands parse the same answer, or two functions
  sort and filter the same list, make one function and call it from both.
- Do not write a closure twice in one function under one name.

## Comments

The comments follow the conventions of the Rust standard library, in
ASD-STE100.

- The first line of a doc comment is one sentence that says what the item does.
  Rustdoc shows it in the index of the module. Put the details in the next
  paragraph.
- Name each argument whose role its name and type do not show, in the prose and
  in backticks: "`fetched_at` is the time that the record states, in RFC 3339".
  Do not write a list of each argument.
- A public function that returns `Result` has a `# Errors` section: the error
  kinds that it gives, and when.
- A function that can panic has a `# Panics` section. A panic on input from a
  user or from Palette is a bug: see [Errors](#errors).
- A main entry point of the library has an `# Examples` section. The example
  runs as a test.
- Link each type, function, and module that a comment names, as
  `` [`Snapshot`] ``. `just lint` refuses a broken link.
- An inline comment says why, not what. Write one above an algorithm, with its
  steps or the rule that it keeps. Write one at a constant, a fact of Palette,
  or a workaround. Do not write a comment that says again what the code says.
- Do not keep code in a comment. Git keeps the history.

## Tests

- Each bug fix has a test that fails without the fix.
- Each panic that input can cause has a case in `tests/fuzz.rs` or a unit test.
- Put a helper that two test files use in `tests/common/mod.rs`. Compare
  against the `const` of the crate, not against a copy of its text.
- The name of a test is a full sentence that states the behavior.

## Before the review

```bash
just lint && just test     # fmt, clippy -D warnings, rustdoc -D warnings, and the tests
```

Then read the diff against each section of this page, and against
[The style rules](./documentation.md#the-style-rules) for each comment and each
message.
