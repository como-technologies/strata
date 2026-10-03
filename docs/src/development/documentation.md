# Write the documentation

The book is the documentation. `README.md` is the entry point, not a second copy.

```bash
just docs-serve        # build the book and open it at http://localhost:3000
just docs-examples     # run each command of the book again, and write docs/src/output/
just docs-check        # fail when docs/src/output/ is not current
just docs-lint         # test each link with mdbook-lint
```

The book needs `mdbook`, `mdbook-mermaid`, and `mdbook-gruvbox`. `just
docs-lint` needs `mdbook-lint`:

```bash
cargo install mdbook mdbook-mermaid mdbook-gruvbox mdbook-lint
```

## Rules

1. **Keep the README short.** It says what strata does, gives a quick start and
   the requirements, and links to the book. A deep dive goes in the book. Add at
   most one link line to the README for it.
2. **Write ASD-STE100 Simplified Technical English.** This applies to the book,
   the README, the help text of the commands, the code comments, and the commit
   messages. See [The style rules](#the-style-rules).
3. **Include the output. Do not copy it.** Each output of strata in the book is
   a file in `docs/src/output/`, which `just docs-examples` writes. Never paste
   an output of strata into a page.
4. **Do not show internal names to a user.** The root is a digest. Only
   [The hashes and the root](../internals/hashes.md) names the tree behind it.
5. **Put each fact in one place.** Link to it from other pages.
6. **State a live result as a fact** in
   [What Palette does](../internals/palette.md), and add the check that
   establishes it to
   [What the live example verifies](./live-runs.md). Neither page tells the
   reader which day a run happened on.
7. **Do not cite the code of Palette.** A page or a code comment states what
   Palette does. It does not give a path, a line number, or a function name
   from the code of Spectro Cloud. That code changes, and the reference becomes
   wrong. Give the error code that Palette returns, or link the live run that
   shows the behavior.

## Where a page goes

| Content | Section |
| --- | --- |
| Install, configure, the first commands | Getting started |
| What a command does, its output, what to expect, its options | Commands: one page for each command |
| A problem, the commands that find it, and the fix | Use cases: one page for each problem |
| Scripts, CI, and the library | Automation |
| Tables and field lists | Reference |
| How strata calculates, and what Palette does | How it works |
| How to build, test, release, and write | Development |

## A command page

A command page ends with the `--help` of its command, from
`docs/src/output/help-<command>.txt`, so its options are those of the binary.
Its text also names each option of that help, with what the option does or a
link to the page that says. `just docs-check` fails on a command page that
leaves the help out, or that has no sentence for an option.

A long command page splits by what the command reads. The command page keeps
what the command does, a table of its pages, what to expect, and its options,
with a table that names each option and links the page that shows it. One page
in `docs/src/commands/<command>/` holds each kind of subject or question, with
its output and its options, and `SUMMARY.md` lists it under the command. A
page in that directory is not a command page: `just docs-check` reads the help
of the command page alone. `snapshot`, `compare`, and `preflight` are split.

## Serve a chapter, to try a command by hand

```bash
just playground          # the tenant of the chapter "list"
just playground compare  # another chapter
```

The runner builds the tenant of that chapter, serves it, prints the settings to
export, and waits. It runs no step and writes no output file, so a page can be
tried by hand before its output is generated. It is also how a reader follows
the book without a tenant.

A chapter that changes its own tenant calls `serve_here` at that point. The
playground then serves the tenant that the steps before the change describe. The steps after it read the change in the page, because the
playground does not make it. Put `serve_here` before an `edit`, a second publish
under one name, or any other line that a step of the page is written to cause.

The playground takes each chapter that `r.chapter(...)` starts in
`xtask/src/chapters.rs`. No second list of the chapters exists, so a new chapter
needs no other change. Two chapters with one name stop the runner.
`just playground CHAPTER-N` serves state `N` of the chapter. A chapter has one
state more than its calls of `serve_here`. `just docs-check` fails on a page
that gives `just playground` with a chapter that the runner does not start. It
also fails on a state that the chapter does not have.

## Add an output

`xtask` runs each command against a local stand-in for the Palette API, which is
`xtask/src/palette.rs`. The stand-in serves JSON files, one for each
request. Each chapter of the runner builds the tenant that one page needs, from
the real pack data in `examples/`, and changes it between commands as Palette
changes it.

1. Find the chapter of the page in `xtask/src/chapters.rs`, or add one with
   `r.chapter("name", env)`.
2. Build the tenant with `projects`, `profile`, `profile_own_copy`, `addons`,
   `cluster`, `cluster_with`, and `registries`. Change it with `edit`,
   `on_cluster_reclaim`, and `set_value`.
3. Add a step with its name, the expected exit code, whether to keep standard
   error, and the command:

   ```rust
   r.step("verify-drift", 1, false, "strata verify releases/1.1.1/dev.json");
   r.step("snapshot-profile", 0, true, "strata snapshot aws-stack@1.1.1 -o r.json");
   ```

   Two steps with one name go into one file. `r.excerpt(name, from, pattern)`
   writes the lines of an output that a page teaches.
4. Run `just docs-examples`, and include the file:

   ````markdown
   ```text
   \{{#include ../output/verify-drift.txt}}
   ```
   ````

   `\{{#include ../output/file.txt:1:20}}` includes lines 1 to 20.

A step with another exit code stops the runner. The runner replaces each time
stamp with `2026-09-17T09:30:00Z`, so a file changes only when strata changes.
A change to Palette behavior in the stand-in needs a live run that shows it.

## The style rules

Test each sentence: does it tell an expert something that the command, the
heading, and their own knowledge do not? If not, delete it.

- Write short sentences: 20 words or fewer in a procedure, 25 in a description.
- Put six sentences or fewer in a paragraph, and three words or fewer in a noun
  cluster.
- Use the active voice and the present tense.
- Write one instruction in each sentence. Start a condition with "if" or "when".
- Use "can" and "must". Do not use "may", "might", "should", or "would". Use
  "could" only as the past tense of "can".
- Use "make sure", not "verify" or "ensure", in prose. A command name is a
  technical name, so `strata verify` is correct.
- Do not use a phrasal verb such as "set up", "look up", or "carry out".
- Do not use a verb form in "-ing" where a noun or a clause can do the work.
- Do not use a contraction, an idiom, or a figure of speech.
- Use American spelling.
- Put a short fact in a comment on the command line, not in a sentence above it.
- Start a warning with the instruction. Put it in a block quote with bold text.

## The theme

`docs/gruvbox/` and `docs/mermaid*.js` are generated and tracked. `just
docs-theme` installs them again.
