# Errors

An error exits `2`. Without `--json`, standard error gets one line that starts
with `strata:`. With `--json`, standard error gets one JSON line:

```text
{{#include ../output/auto-error.txt}}
```

`kind` is a stable name. A new release can add a kind. A kind keeps its meaning.
`message` is the sentence that the terminal shows. The other fields belong to
the kind.

| Kind | Cause | Fields |
| --- | --- | --- |
| `missing-credential` | No API key. | `variable`, `alternate` |
| `api` | Palette answered with an error status. | `path`, `status` |
| `retry-after-too-long` | Palette asked for a longer wait than `STRATA_MAX_RETRY_AFTER`. | `path`, `status`, `retryAfter`, `limit` |
| `invalid-setting` | A variable has a value that strata cannot read. | `variable`, `value` |
| `pattern` | A record pattern is not valid, or matches no file. | `pattern` |
| `transport` | The request did not reach Palette. Also: `snapshot` got no ref and no `--from-raw`. | `path` |
| `not-json` | Palette answered with a body that is not JSON. | `path` |
| `profile-not-found` | No profile version with this name and version in the scope. | `name`, `version` |
| `cluster-profile-not-found` | The cluster runs no profile with this name. | `cluster`, `profile`, `applied` |
| `profile-ref-required` | The command needs a profile, and got a cluster. | `reference` |
| `candidate-invalid` | The candidate file has a problem. | `path` |
| `pack-not-found` | The registry has no such pack version or tag. | `name`, `version`, `registry`, `tag` when given |
| `registry-not-found` | No visible registry has this name. | `registry`, `available` |
| `pack-in-several-registries` | A `pack:` ref named no registry, and more than one has the version. | `name`, `version`, `registries` |
| `project-not-found` | No visible project has this name. | `name`, `available` |
| `cluster-not-found` | No cluster in the scope has this name. | `cluster`, `available` |
| `cluster-name-ambiguous` | Two or more clusters in the scope have this name. | `name`, `uids` |
| `not-snapshot` | The file is not a strata snapshot. | `path` |
| `unsupported-schema` | The snapshot has a schema id that this strata does not read. | `path`, `found`, `supported` |
| `snapshot-schema` | The snapshot does not match its schema. | `path`, `schema` |
| `no-uid` | The record has no uid, for example a candidate record. | |
| `io` | Strata cannot read or write the file. | `path` |
| `empty-file` | A file in a `--from-raw` directory is empty. | `path` |
| `file-not-json` | A file in a `--from-raw` directory is not JSON. | `path` |
| `serialize` | Strata could not write a document. | |

A record that is empty or is not JSON is `not-snapshot`.

An error in the command line itself, such as a missing argument, comes from the
argument parser. It exits `2` with its own message, with no `strata:` prefix and
no JSON line, also with `--json`.

`verify` with many files does not stop at a failed record. The record gets an
`ERROR` line, and its entry in the summary has the `error` object.
