# Palette API requests

Strata reads. Most requests are `GET` requests. Ten are `POST` requests, and
none of them changes anything. Four are searches that Palette takes as a
`POST`. With the other six, `compare --repave`, `compare --validate`, and
`snapshot --validate` ask Palette a question. Each request has the
headers `ApiKey` and `Accept: application/json`. A request in a project has
the header `ProjectUid` too. The
[Palette API reference](https://docs.spectrocloud.com/api/v1/) gives the shape
of each response.

## A profile version

| Request | Gives |
| --- | --- |
| `GET /v1/clusterprofiles/{uid}` | The metadata, the consumers, and the published pack references. `metadata.annotations.scope` gives the scope of the version: `project`, `tenant`, or `system`. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` | The registry digests and the manifest contents. |
| `GET /v1/clusterprofiles/{uid}/variables` | The profile variables. |
| `GET /v1/packs/{uid}` | Only when the profile has a draft: for each pack of the draft, `packValues[].schema[]`, with the `name` and the `format` of each value. Strata writes `********` over each value whose format is `password`. See [Secrets in the values](../internals/palette/profiles.md#secrets-in-the-values). |
| `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}` | Only for a version with a draft, once for each manifest of its two templates: `spec.published.content` and `spec.draft.content`. While a draft waits, `packs/manifests` gives the content of neither template. See [Drafts](../internals/palette/profiles.md#drafts). |
| `GET /v1/dashboard/clusterprofiles/metadata` | The listing: `list`, and the uid of `<name>@<version>` and of `<name>@latest`. In a project, it holds the profiles of the project and the profiles of the tenant. |
| `POST /v1/dashboard/clusterprofiles?limit=50` | The profile search, with the filter `scope`: one item for each profile name of that scope, with the uid of each version in `specSummary.versions[]`. `list` reads it for the scope of each version. In a project, it reads the tenant versions and the system versions. At tenant scope, it reads the system versions. Palette gives 50 names on a page, and strata raises `offset` as for `/v1/packs`. The request changes nothing. |
| `POST /v1/clusterprofiles/{uid}/validate/packs` | Whether Palette takes the packs of the version as an update of itself, for `snapshot <ref> --validate`. The request changes nothing. |

A missing variables endpoint, HTTP 404 or 405 on an older Palette, counts as no
variables. The record says so in `source.variablesUnavailable`: see
[JSON documents](./documents.md#strata-snapshot1). Palette also answers HTTP 404
when it does not have the profile. So after a 404, strata reads the profile
document again. When that read answers HTTP 404, the record fails, and `verify`
reports the version as gone. A cluster does the same with its own document. See
[The variables route](../internals/palette/profiles.md#the-variables-route).

## A cluster

| Request | Gives |
| --- | --- |
| `GET /v1/spectroclusters/{uid}` | `spec.clusterProfileTemplates`: one applied profile each, with its packs. The state, in `status.state`. `verify` reads `status.state`, `metadata.isDeleted`, and `metadata.deletionTimestamp`: Palette keeps the document of a deleted cluster. |
| `GET /v1/spectroclusters/{uid}/profiles/packs/manifests` | The registry digests and manifest contents, one entry for each applied profile. |
| `GET /v1/spectroclusters/{uid}/variables` | The variable definitions and values, one entry for each applied profile. |
| `GET /v1/spectroclusters/{uid}/config/rbacs` | `items[]`: the role bindings of the cluster, each with `spec.bindings[]` of `namespace`, `role`, `subjects`, and `type`. A cluster record and `verify` read it for the `policy` tier. After an HTTP 404, `rbacs` holds `null`. After another HTTP error, the tier has no `rbacs`, `errors.rbacs` of the tier holds the error, and strata still writes the record. |
| `GET /v1/spectroclusters/{uid}/config/namespaces` | `items[]`: the namespaces of the cluster, each with `metadata.name`, `spec.isRegex`, and `spec.resourceAllocation`. A cluster record and `verify` read it for the `policy` tier. After an HTTP 404, `namespaces` holds `null`. After another HTTP error, the tier has no `namespaces`, `errors.namespaces` of the tier holds the error, and strata still writes the record. |
| `GET /v1/dashboard/spectroclusters/meta` | The listing: `list clusters`, the uid of a cluster name, and the second reading of `consumers` when the search takes no `clusterProfiles`. A cluster name goes out first as `GET /v1/spectroclusters/<name>`; strata reads the listing after that HTTP 404. |
| `GET /v1/dashboard/spectroclusters/{uid}` | The summary: the health of the cluster, in `status.health.state`. The cluster document does not carry it. No command reads this request. [The knurl adapter](../automation/knurl.md) reads it for each cluster. |
| `GET /v1/notifications/spectrocluster/{uid}` | Whether an update waits for the cluster, every page. A cluster record and `list clusters --applied` read it. |
| `GET /v1/spectroclusters/{uid}/repave/status` | The repave that waits for approval: its `source`, its reasons, and the time of its state. A cluster record reads it only when the cluster document says `status.repave.state: Pending`. After an HTTP error, `pendingRepaveError` holds the error, and strata still writes the record. |
| `GET /v1/spectroclusters/{uid}/profileUpdates` | `profiles[]`: each profile of the cluster as the update that waits gives it, in the shape of `spec.clusterProfileTemplates`. `cluster:<cluster>@pending` reads it. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` | For `@pending`: the manifests of each profile in `profiles[]`. The cluster holds no copy of them yet. |
| `GET /v1/spectroclusters/{uid}/packs/resolvedValues` | The value that Palette gives each macro, for `snapshot --macros`, and `verify` of a record that holds macro digests. |
| `GET /v1/clustergroups/{uid}/packs/resolvedValues` | The value that Palette gives each macro for a cluster group, in the shape of the answer for a cluster: `snapshot clustergroup:<group> --macros`, and `verify` of its record. |
| `GET /v1/clusterprofiles/{uid}/packs/resolvedValues` | The value that Palette gives each macro at the scope of a profile version, with no cluster, in one `resolved` map: `snapshot <name>@<version> --macros`, and `verify` of its record. |
| `GET /v1/cloudconfigs/{cloudType}/{configUid}` | The machine pools and the cloud part of the cluster, for `snapshot --shape`, and `verify` of a record that holds the `shape` tier. |
| `POST /v1/spectroclusters/{uid}/validate/repave` | Whether a change of the profiles of the cluster replaces its nodes, for `compare --repave`: a move to a profile version, or the update that waits. The request changes nothing. |
| `POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave` | The same answer for a cluster template, for `compare --repave` of a cluster that the template governs. The body names the profiles by uid, with `isUpgradeNow: false`. The request changes nothing. |
| `POST /v1/spectroclusters/{uid}/validate/packs` | Whether Palette accepts the packs of a change of the profiles of the cluster, for `compare --validate`. It takes the body of `validate/repave`, and changes nothing. |

