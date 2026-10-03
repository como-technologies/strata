# CLAUDE.md

Guidance for Claude Code in this repository.

**The book is the documentation.** `docs/src/` holds each fact. Read the page
before you change what it describes. Put a new fact in the book. Do not put it
in this file, and do not grow the README with it.

## Documentation rules

These are hard rules. `docs/src/development/documentation.md` explains each one.

1. **Keep `README.md` short:** what strata does, a quick start, the commands,
   install, links, requirements. A deep dive goes in the book, with at most one
   link line in the README.
2. **Write ASD-STE100 Simplified Technical English** in the book, the README,
   the command help text, the code comments, and the commit messages.
3. **Never paste strata output into a page.** Add a step to
   `xtask/src/chapters.rs`, run `just docs-examples`, and include the file from
   `docs/src/output/`.
4. **Do not show "Merkle" to a user.** The root is a digest. Only
   `docs/src/internals/hashes.md` names the tree.
5. **A command page** shows what the command does, real output, what to expect,
   and its options. **A use case** shows the problem, the commands, the output,
   and the fix. When it depends on a behavior of Palette, it links to the live
   run that shows that behavior.
6. **State a live result as a fact**, with no run id and no date, in
   `docs/src/internals/palette/`
   and `docs/src/development/live-runs.md`.
7. **Do not cite the code of Palette.** No path, line number, or function name
   from the Spectro Cloud repos in a comment or a page. State what Palette does,
   with the error code it returns or the live run that shows it.

## Scope rules

`docs/src/boundary.md` explains each rule in "The rules of the scope".

1. **Ask Palette. Do not copy its rules.** Call its validate route.
2. **Do not repeat a status that Palette shows.** Record it only when a
   difference needs it.
3. **`packs` serves the records and `behind`.** It is not a catalog.
4. **A check of build readiness goes to `strata-preflight`.**
5. **Each item names its question** from the table in `introduction.md`, or
   its gap of build readiness. If it names none, ask the lead and brett first.

## Code rules

Read `docs/src/development/code.md` before you change the Rust code: the layers,
the visibility, the types, the errors, the ownership, the repetition, the
comments, and the tests. A change that breaks one of its rules does not pass the review.

## Before you finish

Run the CI recipes in a clean worktree of the commit, not only in this tree:

```bash
just lint && just test && just library-example && just examples && just docs-check && just docs
```

## The live example

It runs `tofu`, never `terraform`. `just tofu-install` installs the pinned
version. Every recipe that creates something is billable until `just live-down`.
