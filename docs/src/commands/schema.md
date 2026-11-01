# strata schema

Print the JSON Schema of a document or of the candidate file.

```bash
strata schema diff > diff.schema.json
```

| Kind | The document |
| --- | --- |
| `snapshot` | `strata-snapshot/1`: a record from `snapshot` |
| `diff` | `strata-diff/1`: `compare --json`, and `verify --json` for one file |
| `verify-summary` | `strata-verify-summary/1`: `verify --json` for many files |
| `chain` | `strata-chain/1`: `same-release --json` |
| `consumers` | `strata-consumers/1`: `consumers --json` |
| `behind` | `strata-behind/1`: `behind --json` |
| `list` | `strata-list/1`: `list --json` |
| `packs` | `strata-packs/1`: `packs --json` |
| `set` | `strata-set/1`: `snapshot --dir --json` for a set of clusters |
| `candidate` | the candidate file |

```text
{{#include ../output/schema-diff.txt}}
```

Give the schema to a script or a language model that reads the documents. The
`schema` field is a `const` in it, so a validator refuses a document of another
id. The diff, verify-summary, and chain documents also have a `legend` that
explains their values in words. Each `description` is plain text: a link in a
doc comment shows as a code span.
[JSON documents](../reference/documents.md) describes each field.

## The shape of each open field

Some fields hold a JSON document, and not a fixed set of keys. The schema states
the shape of each one, so a validator refuses a value of another shape:

- A profile variable of a record or of a candidate has twelve fields. Only
  `name` is required.
- A segment of the path of a leaf is a string, an integer, or an object with a
  `name`.
- The `params` of a pack, and of a layer of a candidate, are an object of text.
  The one exception is `installOrder` of an app tier, which is an integer.
- A cluster record lists the keys of each tier by its name. `propertyNames`
  also takes the name of a tier of a later release.
- A cluster record lists the parts that strata could not read in
  `source.cluster.errors`. The schema names the keys, and `propertyNames` also
  takes the key of a later release.
- An `error` is an object with a `kind` and a `message`, and the fields of the
  kind. See [Errors](../reference/errors.md).
- Each time has `format: date-time`. One time can be empty text: `recordedAt`
  of a row of `verify --summary` for a file that strata cannot read.

```text
{{#include ../output/schema-variable.txt}}
{{#include ../output/schema-segment.txt}}
{{#include ../output/schema-tiers.txt}}
{{#include ../output/schema-read-errors.txt}}
{{#include ../output/schema-time.txt}}
```

A record that an older release wrote still validates, and it keeps its root. One
case differs: a record with a variable that has `name: null`. Strata does not
read it. See [The documents](../reference/compatibility.md#the-documents).

## Options

```text
{{#include ../output/help-schema.txt}}
```
