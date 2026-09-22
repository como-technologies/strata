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
| `list` | `strata-list/1`: `list --json` |
| `packs` | `strata-packs/1`: `packs --json` |
| `candidate` | the candidate file |

```text
{{#include ../output/schema-diff.txt}}
```

Give the schema to a script or a language model that reads the documents. The
`schema` field is a `const` in it, so a validator refuses a document of another
id. The diff, verify-summary, and chain documents also have a `legend` that
explains their values in words.
[JSON documents](../reference/documents.md) describes each field.

## Options

```text
{{#include ../output/help-schema.txt}}
```
