# Report formats

`--format terminal` is the default. `--format markdown` is for a pull request
comment or an issue. `--format json`, or `--json`, is for a script or a language
model.

Every report goes to standard output. Progress notes go to standard error: see
[Settings](./settings.md).

## The terminal report

```text
{{#include ../output/tag-verify.txt}}
```

| Line | Content |
| --- | --- |
| first | The two sides and their uids. A cluster side reads `<name>@<version> on cluster <cluster>`, with the cluster uid. The update that waits for a cluster reads `<name>@<version> pending for cluster <cluster>`. An edge host side reads `<name>@<version> on edge host <host>`, and a cluster group side reads `<name>@<version> pinned by cluster group <group>`. A pack side is its pack ref, such as `'pack:Public Repo/kubernetes@1.35.3'`, with the pack uid. |
| `root` | The two roots, and `IDENTICAL`, `SAME CONTENT`, or `CHANGED`. |
| `note` | A cause of the result. [strata compare](../commands/compare.md#notes-in-the-report) lists the notes. |
| `palette` | Only with `--repave` or `--validate`: the answer of Palette for the cluster side, such as `no repave` or `accepts`. It is the answer of a check, so it is the last part of the report, after the diffs of `--full`. See [strata compare](../commands/compare/repave.md). |
| `images` | The number of images and charts that changed, and the number of packs that carry them. Below a changed pack, one line for each changed image or chart. |
| `consumers` | The number of clusters that used each side. When one side is a cluster, a candidate, or a pack, the line is a sentence about the profile side alone: see [strata compare](../commands/compare/versions.md#compare-two-versions). Between two clusters, or two packs, there is no line. |
| `variables` | `unchanged`, or `CHANGED` with one line for each added or removed variable, and one line for each changed field. Between two packs there is no line. |
| `cluster values` | Only between two clusters with different variable values: one line for each value, as `<profile>.<variable>: "old" -> "new"`. A sensitive variable shows `(sensitive)`. |
| `cluster tiers` | Only between two cluster records: the state of each tier (`applied`, `policy`, `running`, `shape`, `labels`), and one line for each changed field. See [Two clusters on one release differ](../use-cases/cluster-tiers.md). |
| `<n> packs:` | The count of the packs, and the count of each status: `5 packs: 4 changed, 0 added, 0 removed, 1 unchanged`. The heading names the kind, so a row of a pack gives its layer and its name. |
| a pack | The layer, the name, the version, and the flags. Below a changed pack: the upstream digest, the changed values, the manifest changes, and the impact. `upstream unknown` says that the pack has no upstream digest. |
| `who:` | Only after `verify --who` on a record that drifted: the writes in the audit log since the record, one line each. |
| `DRIFT:`, `UNVERIFIED:`, or `OK:` | Only after `verify`: the verdict. It is the last line. |

The variable traits are `required`, `sensitive`, `hidden`, and `immutable`. A
value in the `cluster values` block never changes `IDENTICAL`, because the
values of a cluster are not part of the release. A changed variable definition
does change it.

- Strata cuts a value longer than 120 characters, and gives its length. `--full`
  and the JSON document keep the full value.
- Strata cuts a root to its first twelve characters. It cuts the hash of an
  upstream digest the same way, and keeps the name of the algorithm:
  `sha256:ff3df77ebc9c`. The markdown comment cuts them the same way, and the
  JSON document keeps them full.
- Strata compares a value that is not valid YAML, such as an unquoted Go
  template, as trimmed text. The report says so.
- `--full` adds a unified diff of the raw values and of each changed manifest.

### Colors

| Color | Used for |
| --- | --- |
| bold green | `IDENTICAL`, `SAME CONTENT`, `OK`, `ADDED`, and the same-content note |
| bold red | `CHANGED`, `DRIFT`, `REMOVED`, `repave`, and `may-repave` |
| bold yellow | `UNVERIFIED`, the other notes, and `variables CHANGED` |
| bold | a profile version, and the answer of a check |
| blue | `IDENTITY`, a renamed tag, and the ref of a cluster or an edge host |
| magenta | `UPSTREAM`, and the name of a registry |
| yellow | `AUTHORED`, `cluster-wide`, the `~` of a changed pack, the pack-order note, the cluster states `Provisioning`, `Pending`, `Importing`, and `unknown`, and a warning |
| dim yellow | `IMAGES` and `CHARTS`: details of `AUTHORED`, so `AUTHORED` reads as the flag |
| cyan | `in-place`, the `@@` line of a unified diff, and the ref of a pack |
| red | old values, the `-` of a removed line, the cluster states `Error`, `Deleting`, and `Deleted`, and an error |
| green | new values, the `+` of an added line, and the cluster state `Running` |
| dim | `sync`, `none`, unchanged packs, uids, roots, and the progress notes |

`strata list`, `strata consumers`, and `strata-preflight --verbose` color a
cluster state. A state that strata does not know has no color.

Strata decides the color in this order:

- `--color=never` or `--no-color` removes it.
- `--color=always`, or `--color` with no value, forces it.
- Then `NO_COLOR` removes it.
- Then `CLICOLOR_FORCE` forces it, also for a pipe and also when `TERM` is
  `dumb`.
- Then `TERM=dumb` removes it.
- Then a terminal gets it.

Standard output and standard error each get their own decision in this order.
The report goes to standard output. The notes, the warnings, and the errors go
to standard error. A command with no `--color`, such as `snapshot`, starts at
`NO_COLOR`.

The diffs of `--full` use the same colors.

### The look of each line

Each line that strata writes is of one class, and each class has one look. The
text of the line keeps the class with no color, so a pipe and a CI log show it
too.

| Class | Look on a terminal | With no color |
| --- | --- | --- |
| A note of the progress, on standard error | dim | It starts with `strata:`. |
| The report, on standard output | plain | the same text |
| The answer of a check | bold, and the last line | the same text, last |
| A warning, on standard error | yellow | It starts with `strata: warning:`. |
| An error, on standard error | red | It starts with `strata: error:`. |

`--json` does not change: the document has no color, and an error is one JSON
object on standard error. Here the notes come first, and the answer of Palette
is the last line:

```text
{{#include ../output/candidate-validate-accepts.txt:1:5}}
```

The answer of each check is the last line of the check:

- `verify`: the verdict, `OK:`, `DRIFT:`, or `UNVERIFIED:`. The writes of
  `--who` and each `INVALID:` line come before it.
- `verify` of many records: the line of the counts.
- `same-release`: the verdict, `SAME RELEASE:` or `NOT THE SAME RELEASE:`.
- `compare --repave` and `compare --validate`: each `palette` line, after the
  report.
- `behind`: the answer of Palette about the bump of a pin, as the last line of
  the lines of that pin.
- `snapshot --validate`: the line `palette    accepts`, after the record. A
  refusal gives each `palette    refuses:` line, and then its error.
- `strata-preflight`: the verdict, `OK:`, `FAIL:`, `NOT READY:`, or
  `NOT COUNTED:`, after the scorecard.

An error stops a check, so the error is the last line then.

### The kind of each thing

The text names the kind of each thing that strata prints, also with no color. A
row gives the ref of its thing, so the output teaches the input. Or a heading
above the rows names their kind, such as `2 registries` or `5 packs:`. Then each
row stays short. A ref goes in single quotes when it holds a space or another
character that a shell reads. So a copy of the ref pastes back into a shell.

| Kind | Text | Color |
| --- | --- | --- |
| a profile version | `<name>@<version>` | bold |
| a pack | `pack:<registry>/<name>@<version>` | cyan |
| a cluster, an edge host | `cluster:<uid>`, `edgehost:<uid>` | blue |
| a registry | the word `registry` and its name, or a heading of registries | magenta |

The color is a second channel only. Where a sentence names a thing, a word of
its kind comes first, such as `cluster payments` or `registry Public Repo`. A
ref names its registry by its uid when the name of the registry holds a `/`, as
[Refs](./refs.md#rules) says.

| Command | What names the kind |
| --- | --- |
| `list --detail` | Each pack under a profile version is its ref, with `(tag <tag>)` when the profile pins another name. A pack with no registry reads `pack <name> <version> (no registry)`. |
| `packs` | The heading of each registry: `registry <registry>, pack <name>: <count>`. Each row below it is one version. |
| `behind` | Each row of a pin is the ref of the pin, and each row of a profile version is `<name>@<version>`. A carrier line gives the ref of the carried pack, with no registry: the newer version can take the pack from another registry. |
| `consumers` | Each row starts with the ref of the consumer: `cluster:<uid>` or `edgehost:<uid>`. A cluster template and an app deployment have no ref, so the row starts with the uid and ends with the kind. |
| `compare`, `verify` | A pack side of the heading is its ref. The heading `<n> packs:` comes before the rows of the packs. |
| `snapshot` of a set | Each line names the cluster by its ref, and then by its name. |

`strata-preflight` uses the same order for each stream. The report goes to
standard output, and the progress lines go to standard error, so each stream
gets its own decision. `strata-preflight` takes `--color` with a value: `auto`,
`always`, or `never`.

## The markdown comment

```text
{{#include ../output/compare-markdown.txt:1:2}}
```

{{#include ../output/compare-markdown.txt:3:}}

The comment has:

- a header table with the two uids, the two roots, the consumers of each side,
  and the fetch times
- a `Consumers:` line in place of that row, when only one side is a profile
  version: against a candidate, a cluster, or a pack
- a result line with the result, the maximum impact, the pack counts, and the
  state of the variables
- a `Note:` line for each note of the terminal report, and the order note as a
  quotation after the pack blocks. [strata compare](../commands/compare.md#notes-in-the-report)
  lists the notes
- one table row for each pack, with the layer, name, version, flags, and impact
- one collapsible `diff` block for the variables, and one for the cluster values
  when two clusters give different values
- one collapsible `diff` block for each changed pack, with the changed values,
  the upstream digest, and the manifest changes
- with `--full`, the unified diffs in `diff` code blocks

`verify` adds a `Verify` line and the verdict. The `Verify` line states the
status in the word of the verdict: `OK`, `UNVERIFIED`, or `DRIFT`.

The markdown writes each `&`, `<`, and `>` of a text as an entity, and GitHub
shows the same characters. A text from an answer, such as a message of Palette,
can hold a tag of HTML. A job summary or a pull request comment shows that tag
as text, also the tag of a link. A value in a code span, such as a uid,
shows as it is. [same-release](../commands/same-release.md#the-archives-of-the-two-builds)
shows the answer of the host of a chart in a reason. `strata-preflight` writes
its markdown in the same way.

The markdown also writes a `\` before each mark of markdown in a text: `*`,
`_`, `[`, `]`, `~`, a backtick, and a `\`. GitHub shows the same characters. So
a link, an emphasis, or a code span in a message of Palette shows as text. A
run of `_` with a letter or a digit on each side starts no emphasis. So such a
run stays as it is, as in `aws_stack`.

GitHub reads a `@name` in a text as a mention, and a mention notifies the
person or the team. So the markdown writes the entity of a zero width space
after each `@` that can start a mention: `@&#8203;name`. GitHub shows the same
text, and the mention notifies nobody. A `@` after a letter or a digit of ASCII
starts no mention, as in an address of mail or in `kubernetes@1.30.1`. Such a
`@` stays as it is. A value in a code span shows no mention, so it stays as it
is too.

GitHub makes a link of a bare address from its text as it is. So an escape in
the address shows in the link, and the link goes to another target. The
markdown keeps an address that GitHub surely links as it is, with each `&`,
mark, and `@` in it. GitHub reads no mention in a link. Each other part of the
text keeps its escapes. The markdown keeps an address that has each of these
forms:

- It starts with `https://` or `http://`, after a space, a `(`, a `"`, or a
  `'`, or at the start of the text.
- Or it starts with `www.` after a space or a `(`.
- Its host has two labels or more, as in `docs.example.com`. A label holds
  letters, digits, and `-` of ASCII, with a letter or a digit at each end.
- It holds only printable characters of ASCII, and no `<`.
- It does not end with an entity, such as `&amp;`.

GitHub ends the link before the marks at the end of an address, such as the
`.` or the `)` of a sentence. A `\` before such a mark stays in the link. So the
markdown writes a `*` there as `&ast;`, and a `_` as `&lowbar;`. A `~` there
stays as it is. Each other address, such as `https://localhost/`, keeps its
escapes, and GitHub shows them in its link.

Each fenced block has a fence of three backticks. When its text holds a run of
three or more backticks, the fence is longer than each run. So a line of the
values of a pack cannot end the block of `--full`, and the text after that line
stays in the block.

## The JSON document

The JSON is never colored and never cut. `--compact` writes it on one line.
With `--full`, a changed pack also has the unified diffs in the field `unified`:
see [The field unified](./documents.md#the-field-unified).
[JSON documents](./documents.md) describes each document.

## The one-line reports

`verify` with many files or with `--summary`, `same-release`, `consumers`,
`behind`, `list`, and `packs` print one line for each item. `verify`,
`same-release`, and `behind` close with a summary line. `consumers`, `list`,
and `packs` head each group with a count. `snapshot` of a `tag:` or `filter:`
set prints one `wrote` or `removed` line for each file. Each command page shows
the lines: [verify](../commands/verify.md),
[same-release](../commands/same-release.md),
[consumers](../commands/consumers.md), [behind](../commands/behind.md),
[list](../commands/list.md), [packs](../commands/packs.md), and
[snapshot](../commands/snapshot.md). `--format markdown` writes the same lines
as a table.
