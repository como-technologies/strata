# Errors

An error exits `2`. Without `--json`, standard error gets one line that starts
with `strata:`. With `--json --quiet`, standard error gets one JSON line. Use
`--quiet`, or `STRATA_QUIET`, so that no progress note shares the stream:

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
| `transport` | The request did not reach Palette, or the answer did not arrive whole. | `path` |
| `not-json` | Palette answered with a body that is not JSON. | `path` |
| `profile-not-found` | No profile version with this name and version in the scope. | `name`, `version`, `tenantScope` |
| `cluster-profile-not-found` | The cluster runs no profile with this name or uid. | `cluster`, `profile`, `applied` |
| `profile-ref-required` | The command needs a profile, and got a cluster, an edge host, a cluster group, an app profile, a pack, or a record of one. | `reference` |
| `record-not-a-source` | `snapshot` got a record. It writes one, and does not read one. | `path` |
| `usage` | The command line cannot work as given, so strata sent no request. Examples: `snapshot` with no ref and no `--from-raw`, `snapshot --raw-dir` of a ref that is not a profile or a cluster, and `compare --repave` with an edge host or a cluster group, and `behind --since`
with a file that is not a `strata-behind/1` document. `snapshot` takes no `--json`, so this kind prints as a `strata:` line. | |
| `candidate-invalid` | The candidate file has a problem. | `path` |
| `pack-not-found` | The registry has no such pack version or tag. | `name`, `version`, `registry`, `tag` when given |
| `pack-name-not-found` | No visible registry offers a pack of that name. | `name` |
| `pack-unavailable` | The registry lists the version as `disabled` or `deleted`. Palette does not serve it, and refuses a profile that pins it. | `name`, `version`, `registry`, `state` |
| `registry-not-found` | No visible registry has this name. | `registry`, `available` |
| `pack-in-several-registries` | A `pack:` ref named no registry, and more than one has the version. | `name`, `version`, `registries` |
| `project-not-found` | No visible project has this name. | `name`, `available` |
| `cluster-not-found` | No cluster in the scope has this name. | `cluster`, `available` |
| `cluster-name-ambiguous` | Two or more clusters in the scope have this name. | `name`, `uids` |
| `edge-host-not-found` | No edge host in the scope has this uid or name. | `edgeHost`, `available` |
| `edge-host-name-ambiguous` | Two or more edge hosts in the scope have this name. | `name`, `uids` |
| `cluster-group-not-found` | No cluster group in the scope has this uid or name. | `clusterGroup`, `available` |
| `cluster-group-name-ambiguous` | Two or more cluster groups in the scope have this name. | `name`, `uids` |
| `filter-not-found` | No saved tag filter of the tenant has this name or uid. | `filter`, `available` |
| `filter-name-ambiguous` | Two or more saved tag filters have this name. | `name`, `uids` |
| `app-profile-not-found` | No app profile in the scope has this name and version. | `name`, `version`, `available` |
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