## The fleet

| Request | Gives |
| --- | --- |
| `GET /v1/dashboard/spectroclusters/search/schema` | The properties that the cluster search takes. `list` searches when it names `tags`, and the second reading of `consumers` when it names `clusterProfiles`. `list clusters`, `cluster:<name>`, and the second reading of `consumers` ask the search for the virtual clusters when it names `environment`. |
| `POST /v1/dashboard/spectroclusters/search?limit=50` | The clusters that the body selects, every page, in the scope of the request: each with its labels, project, cloud, and profiles. The search gives a virtual cluster only when a condition asks for the environment `nested`, with its cluster group and its host cluster in `status.virtual`. `list clusters --tag` and `--filter`, and a `tag:` or `filter:` ref, send it at tenant scope, and again with that condition added. `list clusters` and `cluster:<name>` send it with that one condition, in their scope. The second reading of `consumers` sends `clusterProfiles eq <profile name>` in the scope of the profile, and again with that condition added, when the search takes `environment`. `clusterProfiles` takes a name, and not a uid. |
| `GET /v1/filters/metadata?filterType=tag` | The name and the uid of each saved tag filter of the tenant. |
| `GET /v1/filters/tag/{uid}` | One saved tag filter: `spec.filterGroup`, its conjunction and its conditions. |

The search takes no filter uid, so strata turns each condition of a saved filter
into a condition of the search: the property `tags`, with the values
`<key>:<value>`, or `<key>` for a label that must be there.

## An edge host, a cluster group, and an app profile

