# The facts of Palette

Strata depends on facts about Palette. A fact is a request that strata sends, a field that it reads,
an error code that Palette gives, or a behavior that Palette shows after a change.

Each fact is in three places:

- The stand-in for Palette. Each test and each output of the book assumes it. See
  [Add an output](./documentation.md#add-an-output).
- A live check, which proves the fact against a real tenant. See
  [What the live example verifies](./live-runs.md).
- The book, which tells the user. See [What Palette does](../internals/palette.md) and
  [Palette API requests](../reference/palette-api.md).

Palette changes, and the three places can stop agreeing with each other, or with Palette. The list of
the facts links them. A check fails when a link is missing or wrong.

## The list

The list is the files `xtask/facts/*.yaml`. One file holds the facts of one part of the book:

| File | The facts it holds |
| --- | --- |
| `profiles.yaml` | The requests of a profile version, the audit log, the macros, the gaps that strata fills, and the sections of `internals/palette/profiles.md`. |
| `clusters.yaml` | The requests of a cluster, the fleet, an edge host, a cluster group, and an app profile, and the sections of `clusters.md` and `other-objects.md`. |
| `packs.yaml` | The requests of a registry and of a pack, and the sections of `packs.md`. |
| `validate.yaml` | The validate routes, the property routes, the list routes, and the sections of `validate-routes.md`. |
| `preflight.yaml` | The routes that only the rules of strata-preflight send, and the gaps that preflight fills. |
| `limits.yaml` | The behaviors of `limits.md`, which no run covers or which one run shows in part. |

Each file has `schema: strata-facts/1` and the key `facts`. This fact is one entry of the list:

```yaml
{{#include ../../../xtask/facts/packs.yaml:example}}
```

An entry has these fields:

| Field | What it holds |
| --- | --- |
| `id` | The name of the fact: words with dashes. No two facts have one id. |
| `kind` | `request`, `behavior`, or `gap`. See the next table. |
| `request` | The method and the path of the request. Only a fact of the kind `request` has it. |
| `reads` | What strata reads from the answer or depends on. `fields` are paths in the document: `[]` marks an array, and `*` marks the values of an object. `codes` are error codes of Palette. `values` are the values of a closed set, by field. |
| `stand_in` | The routes of the stand-in that imitate the fact, as paths with `{name}` for a segment that varies. Empty when the stand-in serves no document for it. |
| `live` | The live checks that prove the fact, each as `script.sh:check`. |
| `unchecked` | Why no live check proves the fact. A fact has `live` or `unchecked`, and not both. |
| `pages` | The pages of the book that state the fact, each as a path under `docs/src/` with an optional `#anchor`. |
| `code` | The files of strata that use the fact. |
| `rules` | The rules of strata-preflight that use the fact. |

A fact must have a code or a rule, because a fact that nothing uses is not a dependency.

| Kind | What it states |
| --- | --- |
| `request` | A request that strata or strata-preflight sends, and what they read from the answer. |
| `behavior` | What Palette does after a change or for a given input. It has no request of its own. |
| `gap` | A check that strata makes because no route of Palette answers it. `reads.codes` holds the code that Palette gives at the create. |

## Read the list from a script

`just facts` prints the list as JSON: the key `schema`, and the key `facts` with one object for each
fact. Each array is in each object, also when it is empty. A script reads it with `jq`:

```bash
just facts | jq -r '.facts[] | select(.kind == "request") | .request'
just facts | jq -r '.facts[] | select(.live == []) | .id + ": " + .unchecked'
just facts | jq -r '.facts[] | .live[] | split(":") | "\(.[0]) \(.[1])"'
```

The first command lists each request that strata sends. The second lists each fact that no live check
proves, with the reason. The third lists each live check that a fact names, as the script and the check.

## Add a fact

1. Find the file of the part of the book that states the fact. Add the entry at the end of the
   file, or in the place that the book orders.
2. Write the request exactly as the table of [Palette API requests](../reference/palette-api.md)
   writes it. If the page has no row for it, add the row first.
3. Name the route of the stand-in. If the stand-in serves no document for the fact, leave `stand_in`
   empty.
4. Name the live checks that prove the fact. Each name is a check that the script calls, and each
   check has a row in [What the live example verifies](./live-runs.md). If no check proves the fact,
   write the reason in `unchecked`.

   A script can build a check name from a variable, such as `fault-$id`. The book writes that name
   with a placeholder, such as `fault-<id>`. A fact can name the pattern in either form, or one
   name that the pattern makes.
5. Name the page with its heading, and the files or the rules that use the fact.
6. Run `cargo xtask facts --check`. Then run `just docs-check`, which adds the check of the stand-in.

## What the check does

The check reads the list, the book, the scripts of `examples/live/`, the catalog of rules that
strata-preflight carries, and the routes that the stand-in serves. It reads the rules with the types
of strata-preflight, and writes no second parse of a rule file. It reports each fault with the id of
the fact.

| The check fails when | Why |
| --- | --- |
| A route that the stand-in serves is in no `stand_in`. | The stand-in imitates a route that no fact states. |
| A `stand_in` matches no route that the stand-in serves. | The fact names a route that the stand-in lost. |
| A fact has no `live` and no `unchecked`, or both. | No one knows how the fact is proved. |
| A live check names a script that does not exist, or a check that the script does not call. | The proof is not there. |
| A live check has no row in the book. | The book does not tell what the check establishes. |
| A page or an anchor of a fact is not in the book. | The book does not state the fact. |
| An error code of a fact is on none of its pages. | The code has a typing fault, or the page lost it. |
| A table of `reference/palette-api.md` lists a request that no fact has. | The book lists a request that the list does not hold. |
| A rule of strata-preflight sends a request that no fact has, or a fact names the wrong rules. | The rules and the list disagree. |
| A `##` heading of `internals/palette/` is on no fact. | The book states a fact that the list does not hold. |
| A file of strata that a fact names does not exist. | The fact points at nothing. |
| An id is used twice, or two facts have one request. | The list holds a copy. |

Some headings of `internals/palette/` state no fact about Palette, such as "Where these answers
come from". `NOT_FACTS` in `xtask/src/facts/check.rs` lists them.

`cargo test -p xtask` makes every rule, except the two rules of the stand-in. `just test` runs it.
`just docs-check` and `just docs-examples` run each chapter of the book, so they know the routes that
the stand-in serves, and they make the two rules of the stand-in too.

## When Palette changes

A live check that fails shows a fact that Palette changed. Change the fact, the stand-in, and the page
together, so that the three places agree again.

When Palette adds a route that answers a fact of the kind `gap`, remove the check of strata and the
fact. The rule is "ask Palette first": see [Where each part belongs](../boundary.md#the-rules-of-the-scope).
