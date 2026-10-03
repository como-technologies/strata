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
| `no-draft` | A `<ref>@draft` named a profile version with no draft: Palette holds only its published template. | `name`, `version`, `uid` |
| `cluster-profile-not-found` | The cluster runs no profile with this name or uid. | `cluster`, `profile`, `applied` |
| `profile-ref-required` | The command needs a profile, and got a cluster, an edge host, a cluster group, an app profile, a pack, or a record of one. | `reference` |
| `record-not-a-source` | `snapshot` got a record. It writes one, and does not read one. | `path` |
| `palette-refuses` | `snapshot --validate` asked Palette whether it takes the packs, and Palette refused them. Strata prints each error as a `palette    refuses:` line first, and writes no record. | `reference`, `errors[]` of `pack`, `code`, and `message` |
| `usage` | The command line cannot work as given, so strata sent no request, or only read the cluster document for `compare --repave`. Examples: `snapshot` with no ref and no `--from-raw`, `snapshot --raw-dir` of a ref that is not a profile or a cluster, `compare --repave` or `compare --validate` with an edge host or a cluster group, `compare --repave` with two clusters other than `cluster:<c>` and `cluster:<c>@pending`, `snapshot --validate` with `--from-raw`, a cluster ref, or an `@draft` ref, and `behind --since` with a file that is not a `strata-behind/1` document. `compare --repave` also gives this kind after it reads the cluster document. The causes are a stale `@pending` record, and pack values to a cluster in a cluster template. `snapshot` takes `--json` only with `--dir`. Without `--dir`, this kind prints as a `strata:` line. | |
| `candidate-invalid` | The candidate file has a problem, or it gives a value that Palette changes at the create. A rule that only the create of Palette checks also gives this kind, and the message names the code of Palette. For the rules of the validate route, `snapshot --validate` asks Palette. See [Checks before any request](candidate-file.md#checks-before-any-request). | `path` |
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

## Refusals that strata does not send again

Strata sends a request again after HTTP 5xx (see
[Settings](./settings.md)). Palette also answers some refusals with HTTP `500`.
If the request does not change, the refusal does not change. A second request
gets the same refusal. When the body of a 5xx has one of these codes in `code`, strata does
not send the request again. The error is `api`, with the status that Palette
sent.

| Code | Palette refuses |
| --- | --- |
| `DeletionResourceInUseError` | The delete of an object that another object uses. |
| `ResOperationForbidden` | An operation on this object, for this key. |
| `OperationForbidden` | An operation, for this key. |
| `ClusterProfileCreationValidationFailure` | A new profile that is not valid. |
| `ClusterProfileUpdateValidationFailure` | A change to a profile that is not valid. |
| `ClusterProfileInvalidPackState` | A profile that pins a pack version that is disabled or deleted. |
| `InvalidLimit` | A listing with a `limit` above the maximum. |

`POST /v1/spectroclusters/{uid}/validate/packs` refuses the whole request with
HTTP `500` and one of these codes. Strata does not send it again, and
`compare --validate` prints the refusal as the answer of Palette, with exit
`0` or `1` from the comparison:
[Ask Palette if it accepts the packs](../commands/compare/validate.md).

| Code | Palette refuses |
| --- | --- |
| `PackValuesYamlInvalid` | Pack values that are not YAML. |
| `PackManifestYamlInvalid` | A manifest of a pack that is not YAML. |
| `ClusterDashboardPacksMutuallyExclusive` | Two dashboard packs that exclude each other. |
| `InvalidName` | A pack or a manifest with a name that is not valid. |
| `DuplicatePacksForbidden` | One pack two times in a profile. |
| `DuplicatePackManifestsForbidden` | One manifest two times in a pack. |
| `DuplicateClusterPacksForbidden` | One pack in two profiles of the cluster. |
| `MultiPackVersionsForbidden` | Two versions of one pack across the profiles. |
| `PackSpectroIntegrationNotSupported` | An integration pack that the cloud, or an imported cluster, does not support. |
| `ClusterProfileOidcNotConfigured` | An integration pack that needs an OIDC identity provider, when neither the Kubernetes pack nor the organization gives one. |
| `DisabledPacksForbidden` | A pack that is disabled. |

The same route refuses one more case with HTTP `403`, and not `500`:
`PackValuesOverrideForbidden`, a pack that an integration pack names as a
read-only dependency, with values other than its defaults. `compare --validate`
prints it as the answer of Palette too. Each other `403` is an error of strata.

`POST /v1/clusterprofiles/validate/packs` and
`POST /v1/clusterprofiles/{uid}/validate/packs` refuse the whole request with
HTTP `500` and one of these codes. Strata does not send it again, and
`snapshot --validate` prints the refusal as the answer of Palette, with exit
`2` and the error `palette-refuses`:
[Ask Palette before you publish](../commands/candidate.md#ask-palette-before-you-publish).

| Code | Palette refuses |
| --- | --- |
| `ClusterDashboardPacksMutuallyExclusive` | Two dashboard packs that exclude each other. |
| `PackValuesYamlInvalid` | Pack values that are not YAML. |
| `PackManifestYamlInvalid` | A manifest of a pack that is not YAML. |
| `DisabledPacksForbidden` | A pack that is disabled. |
| `SubResourceLimitValidationFailure` | More than 200 packs, or more than 200 manifests. |
| `ClusterProfilePacksInConflict` | For the route of an update: a pack name that is also in another profile of a cluster that runs the version. |

Strata does not copy these checks. A candidate that breaks one of them gives a
record without `--validate`, and this refusal with `--validate`.

A 5xx with a different code, or with no code, is a fault. Strata sends the
request again.

An error in the command line itself, such as a missing argument, comes from the
argument parser. It exits `2` with its own message, with no `strata:` prefix and
no JSON line, also with `--json`.

`verify` with many files does not stop at a failed record. The record gets an
`ERROR` line, and its entry in the summary has the `error` object.
