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
| `api` | Palette answered with an error status. On a 401 or a 403, the message names the key. A 403 with the code `ProjectUidIsNotEmpty` names the tenant scope in its place: the route takes no project. | `method`, `path`, `status` |
| `retry-after-too-long` | Palette asked for a longer wait than `STRATA_MAX_RETRY_AFTER`. | `method`, `path`, `status`, `retryAfter`, `limit` |
| `invalid-setting` | A variable has a value that strata cannot read. | `variable`, `value` |
| `pattern` | A record pattern is not valid, or matches no file. | `pattern` |
| `transport` | The request did not reach Palette, or the answer did not arrive whole. | `method`, `path` |
| `not-json` | Palette answered with a body that is not JSON. | `method`, `path` |
| `not-archive` | Palette answered the download of a profile version with a file that strata cannot read. The file is no gzip or no tar, or its list of the packs does not parse. See [The download of a version](../internals/palette/profiles.md#the-download-of-a-version). | `method`, `path` |
| `profile-not-found` | No profile version with this name and version in the scope. | `name`, `version`, `tenantScope` |
| `profile-name-ambiguous` | Two or more profile versions in the scope have this name and version: a profile of the tenant and a profile of the project. Give the uid in place of the name. | `name`, `version`, `profiles[]` of `uid` and `scope` |
| `no-draft` | A `<ref>@draft` named a profile version with no draft: Palette holds only its published template. | `name`, `version`, `uid` |
| `cluster-profile-not-found` | The cluster runs no profile with this name or uid. | `cluster`, `profile`, `applied` |
| `profile-ref-required` | The command needs a profile, and got a cluster, an edge host, a cluster group, an app profile, a pack, or a record of one. | `reference` |
| `record-not-a-source` | `snapshot` got a record. It writes one, and does not read one. | `path` |
| `palette-refuses` | `snapshot --validate` asked Palette whether it takes the version, and Palette refused its name, its version, or its packs. Strata prints each error as a `palette    refuses:` line first, and writes no record. | `reference`, `errors[]` of `pack`, `code`, and `message` |
| `usage` | The command line cannot work as given. See [The usage errors](#the-usage-errors). | |
| `stale-record` | The record of the update that waits for a cluster holds a profile that the cluster does not run now. The record is older than the cluster. See [The refusals that a record or a cluster causes](#the-refusals-that-a-record-or-a-cluster-causes). | `cluster`, `name`, `profile` |
| `cluster-template-governs` | A cluster template governs the cluster, and `compare --repave` asks about a change that gives pack values to a profile that the cluster runs. The route of the template takes no pack values. | `cluster`, `template` |
| `no-cloud-config` | The document of the cluster names no cloud config, so `snapshot --shape` has nothing to read. | `cluster` |
| `candidate-invalid` | The candidate file has a problem, or it gives a value that Palette changes at the create. A rule that only the create of Palette checks also gives this kind. Then the message names the code of Palette, and `code` holds it. For the rules of the validate route, `snapshot --validate` asks Palette. See [Checks before any request](candidate-file.md#checks-before-any-request). | `path`, `code` for a rule of the create |
| `pack-not-found` | The registry has no such pack version or tag. `unsynced` names the registries that Palette does not sync, among the ones that strata asked. Palette lists a pack of such a registry only while a profile pins the pack. | `name`, `version`, `registry`, `tag` when given, `unsynced` when it is not empty |
| `pack-name-not-found` | No visible registry offers a pack of that name. `unsynced` names the visible registries that Palette does not sync. | `name`, `unsynced` when it is not empty |
| `pack-unavailable` | Spectro Cloud retired the version: its `system_state` in the listing is `disabled` or `deleted`. Palette does not serve it, and refuses a profile that pins it. A version with the mark `disabled` is not this error: Palette serves it, and `--validate` asks Palette. | `name`, `version`, `registry`, `state` |
| `registry-not-found` | No visible registry has this name. | `registry`, `available` |
| `registry-name-ambiguous` | Two or more visible registries have this name: a registry of the system and a registry of the tenant. Give the uid in place of the name. | `name`, `uids` |
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
| `app-profile-name-ambiguous` | Two or more app profiles in the scope have this name and version. Palette refuses a second one only in one scope. Give the uid in place of the name. | `name`, `version`, `uids` |
| `not-snapshot` | The file is not a strata snapshot. | `path` |
| `unsupported-schema` | The file is a record, or a report for `behind --since`, of a schema id that this strata does not read. | `path`, `found`, `supported` |
| `snapshot-schema` | The record, or the report for `behind --since`, does not match its schema. The message names the field that does not match, such as `variables[0].name` or `source.cluster.profiles[0].version`, and says to take the file again. | `path`, `schema`, `field` unless the document as a whole does not match |
| `log-invalid` | `strata report` read a line of the log of the unknown values that is not a line of `strata-unknown/1`. The line has a field outside the list, a missing field, or a field of the wrong type. Or a field holds a text that the document does not allow, such as the path of a real request. The message gives the number of the line. Strata prints nothing and marks nothing. See [What strata never reports](./report.md#what-strata-never-reports). | `path`, `line` |
| `report-send` | The endpoint did not take the report. Strata could not reach it, the endpoint answered a status outside 2xx, or the report was not valid. A run never fails with this error. It prints the message as a warning, and `--quiet` hides it. The next send holds the lines again. See [Send the report by itself](./report.md#send-the-report-by-itself). | `endpoint` |
| `no-uid` | The record has no uid, for example a candidate record. | |
| `io` | Strata cannot read or write the file. | `path` |
| `empty-file` | A file in a `--from-raw` directory is empty. | `path` |
| `file-not-json` | A file in a `--from-raw` directory is not JSON. | `path` |
| `serialize` | Strata could not write a document. | |

A record that is empty or is not JSON is `not-snapshot`.

The error `snapshot-schema` names the field that does not match. Here a person
edited a record, and the version of a profile in the cluster is `null`:

```text
{{#include ../output/tiers-edited-verify.txt}}
```

## The usage errors

A `usage` error comes before any request in these cases:

- `snapshot` with no ref and no `--from-raw`.
- `snapshot --raw-dir` of a ref that is not a profile or a cluster.
- `snapshot --validate` with `--from-raw`, a cluster ref, or an `@draft` ref.
- `behind --since` with a file that is not a `strata-behind/1` document.
- `behind --pin` with a key that no record pins from a registry.
- `compare --repave` or `compare --validate` with a pair that the refs show to
  be wrong. One side is an edge host, a cluster group, or a set of clusters.
  Or the pair is not a cluster and a profile version. `compare --repave` also
  takes `cluster:<c>` and `cluster:<c>@pending`.
- `verify` of a record of an object that a later release wrote. The record
  shows its kind, so `verify` needs no key.
- `strata-preflight --release` with a text that is not a version, such as
  `4.11.x`. Each rule with `releases` applies to such a release.

A record file shows its side when strata reads the file. That read is no
request, so these errors come before any request too.

One `usage` error can come after a request. `compare --repave cluster:<a>
cluster:<b>@pending` gives it when `a` and `b` name two clusters. A name and a
uid can name one cluster, so only the reads show that they name two.

`snapshot` takes `--json` only with `--dir`. Without `--dir`, a `usage` error
of `snapshot` prints as a `strata:` line.

## The refusals that a record or a cluster causes

The command is correct in these cases, but a record or a cluster stops it.
Each case has a kind of its own, and not `usage`. Each comes after the first
request, because only the document of the cluster shows it:

- `stale-record`: the record of the update that waits for a cluster holds a
  profile that the cluster does not run now. Record the update again with
  `cluster:<c>@pending`.
- `cluster-template-governs`: a cluster template governs the cluster, and
  `compare --repave` asks about a change that gives pack values to a profile
  that the cluster runs. The route of the template takes no pack values. See
  [A cluster in a cluster template](../commands/compare/repave.md#a-cluster-in-a-cluster-template).
- `no-cloud-config`: `snapshot --shape` reads the cloud config of a cluster,
  and the document of the cluster names none.

In each of the three, `cluster` is the uid of the cluster. `stale-record` adds
`name`, the name of the cluster, which its message shows. `no-cloud-config`
holds the name in `cluster` only when the document of the cluster gives no uid.

With `--json`, the fields of the kind are in the error. This is `stale-record`:

```text
{{#include ../output/edited-stale-record-json.txt}}
```

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
| `AwsKeyPairNotFound` | A key pair that the region does not hold, on the property route of AWS. |
| `FeatureNotSupported` | A feature that the cloud account does not support, such as a Pod Identity account on the property route of AWS. |

`POST /v1/spectroclusters/{uid}/validate/packs` refuses the whole request with
HTTP `500` and one of these codes. Strata does not send it again.
`compare --validate` prints the refusal as the answer of Palette, with exit
`0` or `1` from the comparison. See
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

The same route refuses one more case with HTTP `403`, and not `500`. The code is
`PackValuesOverrideForbidden`, for a pack that an integration pack names as a
read-only dependency, with values other than its defaults. `compare --validate`
prints it as the answer of Palette too. Each other `403` is an error of strata.

`POST /v1/clusterprofiles/validate/packs` and
`POST /v1/clusterprofiles/{uid}/validate/packs` refuse the whole request with
HTTP `500` and one of these codes. Strata does not send it again.
`snapshot --validate` prints the refusal as the answer of Palette, with exit
`2` and the error `palette-refuses`. See
[Ask Palette before you publish](../commands/candidate.md#ask-palette-before-you-publish).

| Code | Palette refuses |
| --- | --- |
| `ClusterDashboardPacksMutuallyExclusive` | Two dashboard packs that exclude each other. |
| `PackValuesYamlInvalid` | Pack values that are not YAML. |
| `PackManifestYamlInvalid` | A manifest of a pack that is not YAML. |
| `DisabledPacksForbidden` | A pack that is disabled. |
| `SubResourceLimitValidationFailure` | More than 200 packs, or more than 200 manifests. |
| `ClusterProfilePacksInConflict` | For the route of an update: a pack name that is also in another profile of a cluster that runs the version. |

The API schema of both routes refuses two equal packs with HTTP `422`, and the
code is the number `610`. `snapshot --validate` prints it as the answer of
Palette too.

`behind` reads the refusals of the cluster route and of the route of an update as
the answer too. It prints them under `bump to`, and its exit does not change.
Each other error of a request is a `palette gave no answer` line there, and the
pin stays behind. See
[Palette accepts the move, or refuses it](../commands/behind.md#palette-accepts-the-move-or-refuses-it).

For a candidate, `snapshot --validate` also asks
`GET /v1/clusterprofiles/validate/name`. It refuses a name and a version that
the project holds with HTTP `409` and `ClusterProfileAlreadyExists`. It refuses
the name or the version with HTTP `500` and one of these codes:

| Code | Palette refuses |
| --- | --- |
| `ResourceNameEmpty` | An empty name. |
| `VersionParseFailure` | A version that does not parse as semver. |

It also asks `POST /v1/clusterprofiles/import/validate`. It refuses the whole
request with HTTP `409` and `ClusterProfileAlreadyExists`, or with HTTP `404`
and `PackMacroUndefined`, a macro that Palette does not define. It refuses a
body that the API schema does not take with HTTP `422`. It refuses the whole
request with HTTP `500` and one of these codes:

| Code | Palette refuses |
| --- | --- |
| `ResourceLimitValidationFailure` | A new profile in a tenant that holds the most profiles that its limit takes. |
| `SubResourceLimitValidationFailure` | More than 200 packs, or more than 200 manifests. |
| `UnSupportedCloudType` | A cloud type that Palette does not support. |
| `ClusterDashboardPacksMutuallyExclusive` | Two dashboard packs that exclude each other. |
| `InvalidName` | A manifest, or a manifest pack, with a name that is not a DNS-1123 label of 32 characters at most. |
| `DuplicatePacksForbidden` | One pack name two times in a profile. |
| `DuplicatePackManifestsForbidden` | One manifest name two times in a pack. |
| `MultiPackVersionsForbidden` | Two versions of one pack. |
| `PackValuesYamlInvalid` | Pack values that are not YAML. |
| `PackManifestYamlInvalid` | A manifest of a pack that is not YAML. |
| `PackSpectroIntegrationNotSupported` | An integration pack that the cloud does not support. |
| `ClusterProfileOidcNotConfigured` | An integration pack that needs an OIDC identity provider that no pack or organization gives. |
| `DisabledPacksForbidden` | A pack that is disabled. |
| `ResourceNameEmpty` | An empty name. |
| `VersionParseFailure` | A version that does not parse as semver. |
| `ClusterProfileCreationValidationFailure` | The variables, or the values and the dependencies of a pack. |

When the import validate route finds no registry that holds a pack of the body,
it checks nothing. It gives HTTP `500` with one of these codes. Strata does not
send it again, and the error is `api`, because Palette gave no answer about the
candidate: `ClusterProfileImportRegistriesNotFound`,
`ClusterProfileImportOciRegistriesNotFound`,
`ClusterProfileImportPrivateHelmRegistriesNotFound`, and
`ClusterProfileImportPackNotFound`.

Strata does not copy these checks. A candidate that breaks one of them gives a
record without `--validate`, and this refusal with `--validate`.

`strata preflight` asks `POST /v1/spectroclusters/{cloudType}/validate`. Strata
does not send its refusal again, and `palette-validate` reports it. See
[The validate routes](../internals/palette/validate-routes.md#post-v1spectroclusterscloudtypevalidate).

| Code | Palette refuses |
| --- | --- |
| `ClusterProfilesNotSupported` | A profile of another cloud type than the route, and not of the cloud type `all`. |

A 5xx with a different code, or with no code, is a fault. Strata sends the
request again.

An error in the command line itself, such as a missing argument, comes from the
argument parser. It exits `2` with its own message, with no `strata:` prefix and
no JSON line, also with `--json`.

`verify` with many files does not stop at a failed record. The record gets an
`ERROR` line, and its entry in the summary has the `error` object.
