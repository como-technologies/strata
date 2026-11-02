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
| `request` | The method, the path, and the query of the request. A value of the query that varies is `{name}`, as in the path. Only a fact of the kind `request` has it. |
| `reads` | What strata reads from the answer or depends on. `fields` are paths in the document: `[]` marks an array, and `*` marks the values of an object. `codes` are error codes of Palette. `values` are the values of a closed set, by field. `names` are the names that strata needs from a set that Palette can extend, by field. |
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
   writes it. Write the query that strata sends, such as `?filterType=tag`. If the page has no row
   for it, add the row first.
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
| A GET fact reads a field that no document of the stand-in holds. | The contract run has no type to compare, and reports a difference for each tenant that gives the field. |
| A file of strata that a fact names does not exist. | The fact points at nothing. |
| An id is used twice, or two facts have one request. | The list holds a copy. |

Some headings of `internals/palette/` state no fact about Palette, such as "Where these answers
come from". `NOT_FACTS` in `xtask/src/facts/check.rs` lists them.

`cargo test -p xtask` makes every rule, except the two rules of the routes of the stand-in. `just test`
runs it. It makes the rule of the fields too, when `just examples` has built the examples.
`just docs-check` and `just docs-examples` run each chapter of the book, so they know the routes and
the documents that the stand-in serves, and they make the three rules of the stand-in.

## The weekly contract run

The three places of a fact can agree with each other and still disagree with Palette. The contract
run finds that. It asks a real tenant the requests of the facts, and compares each answer with the
stand-in. A timer on kadomony runs it each week. It opens an issue for each fact that differs, and for
each closed set that holds a value which strata does not know.
kadomony is the machine of the maintainers that holds the key of the lab tenant.

The run sends GET requests only. It builds no cluster, and it changes nothing in the tenant.

### What the run compares

For each fact of the kind `request`, the run does these steps:

1. It reads the documents that the stand-in serves for the routes of `stand_in`. Each chapter of
   the book builds a tenant, and the run reads the documents of every chapter.
2. It finds an object in the tenant. The first entry of a listing is the object. The `{uid}` of
   a cluster comes from the cluster listing, and the `{configUid}` of a cloud config comes from the
   document of that cluster. A value of the query comes from a listing too: the `{cloudAccountUid}`
   of the regions route comes from `GET /v1/cloudaccounts/aws`.
3. It sends the request with its query, and compares the answer with the documents of the stand-in.