| Request | Gives |
| --- | --- |
| `GET /v1/edgehosts/{uid}` | `spec.clusterProfileTemplates`: the profiles bound to the edge host. `verify` reads the state in `status.state`, and takes a 404 as a host that is gone. |
| `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}` | `spec.published.content`: the copy of a manifest that the edge host holds, by the uid that its template names. |
| `POST /v1/dashboard/edgehosts/search?limit=50` | Every edge host, every page, with a summary of its profiles: the uid of an edge host name, and the edge hosts that `consumers` counts. The body has no filter. |
| `GET /v1/clustergroups/{uid}` | `spec.clusterProfileTemplates`: the profiles that the group pins for its virtual clusters. `spec.clustersConfig`: the config that the group gives each virtual cluster, for the `policy` tier. `verify` reads `status.isActive`, and takes a 404 as a group that is gone. The second reading of `consumers` reads it for each group, to find the groups that pin the version. |
| `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}` | `spec.published.content`: the copy of a manifest that a cluster group holds, by the uid that its template names. |
| `GET /v1/clustergroups/hostCluster/metadata` | The name and the uid of each cluster group: the uid of a group name, and for the second reading of `consumers`, the groups to read, at tenant scope and at the scope of the key. |
| `POST /v1/dashboard/clustergroups/{uid}/virtualClusters?limit=50` | The virtual clusters of the group, every page: `members[]` of the record, and for the second reading of `consumers`, the virtual clusters of each group that pins the version. The body has no filter. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` and `GET /v1/clusterprofiles/{uid}/variables` | For an edge host and a cluster group: the manifests and the variable definitions of each profile. |
| `GET /v1/appProfiles/{uid}` | The metadata and the version of an app profile, and `status.inUseApps`: the app deployments that `consumers` lists. |
| `GET /v1/appDeployments/{uid}` | One app deployment, for `consumers` of an app profile: its state, and the version its copy names. |
| `GET /v1/appProfiles/{uid}/tiers` | Each tier in full: its type, version, source tier, registry, values, properties, and manifests. |
| `GET /v1/appProfiles/{uid}/tiers/{tierUid}/manifests/{manifestUid}` | The content of one manifest of a tier. |
| `GET /v1/dashboard/appProfiles/metadata` | The name, the version, and the uid of each app profile: the uid of `<name>@<version>`. |

What each object holds, and what Palette does to it after an edit in place, is
in [Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md).

## Other reads

| Request | Read by | Gives |
| --- | --- | --- |
| `GET /v1/dashboard/projects/metadata` | `list projects`; the name in `PALETTE_PROJECT` or `--project-name`; the warning when `PALETTE_PROJECT_UID` and `PALETTE_PROJECT` name two projects; `list` and `consumers`, to name each project | Every project the key can see: name and uid. |
| `GET /v1/registries/metadata` | `packs`, `behind`, a `pack:` ref, and `verify` on a pack record. `snapshot --from-candidate` when a layer names its registry by name, or when the listing does not have the version of a layer. `snapshot --from-candidate --validate`, for the registry of each pack in the body of the import validate route. | The registries the key can see: name and uid. Also `kind` and `isPrivate`: Palette does not sync a Helm registry that is private. |
| `GET /v1/packs?filters=spec.name=<name>&limit=100&offset=<n>` | `snapshot --from-candidate` for each layer that is not a `manifest` layer, `packs`, `behind`, a `pack:` ref, and `verify` on a pack record | Each version of the pack in each registry: the pack uid, the layer, the type, the default values, the digest when the registry gives one, when Palette first listed the version, and the architectures that it supports. |
| `GET /v1/packs/<name>/registries/<uid>` | `snapshot --from-candidate` for each layer that is not a `manifest` layer, `packs`, `behind`, a `pack:` ref, and `verify` on a pack record | The tags of the pack in one registry, with its values and its layer. |
| `GET /v1/packs/<name>/registries/<uid>?states=deprecated,disabled,deleted` | `packs` and `behind`, when the listing shows a `disabled` or a `deleted` version in the registry | The tags of the retired versions only. The request with no query does not include the disabled and the deleted versions. See [Pack states](../internals/palette/packs.md#pack-states). |
| `POST /v1/clusterprofiles/validate/packs` | `snapshot --from-candidate --validate` | Whether Palette takes the packs of the candidate in a new profile. The request changes nothing. See [The validate routes](../internals/palette/validate-routes.md#post-v1clusterprofilesvalidatepacks). |
| `GET /v1/clusterprofiles/validate/name?name=<name>&version=<version>` | `snapshot --from-candidate --validate` | Whether Palette takes the name and the version of the candidate for a new profile: `204` with no body, or a refusal. See [The validate routes](../internals/palette/validate-routes.md#get-v1clusterprofilesvalidatename). |
| `POST /v1/clusterprofiles/import/validate` | `snapshot --from-candidate --validate` | Whether Palette takes the candidate as an import, in the shape of a profile export: the names of the packs and the manifests, and a repeat of each. The request changes nothing. See [The validate routes](../internals/palette/validate-routes.md#post-v1clusterprofilesimportvalidate). |
| `GET /v1/registries/oci/summary` | `snapshot --from-candidate`, a `pack:` ref, `verify` on a pack record, and `behind` | The provider type of each OCI registry: `pack`, `helm`, or `zarf`. A record names no provider type when Palette answers the listing with a 4xx. Also `status.sync.isSyncSupported`, which `snapshot --from-candidate` reads when the listing does not have the version of a layer from an OCI registry. For `verify` on a record of a pack of type `oci`, and for `behind` on a pin of such a pack that is current: `status.sync`, the last sync of the registry. |
| `GET /v1/registries/pack/summary` | `verify` on a record of a pack of type `spectro`, and `behind` on a pin of such a pack that is current | `status.sync` of each pack registry, the last sync: `isSyncSupported`, `lastSyncedTime`, `lastRunTime`, `status`, `message`, and `syncRecovery`. See [The sync of a registry](../internals/palette/packs.md#the-sync-of-a-registry). |
| `GET /v1/registries/helm/summary` | `verify` on a record of a pack of type `helm`, and `behind` on a pin of such a pack that is current | The same, for each Helm registry. When Palette answers a summary listing with a 4xx, or the listing does not have the registry, the verdict and the pin have no sync note. When Palette answers with a 5xx, or the request does not complete, the verdict and the pin have no sync note, and `verify.registrySyncError` or `pins[].registrySyncError` gives the reason. |
| `GET /v1/audits?resourceKind=<kind>&resourceUid=<uid>&startTime=<time>&limit=100` | `verify --who` | The writes to the object since the record, every page. The kind is `clusterprofile`, `appprofile`, `edgehost`, or `clustergroup`. For a cluster record, strata sends no `resourceKind`: the uid alone finds the entries of `spectrocluster` and of `virtualCluster`. Strata also reads the log of each profile of `profilesOverwritten`, as `clusterprofile`. A page has 100 entries, the most that Palette serves. |

`list --packs`, and `list --pack`, read `GET /v1/clusterprofiles/{uid}` for each
profile version. Without them, `list` reads the profile search for the scope
of each version. `list clusters --applied`, and `list --cluster`, read
`GET /v1/spectroclusters/{uid}` for each cluster. `consumers` reads each cluster
that the profile lists, in its own project. Then it reads the cluster listing at
tenant scope, and `GET /v1/spectroclusters/{uid}` for each cluster the profile
did not name: [consumers](../commands/consumers.md) says why, and
`--list-only` stops it. Then it searches the edge hosts. For an app profile,
`consumers` reads `GET /v1/appDeployments/{uid}` for each app deployment that
the app profile lists, in its own project, and nothing more. `verify` on a
cluster record reads each profile that the cluster runs.

Palette serves `/v1/packs` 100 entries at a time. Strata raises `offset` by the
entries it got, until a page is short or `offset` reaches `listmeta.count`.

`--raw-dir` saves the three documents of a profile or a cluster, and
`scope.json`. `scope.json` holds the project, the time of the read, and
`variablesUnavailable`. So `--from-raw` rebuilds the same record, with the same
fetch time and the same flag. For a cluster, it also saves the notifications,
with the values of each pack masked, and the role bindings and the namespaces:
see
[The saved answers of Palette](../commands/snapshot/no-network.md#the-saved-answers-of-palette).
