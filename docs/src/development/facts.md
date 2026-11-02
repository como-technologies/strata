# The facts of Palette

Strata depends on facts about Palette. A fact is a request that strata sends, a field that it reads,
an error code that Palette gives, or a behavior that Palette shows after a change. A rule of the
cloud tier of strata-preflight also depends on the answer of a cloud. [The facts of the
clouds](#the-facts-of-the-clouds) are in the same list.

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
| `clouds.yaml` | The calls of the tool of a cloud that the cloud-tier rules send, and the tables of cloud facts that they hold. |

Each file has `schema: strata-facts/1` and the key `facts`. This fact is one entry of the list:

```yaml
{{#include ../../../xtask/facts/packs.yaml:example}}
```

An entry has these fields:

| Field | What it holds |
| --- | --- |
| `id` | The name of the fact: words with dashes. No two facts have one id. |
| `kind` | `request`, `behavior`, `gap`, or `cloud`. See the next table. |
| `request` | The method, the path, and the query of the request. A value of the query that varies is `{name}`, as in the path. The query holds each name that a rule of the fact sends in its path. Only a fact of the kind `request` has it. |
| `scope` | Only a fact of the kind `request` has it: `client` or `tenant`. The default is `client`. The run sends a fact of the scope `tenant` with no project. See [The scope of a request](#the-scope-of-a-request). |
| `reads` | What strata reads from the answer or depends on. `fields` are paths in the document: `[]` marks an array, and `*` marks the values of an object. `codes` are error codes of Palette. `values` are the values of a closed set, by field. `names` are the names that strata needs from a set that Palette can extend, by field. |
| `stand_in` | The routes of the stand-in that imitate the fact, as paths with `{name}` for a segment that varies. Empty when the stand-in serves no document for it. |
| `live` | The live checks that prove the fact, each as `script.sh:check`. |
| `unchecked` | Why no live check proves the fact. A fact has `live` or `unchecked`, and not both. A fact of the kind `cloud` has `call` or `unchecked`. |
| `pages` | The pages of the book that state the fact, each as a path under `docs/src/` with an optional `#anchor`. |
| `code` | The files of strata that use the fact. |
| `rules` | The rules of strata-preflight that use the fact. |
| `call` | Only a fact of the kind `cloud` has it: the read-only call of the tool of the cloud, as a list of words. `{name}` is a value of the lab. |
| `table` | Only a fact of the kind `cloud` has it: the date of a table that a rule holds, its values, and `per_account` for a table whose values each account sees for itself. See [The tables](#the-tables). |
| `palette` | Only a fact of the kind `cloud` has it: the route of Palette that lists a part of the answer, and the fields that it lacks. See [Add a fact of a cloud](#add-a-fact-of-a-cloud). |

A fact must have a code or a rule, because a fact that nothing uses is not a dependency.

| Kind | What it states |
| --- | --- |
| `request` | A request that strata or strata-preflight sends, and what they read from the answer. |
| `behavior` | What Palette does after a change or for a given input. It has no request of its own. |
| `gap` | A check that strata makes because no route of Palette answers it. `reads.codes` holds the code that Palette gives at the create. |
| `cloud` | A call of the tool of a cloud that a cloud-tier rule sends, or a table of cloud facts that a rule holds. See [The facts of the clouds](#the-facts-of-the-clouds). |

## Read the list from a script

`just facts` prints the list as JSON: the key `schema`, and the key `facts` with one object for each
fact. Each array is in each object, also when it is empty. A script reads it with `jq`:

```bash
just facts | jq -r '.facts[] | select(.kind == "request") | .request'
just facts | jq -r '.facts[] | select(.live == [] and .kind != "cloud") | .id + ": " + .unchecked'
just facts | jq -r '.facts[] | .live[] | split(":") | "\(.[0]) \(.[1])"'
just facts | jq -r '.facts[] | select(.call != []) | .call | join(" ")'
```

The first command lists each request that strata sends. The second lists each fact that no live check
proves, with the reason. The third lists each live check that a fact names, as the script and the check.
The fourth lists each call of a cloud.

## Add a fact

1. Find the file of the part of the book that states the fact. Add the entry at the end of the
   file, or in the place that the book orders.
2. Write the request exactly as the table of [Palette API requests](../reference/palette-api.md)
   writes it. Write the query that strata sends, such as `?filterType=tag`. Add each name that a
   rule of the fact sends in its path. If the page has no row for it, add the row first.
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
| A rule of strata-preflight sends a request that no fact has, or a fact names the wrong rules. The read of Palette that a command check makes first is such a request. | The rules and the list disagree. |
| A rule of a fact sends a query in its path, and the request of the fact lacks that query. | The fact does not state the request that the rule sends. Palette can refuse the request without the query. |
| A `##` heading of `internals/palette/` is on no fact. | The book states a fact that the list does not hold. |
| A GET fact reads a field that no document of the stand-in holds. | The contract run has no type to compare, and reports a difference for each tenant that gives the field. |
| A file of strata that a fact names does not exist. | The fact points at nothing. |
| An id is used twice, or two facts have one request. | The list holds a copy. |
| A rule of the cloud tier is in no fact of the kind `cloud`. | The call of a cloud is not in the list. |
| The call of a fact is the command of none of its rules, or no fixture of the rules answers it. | The fact, the rule, and the stand-in disagree. |
| The second command of a rule of the lab is the call of no fact that names the rule. | The contract run sends the calls of the list only, so nothing proves that command. |
| Two facts send one call. | The list holds a copy. |
| A rule or a file of strata holds a text that names a cloud fact, and no fact proves it. | A table of cloud facts has no contract check. See [The tables](#the-tables). |
| A table has no call, and a rule of its fact is for a cloud that the lab has. | The table has no contract check. |
| A fact with no call names a rule that runs a command on a cloud of the lab. | The run sends no call for that rule. A row, a rule of another cloud, and a check of the lab itself are not such rules. |
| A call is not a `list`, `describe`, `get`, `show`, or `simulate` operation. | The run sends each call every week, and must change nothing in the account. |
| A `palette` route has no method, lacks no field, or lacks a field that the fact does not read. | The fact names a route that answers nothing, or the whole call. |

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

The run sends GET requests only. It builds no cluster, and it changes nothing in the tenant. It
also sends the read-only calls of the clouds: see [The facts of the clouds](#the-facts-of-the-clouds).

### What the run compares

For each fact of the kind `request`, the run does these steps:

1. It reads the documents that the stand-in serves for the routes of `stand_in`. Each chapter of
   the book builds a tenant, and the run reads the documents of every chapter.
2. It finds an object in the tenant. The first entry of a listing is the object. The `{uid}` of
   a cluster comes from the cluster listing, and the `{configUid}` of a cloud config comes from the
   document of that cluster. A value of the query comes from a listing too: the `{cloudAccountUid}`
   of the regions route comes from `GET /v1/cloudaccounts/aws`. The security groups take the first
   account, the first region of that account, and the first VPC of that region.
3. It sends the request with its query, and compares the answer with the documents of the stand-in.

The request has the query that strata sends, or that the stand-in serves the route with. Some routes
need it. See [The query of a request](#the-query-of-a-request). The run sends the request in the
scope of the fact. See [The scope of a request](#the-scope-of-a-request).

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
The run asks the route in the same way. Four routes of AWS fail without their query, with HTTP 422.
See [The list routes](../internals/palette/validate-routes.md#the-list-routes):

- The regions route needs `cloudAccountUid`.
- The VPC route needs `cloudAccountUid`.
- The key pair route needs `cloudAccountUid`.
- The security group route needs `region` and `vpcId`. It takes `cloudAccountUid` too.

The stand-in puts the query in the name of the file of the document, after an `@`, and serves a
route with a query only with that query. A value of the query that varies is `{name}` in the request.
The run takes the value from a listing of the tenant, as it takes the `{uid}` of a path. The fact
`cloud-account-listing` gives the account of AWS. The fact `validate-aws-regions` gives its first
region, and the fact `validate-vpcs` gives the first VPC of that region. When the run finds no value
for a placeholder of the query, it skips the fact and gives the reason. It never sends the request
without the query.

### The scope of a request

A request names a project with the header `ProjectUid`, or names none. The run asks with the project
of `~/.palette.env`. A route of the tenant is not an operation of the project scope. Palette refuses
a request that names a project: HTTP 403, with the code `ProjectUidIsNotEmpty`. The refusal is the
same with each query. The saved tag filters are such routes. See
[An imported cluster](../internals/palette/clusters.md#an-imported-cluster).

A fact of such a route has `scope: tenant`. The run sends the request with a second client of the
same tenant that names no project, as strata does. The run sends the listing that finds the subject
in that scope too: the `{uid}` of `tag-filter` comes from `tag-filter-listing`. A fact with no
`scope` has the scope `client`, and the run sends it with the project.

### The sets of values

A fact lists a set of values under one of two keys of `reads`. The key says what the run does with
the set:

| Key | The set | The run reports |
| --- | --- | --- |
| `values` | A closed set. Palette gives no other value, and strata tells the values apart. | An unknown value, for a value that neither the fact nor the stand-in names. See [The unknown values](#the-unknown-values). |
| `names` | The names that strata needs from a set that Palette can extend. | A difference of the kind `name`, for a name of the fact that the answer lacks. A name that the fact does not list is no difference. |

A value that a closed set does not name is an unknown value and nothing else. It is not also a
difference, so one new value opens one issue.

The search schema is a set of names. The tenant has more properties than the three that strata
sends in a search. A set is in `values` unless strata ignores the values that it does not list.

The kinds of the audit log are in neither set. The fact `audit-log` reads the type of
`items[].spec.resource.kind` and lists no kind. An audit log lists the writes of the tenant, so a page
can hold no entry of a kind that strata acts on. It can also hold a kind that strata ignores, such as
`cloudaccount` or `registry`. Neither is a difference or an unknown value. The fact
`audit-resource-kind` lists the kinds that strata acts on.

An answer with no value at the field of a set of names has nothing to compare, as a list with no
entry has no type to compare.

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

A document of this chapter holds only the fields that a fact reads. Read the API definition of the
route before you add a field. The type of the field and each value must agree with it. A wrong type
makes the next contract run report a false `type` difference. For example, `spec.archType` of the
cluster listing is a list, not a text.

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
| `facts[]` | One entry for each fact of the kind `request`, in the order of the list. Then one for each fact of the kind `cloud`. |
| `facts[].id`, `kind`, `request` | The id of the fact, its kind, and its request. For a fact of the kind `cloud`, the request is the call. |
| `facts[].outcome` | The outcome of the fact. |
| `facts[].reason` | Why the run skipped the fact or could not check it. It also names the values of a table that the account cannot check, when it can check the others. |
| `facts[].differences[]` | Each difference: `kind` is `status`, `type`, `missing`, `name`, or `table`. `path` is the field. `expected` is what the stand-in has, and `found` is what the tenant has. For a `name`, `expected` lists the names that strata needs and the answer lacks, and `found` lists the names that the answer has. For a table, `expected` is the values that the answer lacks. |
| `facts[].unknownValues[]` | Each closed set that holds an unknown value. `set` is the field of `reads.values`. `known` is what the fact and the stand-in name, and `values` is what the tenant gave. The key is absent when the entry has none. |
| `facts[].live`, `pages`, `rules` | The live checks, the pages, and the rules of the fact. Only a fact of the kind `cloud` has `rules`. |
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

`just contract-profiles up` needs AWS credentials on the machine. The `aws` provider reads its
default chain also when no resource uses it. So `up` needs the credentials, and it makes no AWS
resource. `AWS_PROFILE=spectro-admin` gives them on kadomony.

The live scripts find their objects by the exact name or uid. The profiles of the set have other
names, so they change no result of a live script. Each listing of the profiles in a live script
matches by name or uid. A test of `tests/live_scripts.rs` reads the scripts and fails on a listing
that does not.

### Run it

```bash
just live-contract                     # the tenant that ~/.palette.env names; needs just examples
just live-contract --state state.json  # and the latest releases, compared with state.json
just live-contract --no-cloud          # the requests of Palette alone, with no call of a cloud
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

The service sets `AWS_PROFILE=spectro-admin`. It reads the values of the lab account from
`~/.config/strata-contract/lab.env`: see [The lab values](#the-lab-values). The run reads the
Palette key from `~/.palette.env`, so no key is in a unit file, in the repository, or in GitHub. A
user timer runs when no one is logged in only if the user has linger: `loginctl enable-linger`.
`install` tells when it is off.

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

## The facts of the clouds

A rule of the cloud tier of strata-preflight runs the tool of a cloud, such as `aws` or `gcloud`,
and reads the answer. The list holds a fact of the kind `cloud` for each of these calls. The fixtures
of the rules are the stand-in: the tests of the catalog read the answers in them. No live script
proves a cloud fact. The contract run proves it each week. [The facts of the
clouds](../internals/preflight.md#the-facts-of-the-clouds) says which calls the rules send.

This fact is one entry of `clouds.yaml`:

```yaml
{{#include ../../../xtask/facts/clouds.yaml:example}}
```

A cloud fact has these fields beside the common fields:

- `call` is the command, as a list of words. The call lists or describes, and changes nothing.
  `{region}` is a value of the lab account.
- `reads.fields` are the paths in the JSON that the command prints.
- `rules` are the rules of the cloud tier that read the answer. Each rule of the cloud tier must be in
  a fact.
- `palette` names a route of Palette that lists a part of the answer. `route` is the method and the
  path. `lacks` lists the fields of `reads.fields` that the route does not list. Each one is the reason
  why the rule stays in the cloud tier. A route that lacks no field answers the rule, and the rule moves to
  the palette tier.
- `unchecked` gives the reason when the run cannot send the call: the lab has no account of the
  cloud, or the rule has no command.

The first words of `call` up to the first flag or `{name}` are the command. They are the command of
each rule of the fact. The fixtures of those rules hold the answer that the run compares.

### The lab values

The lab account is the account that the tool of the cloud uses. `AWS_PROFILE=spectro-admin` in the
service selects it. A call names the values that depend on the account as `{name}`. The run reads
each value from the variable `STRATA_CONTRACT_<NAME>`, and then from the variables that the tool
reads:

| Value | Variables, in the order of use | Without a value |
| --- | --- | --- |
| `{region}` | `STRATA_CONTRACT_REGION`, `AWS_REGION`, `AWS_DEFAULT_REGION` | The run skips the call. |
| `{project}` | `STRATA_CONTRACT_PROJECT`, `CLOUDSDK_CORE_PROJECT`, `GOOGLE_CLOUD_PROJECT` | The run skips the call. |
| `{gcp_region}` | `STRATA_CONTRACT_GCP_REGION`, `CLOUDSDK_COMPUTE_REGION` | The run skips the call. |
| `{name_prefix}` | `STRATA_CONTRACT_NAME_PREFIX` | The prefix is empty. |
| `{role_arn}` | `STRATA_CONTRACT_ROLE_ARN` | The run skips the call. The role is one that the lab account can simulate. |
| `{cluster}` | `STRATA_CONTRACT_CLUSTER` | The run skips the call. The cluster is a GKE cluster of the lab. |
| `{image}` | `STRATA_CONTRACT_IMAGE` | The run skips the call. The image is in the image project of Spectro Cloud. |

A value is the name of an object, never a secret. The service reads the variables from
`~/.config/strata-contract/lab.env`, one `STRATA_CONTRACT_NAME=value` in each line.

### What the run compares for a cloud

For each cloud fact, the run does these steps:

1. It skips the fact when it has no call, when the tool is not on the `PATH`, or when the lab
   lacks a value. The entry gives the reason.
2. It sends the call. A call that fails, or prints no JSON, is the outcome `error`. An expired login
   of the tool is such a failure.
3. It compares the answer with the fixtures of the rules, as it does for a request of Palette.
   See [what the run compares](#what-the-run-compares). A field that each fixture has, and the
   answer lacks, is a difference.
4. For a table, it checks that the answer lists each value of the table. For a table of an account,
   it checks the values that the account lists. See [The tables](#the-tables).

The outcomes and the exit code are those of the requests of Palette. The run sends each call with a
time limit of two minutes. The document keeps the differences and the reasons, and no answer.

### The tables

A rule of any tier can hold a table of cloud facts as text, such as the zone ids that EKS refuses. The
fact of the table has a `table`. `as_of` is the date of the table: the day when the rule took its
values from their source. Change the date when you change the values. `holds` are the values. The
one field in `reads.fields` is the field of the answer that lists them.

The run checks the table in this way. Each value must be equal to a text or a number that the
answer lists at the field. A value that the answer does not list is a difference of the kind
`table`. The difference names the missing values and ten of the listed values.

Each account sees its own values in some tables. An account has its own zone ids: the lab account
has `usw1-az1` and `usw1-az3` in `us-west-1`, and not `usw1-az2`. A fact of such a table has
`per_account: true`. A value that the answer lacks is then no difference, because the account
cannot check it. When the answer lists some values, the outcome is `same`, and the reason names the
values that the account cannot check. When the answer lists none, the outcome is `skipped`, and the
reason says that the account cannot check the table. A table with no `per_account` keeps the check:
a value that the answer lacks is a difference.

The check of the list finds a table that has no fact. It reads the programs and the arguments of
each rule of any tier, and the code of strata. It finds these texts:

- A zone id, a region, an instance type, or a machine type.
- An Azure size, an image id, an AMI type, or a quota code.

`SHAPES` in `xtask/src/facts/cloud.rs` lists the patterns. The code of strata holds no such text.
Read the fact from the cloud or from a route of Palette instead.

A fact proves a text of a rule in one of two ways. The text is a value of its table, or it is in its
call, as a quota code is. A table needs a call. A table of a cloud that the lab has no account of
has no call, and `unchecked` gives the reason.

### Add a fact of a cloud

1. Write the call in `clouds.yaml`. Its first words, up to the first flag, are the command of the rule, or its second command (`also`).
2. List the fields of the answer that the rule reads in `reads.fields`.
3. Make sure that a fixture of the rule answers the call with those fields. The `_exec` entry of the
   fixture holds the answer.
4. Name the rule in `rules`, and the page `internals/preflight.md#the-calls-of-the-clouds` in
   `pages`.
5. Ask Palette first. If a route lists a field that the rule reads, read the route in the rule.
   Use the call for the other fields. If the route lists every field, move the rule to the palette
   tier, and remove the fact. If it lists some fields, write `palette` with the fields that it lacks.
6. If the lab cannot send the call, write the reason in `unchecked`, and omit `call`.
7. Run `cargo xtask facts --check`.

For a table, add a fact with `table`. Write its call, the one field that lists the values, the date,
and the values. Each text value must be in a program of the rule.

### The issues of the clouds

The report opens an issue with the label `cloud-drift` for a cloud fact that differs, in the
milestone `Backlog`. The marker of the fact is on the first line, as it is for a request. The
report updates the issue while the fact differs, and closes it when the fact is the same again. The
report creates the label when the repository has none. `--cloud-label` gives another name.

### What to do with an issue of the clouds

An issue of the label `cloud-drift` means that a cloud answers a call in another way than the
fixtures of the rules. Or the answer lacks a value of a table. Do these steps:

1. Send the call by hand in the lab account. A difference can come from the account: a role that
   was deleted, or a login that expired.
2. If the cloud changed, change the rules that read the fact, their fixtures, the fact, and the
   book together. The check of the list and the tests of the catalog show what disagrees.
3. If a table lost a value, read the source of the table again. For the zone ids, it is the
   documentation of EKS. Change the table and its date together.

## When Palette changes

A live check that fails shows a fact that Palette changed. So does an issue of the label
`palette-drift`, which [the weekly contract run](#the-weekly-contract-run) opens. Change the fact,
the stand-in, and the page together, so that the three places agree again.

When Palette adds a route that answers a fact of the kind `gap`, remove the check of strata and the
fact. The rule is "ask Palette first": see [Where each part belongs](../boundary.md#the-rules-of-the-scope).