The request has the query that strata sends, or that the stand-in serves the route with. Some routes
need it. See [The query of a request](#the-query-of-a-request).

The comparison has three parts:

| Part | A difference is |
| --- | --- |
| The status | The tenant answers with a status of 4xx, other than 401 and 429. The stand-in answers 200. |
| The type of a field | A field of `reads.fields` has a type that no document of the stand-in has there. Or each document of the stand-in has the field, and no place of the answer has it. |
| The names | A name of `reads.names` is not in the answer, and the answer has a value at that field. |

The run compares shapes. It never compares the values of two tenants. A `null` and a missing field
are one type, `absent`. A field that some documents of the stand-in lack can be missing from the
answer. A list with no entry has no field to compare.

The outcome of a fact is one of these:

| Outcome | What it means |
| --- | --- |
| `same` | The answer has the status and the types of the stand-in. It can hold an unknown value. |
| `differs` | The answer differs. The entry lists each difference. |
| `skipped` | The run sent no request, or the tenant has no object to ask about. The entry gives the reason. |
| `error` | The run could not check the fact. A 401, a 429, a status of 5xx, or a failure of the network is the cause. |

The run skips a fact in these cases:

- The request is a POST. It sends a body, and the live checks of the fact send it.
- The stand-in serves no document for the fact, or the fact reads no field, no value, and no name.
- The tenant has no object for a placeholder. An empty tenant skips most of the facts.
- No source gives an object for a placeholder. `cloud-account` and the edge hosts are such facts.

A skipped fact names its live checks in the entry. Run them in a live window to prove the fact.

### The query of a request

The request of a fact has the query that strata sends, or that the stand-in serves the route with.
The run asks the route in the same way. Two routes fail without their query:

- The filter listing needs `filterType=tag` in a project. See
  [An imported cluster](../internals/palette/clusters.md#an-imported-cluster).
- The regions route needs `cloudAccountUid`. See
  [The list routes](../internals/palette/validate-routes.md#the-list-routes).

The stand-in puts the query in the name of the file of the document, after an `@`, and serves each of
these routes only with its query. A value of the query that varies is `{name}` in the request. The
run takes the value from a listing of the tenant, as it takes the `{uid}` of a path. The fact
`cloud-account-listing` gives the account of AWS.

### The sets of values

A fact lists a set of values under one of two keys of `reads`. The key says what the run does with
the set:

| Key | The set | A difference is |
| --- | --- | --- |
| `values` | A closed set. Palette gives no other value, and strata tells the values apart. | A value that neither the fact nor the stand-in names. See [The unknown values](#the-unknown-values). |
| `names` | The names that strata needs from a set that Palette can extend. | A name of the fact that the answer lacks. A name that the fact does not list is no difference. |

The search schema is a set of names. The tenant has more properties than the three that strata
sends in a search.
The kinds of the audit log are names too. The log of the lab has the kinds `cloudaccount` and
`registry`, and strata does not act on them. A set is in `values` unless strata ignores the values
that it does not list.

An answer with no value at the field of a set of names has nothing to compare, as a list with no
entry has no type to compare. An audit log lists the writes of the tenant. A page that holds no entry
of a kind that strata needs makes a difference. Create an entry of that kind, or remove the
kind from the fact.

### The fields of the stand-in

The run compares the type of a field in the answer with the types in the documents of the stand-in.
A field that no document holds has no type to compare. The run then reports a difference for each
tenant that gives the field. So the check of the facts fails when a GET fact reads a field that no
document of its stand-in holds. A document holds a field when it has a value there that is not
`null`.

A field that a tenant can omit needs two documents: one that holds it, and one that does not.
The OCI registries are an example. A registry that recovered a sync has `syncRecovery` and no
`message`. A registry that recovers has both.

The chapters of the book build the documents. Some fields are in no page, such as the note of an
audit entry. The chapter `fields` in `xtask/src/chapters/fields.rs` holds one document for each such
field. It runs no command and writes no output. Add a document there when the check names a field
that the other chapters omit.

### The unknown values

A fact can name the values of a closed set, by field, in `reads.values`. The run reads the text at
each such field of the answer. A text that neither the fact nor the documents of the stand-in name
is an unknown value: Palette gave a value that strata does not know.

An unknown value is no difference. The status and the types can be the same, so the outcome stays
`same`. The entry lists the values for each field, and the run counts each value one time in the
summary. A new value makes the exit code 1.

The library of strata finds the same values in the run of a user, and writes a note. See
[The note for a value that Palette gives](../reference/compatibility.md#the-note-for-a-value-that-palette-gives).
The contract run asks one real tenant on a schedule. The note comes from the tenants of the users,
which the lab does not run.

### The result

The result is the document `strata-contract/1`. `contract.sh` writes it to `contract.json`.

| Field | What it holds |
| --- | --- |
| `schema` | `strata-contract/1`. |
| `checkedAt` | The time of the run, in RFC 3339. |
| `target` | The address of the tenant. The document holds no key. |
| `summary` | The count of the entries with each outcome: `same`, `differs`, `skipped`, `error`. `unknownValues` counts each unknown value. |
| `facts[]` | One entry for each fact of the kind `request`, in the order of the list. |
| `facts[].id`, `request` | The id of the fact and its request. |
| `facts[].outcome` | The outcome of the fact. |
| `facts[].reason` | Why the run skipped the fact or could not check it. |
| `facts[].differences[]` | Each difference: `kind` is `status`, `type`, `missing`, or `name`. `path` is the field. `expected` is what the stand-in has, and `found` is what the tenant has. For a `name`, `expected` lists the names that strata needs and the answer lacks, and `found` lists the names that the answer has. |
| `facts[].unknownValues[]` | Each closed set that holds an unknown value. `set` is the field of `reads.values`. `known` is what the fact and the stand-in name, and `values` is what the tenant gave. The key is absent when the entry has none. |
| `facts[].live`, `pages` | The live checks and the pages of the fact. |
| `releases[]` | The latest release of each repository, when the run keeps a state file. See [The releases](#the-releases). |

The run exits with one of these codes:

- 0: each fact that the run checked is the same, and no value is unknown.
- 1: a fact differs, or a value is unknown.
- 2: the run could not check a fact or a release.

A skipped fact does not change the code.

### The releases

A new release of the Palette SDK or of the Terraform provider can change what Palette does. With
`--state FILE`, the run reads the latest release of `spectrocloud/palette-sdk-go` and of
`spectrocloud/terraform-provider-spectrocloud` from GitHub. It sends no token.

The run compares each tag with the tag that the last run saw, which the state file keeps. A tag
that differs is new. The first run only keeps the tags.

The run orders no version. A tag of the SDK, such as `v4.10.b`, is no Semantic Version. The latest
release is the one that GitHub names.

Each entry of `releases[]` has these fields:

| Field | What it holds |
| --- | --- |
| `repo` | The repository, as `owner/name`. |
| `tag`, `publishedAt` | The tag of the latest release, and the time of its publish. |
| `seen` | The tag that the last run saw. The first run has none. |
| `new` | True when `tag` is not `seen`. |
| `error` | Why the run could not read the release. The state keeps the old tag. |

### The issues

`contract-report.sh` turns the result into issues of the repository. It opens, updates, and closes
them in this way:

- A fact that differs has one open issue with the label `palette-drift`, in the milestone
  `Backlog`. The label is created when the repository has none. The first line of the body is the
  marker `<!-- strata-contract: <fact id> -->`, and the script finds the issue by it.
- The script updates the issue while the fact differs. It closes the issue with a comment when the
  fact is the same again.
- A fact with an unknown value has one open issue for each closed set that holds one. The issue has
  the title "Palette has a new value of" the field and the fact, the label `palette-drift`, and the
  milestone `Backlog`. The first line of the body is the marker
  `<!-- strata-contract: values <fact id> <field> -->`. The body lists each new value and each
  value that the fact and the stand-in name. The script updates the issue while the value is there.
  It closes the issue with a comment when the run checked the fact and found no new value.
- A skipped fact and a fact that the run could not check keep their issues as they are.
- A new release has one issue, "Prove the facts again on" the repository and the tag. A person
  closes it.

The script exits with 0 when no fact differs, no value is unknown, and no release is new. It exits
with 1 when it handled a fact that differs, an unknown value, or a new release. It exits with 2
when a fact or a release could not be checked, or when a call to GitHub failed.

Each body links this page in the repository that the report writes to. That repository is `--repo`,
or the repository that GitHub names for the checkout. A dry run calls no host, so it takes the
repository from `--repo` or from the `origin` remote of the checkout. When it finds none, it says so,
and the links name `OWNER/REPO`.

The script reads the host layer of `scripts/ci/verify-report.sh`, so it holds no second copy of the
functions that call GitHub. The contract script is not shipped, so it can read a file of the
repository. The shipped report scripts stay copies on purpose.

### The profiles of the run

Some facts need a cluster profile. The run reads the first entry of
`GET /v1/dashboard/clusterprofiles/metadata`. After `just live-down`, the project holds no
profile, so the run finds no object and skips these facts.

The project keeps a standing set of cluster profiles for the run. The set holds profiles only. It
has no cluster, no AWS resource, no access key, and no registry. Make the set once, from the
clone that the timer uses:

```bash
just contract-profiles up      # show the plan, and make the profiles after you say yes
just contract-profiles down    # show the plan, and remove the profiles after you say yes
just contract-profiles status  # list what the state holds; exit 1 when it holds nothing
```

`contract-profiles.sh` applies the module of [the live example](./live-example.md) with these
variables:

| Variable | Value | Reason |
| --- | --- | --- |
| `name_prefix` | `strata-contract` | The names of the set differ from the names of the live example. |
| `cluster` | `false` | The set builds no cluster. |
| `own_registries` | `false` | The module makes no ECR repository, no IAM user with its key, no Palette registry, and no profile that needs one. |
| `cloud` | `aws` | The cloud `eks` makes a role in AWS. |

The cluster group, the cluster template, and the registry kinds stay off, as their defaults say.
The default of `own_registries` is `true`, and no recipe of the live example sets it. So the live
example builds what it built before the switch.

The set has a state of its own: the file `contract.tfstate` in `examples/live/terraform/`. `live-up`
and `live-down` read `terraform.tfstate`. So `live-down` never removes the set, and `live-up`
never sees it. The recipe needs the `terraform.tfvars` of the live example for the names in the
tenant. It reads the key from `~/.palette.env` through `tf.sh`, and never prints it.

The live scripts find their objects by the exact name or uid. The profiles of the set have other
names, so they change no result of a live script. Each listing of the profiles in a live script
matches by name or uid. A test of `tests/live_scripts.rs` reads the scripts and fails on a listing
that does not.

### Run it

```bash
just live-contract                     # the tenant that ~/.palette.env names; needs just examples
just live-contract --state state.json  # and the latest releases, compared with state.json
just contract-report --dry-run runs/contract.json
```

The run writes `contract.json` and `contract.log` under `examples/live/runs/<UTC time>/contract/`,
or under `OUT`. `just contract-report` takes the path of `contract.json`, and the options of
`contract-report.sh`.

### The timer

The timer is a systemd user timer on kadomony. `deploy/contract/weekly.sh` is the job. It updates
the checkout with `git pull --ff-only`, builds the examples, runs the contract run, and runs the
report. Install the timer from a clone of `main` that nothing else uses:

```bash
just contract-timer-install   # write the units to ~/.config/systemd/user/, and start the timer
just contract-timer-status    # the timer, and the last result
just contract-timer-remove    # stop the timer, and delete the units; the results stay
```

The service sets `AWS_PROFILE=spectro-admin`. The run reads the Palette key from `~/.palette.env`,
so no key is in a unit file, in the repository, or in GitHub. A user timer runs when no one is
logged in only if the user has linger: `loginctl enable-linger`. `install` tells when it is off.

The job keeps its results in `~/.local/share/strata-contract/`, or in `STRATA_CONTRACT_DIR`:

- `runs/<UTC time>/` holds each run. The 12 newest runs stay.
- `last.json` holds the newest result.
- `state.json` holds the tags of the releases that the last run saw.

The run writes the new tags to a copy, `runs/<UTC time>/state.json`. The job moves the copy onto
`state.json` only when the report handled the issues: the report exits with 0 or 1. When the
report fails, the old tags stay. The next run then notes the new tag again, and the report opens
the issue of the release. A report that fails never loses the issue of a release.

The job exits with 2, and the service shows a failure, when the run could not check a fact or a
release. It also exits with 2 when the report could not write an issue. A fact that differs, or a
value that is unknown, is no failure: the issue is the report.

### What to do with an issue

An issue of drift means that Palette answers one request in another way than the stand-in. The
issue lists each difference, the live checks, and the pages of the fact. Do these steps:

1. Prove the fact again with its live checks. A difference can come from the tenant: an object
   that lacks a field, or a key that cannot read a route.
2. For a `name` difference, read the names that the tenant has. If Palette renamed a name that strata
   needs, change the fact, the code, and the page together. If the tenant lacks the name, learn
   why: an audit log lacks a kind when no entry has it.
3. If Palette changed, change the stand-in, the fact, and the page together. See the next section.
4. If Palette is wrong, tell the lead. The lead records the fault of Palette in the local findings
   log. Do not open an issue for it.

### What to do with a new value

An issue "Palette has a new value of" a field means that Palette gave a text which neither the fact
nor the stand-in names. Strata reads such a text with no error and keeps it, but it does not tell
the text apart from the values it knows. The issue lists each new value and the field. Do these
steps for each new value:

1. Prove the value again. Read the answer of the request by hand, or run a live check of the fact.
   A value can come from one object of the lab tenant, such as a cluster that a person made by hand.
2. Find the closed set of strata that reads the field. The files in `code` of the fact name it.
   Most sets are `Open` values, so they keep the text of a value that they do not name.
3. Decide, and write the decision down:
   - Add the value to the closed set when strata must tell it apart. Add the variant with
     `impl_closed_set!`, and add its text to the test of the sets in `tests/closed_set.rs`. Then
     add the value to `reads.values` of the fact and to the page of the fact.
   - Do not add the value when strata does not need to tell it apart. Add it to `reads.values` of
     the fact so that the next run counts it as known. Write in the page of the fact why the set
     stays as it is, and what each command does with the value.
4. Run `cargo xtask facts --check` and `just docs-check`. The issue closes at the next run, when
   the run checked the fact and found no new value.

A person who sees the note on standard error has the same choice. The note names the type of the
set, such as `PackState`, and the log has the route. The fact for that route names the field.

## When Palette changes

A live check that fails shows a fact that Palette changed. So does an issue of the label
`palette-drift`, which [the weekly contract run](#the-weekly-contract-run) opens. Change the fact,
the stand-in, and the page together, so that the three places agree again.

When Palette adds a route that answers a fact of the kind `gap`, remove the check of strata and the
fact. The rule is "ask Palette first": see [Where each part belongs](../boundary.md#the-rules-of-the-scope).
