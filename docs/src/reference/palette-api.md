# Palette API requests

Strata reads. Most requests are `GET` requests. Five are `POST` requests, and
none of them changes anything: four are searches that Palette takes as a
`POST`, and `compare --repave` asks Palette a question. Each request has the
headers `ApiKey` and `Accept: application/json`. A request in a project has
the header `ProjectUid` too. The
[Palette API reference](https://docs.spectrocloud.com/api/v1/) gives the shape
of each response.

## A profile version

| Request | Gives |
| --- | --- |
| `GET /v1/clusterprofiles/{uid}` | The metadata, the consumers, and the published pack references. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` | The registry digests and the manifest contents. |
| `GET /v1/clusterprofiles/{uid}/variables` | The profile variables. |
| `GET /v1/dashboard/clusterprofiles/metadata` | The listing: `list`, and the uid of `<name>@<version>` and of `<name>@latest`. |

A missing variables endpoint, HTTP 404 or 405 on an older Palette, counts as no
variables. The record says so in `source.variablesUnavailable`: see
[JSON documents](./documents.md#strata-snapshot1).

## A cluster

| Request | Gives |
| --- | --- |
| `GET /v1/spectroclusters/{uid}` | `spec.clusterProfileTemplates`: one applied profile each, with its packs. The state, in `status.state`. |
| `GET /v1/spectroclusters/{uid}/profiles/packs/manifests` | The registry digests and manifest contents, one entry for each applied profile. |
| `GET /v1/spectroclusters/{uid}/variables` | The variable definitions and values, one entry for each applied profile. |
| `GET /v1/dashboard/spectroclusters/meta` | The listing: `list clusters`, the uid of a cluster name, and the second reading of `consumers`. A cluster name goes out first as `GET /v1/spectroclusters/<name>`; strata reads the listing after that HTTP 404. |
| `GET /v1/dashboard/spectroclusters/{uid}` | The summary: the health of the cluster, in `status.health.state`. The cluster document does not carry it. No command reads this request. [The knurl adapter](../automation/knurl.md) reads it for each cluster. |
| `GET /v1/notifications/spectrocluster/{uid}` | Whether an update waits for the cluster, every page. A cluster record and `list clusters --applied` read it. |
| `GET /v1/spectroclusters/{uid}/profileUpdates` | `profiles[]`: each profile of the cluster as the update that waits gives it, in the shape of `spec.clusterProfileTemplates`. `cluster:<cluster>@pending` reads it. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` | For `@pending`: the manifests of each profile in `profiles[]`. The cluster holds no copy of them yet. |
| `GET /v1/spectroclusters/{uid}/packs/resolvedValues` | The value that Palette gives each macro, for `snapshot --macros`, and `verify` of a record that holds macro digests. |
| `GET /v1/cloudconfigs/{cloudType}/{configUid}` | The machine pools and the cloud part of the cluster, for `snapshot --shape`, and `verify` of a record that holds the `shape` tier. |
| `POST /v1/spectroclusters/{uid}/validate/repave` | Whether a change of the profiles of the cluster replaces its nodes, for `compare --repave`. The request changes nothing. |

## The fleet

| Request | Gives |
| --- | --- |
| `GET /v1/dashboard/spectroclusters/search/schema` | The properties that the cluster search takes. Strata searches when it names `tags`. |
| `POST /v1/dashboard/spectroclusters/search?limit=50` | The clusters that the filter of the body selects, every page, at tenant scope: each with its labels, its project, and its cloud. `list clusters --tag` and `--filter`, and a `tag:` or `filter:` ref, read it. The request changes nothing. |
| `GET /v1/filters/metadata?filterType=tag` | The name and the uid of each saved tag filter of the tenant. |
| `GET /v1/filters/tag/{uid}` | One saved tag filter: `spec.filterGroup`, its conjunction and its conditions. |

The search takes no filter uid, so strata turns each condition of a saved filter
into a condition of the search: the property `tags`, with the values
`<key>:<value>`, or `<key>` for a label that must be there.

## An edge host, a cluster group, and an app profile

| Request | Gives |
| --- | --- |
| `GET /v1/edgehosts/{uid}` | `spec.clusterProfileTemplates`: the profiles bound to the edge host. |
| `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}` | `spec.published.content`: the copy of a manifest that the edge host holds, by the uid that its template names. |
| `POST /v1/dashboard/edgehosts/search?limit=50` | Every edge host, every page, with a summary of its profiles: the uid of an edge host name, and the edge hosts that `consumers` counts. The body has no filter. |
| `GET /v1/clustergroups/{uid}` | `spec.clusterProfileTemplates`: the profiles that the group pins for its virtual clusters. |
| `GET /v1/clusterprofiles/{uid}/packs/{packName}/manifests/{manifestUid}` | `spec.published.content`: the copy of a manifest that a cluster group holds, by the uid that its template names. |
| `GET /v1/clustergroups/hostCluster/metadata` | The name and the uid of each cluster group: the uid of a group name. |
| `POST /v1/dashboard/clustergroups/{uid}/virtualClusters?limit=50` | The virtual clusters of the group, every page: `members[]` of the record. The body has no filter. |
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
| `GET /v1/registries/metadata` | `packs`, `behind`, a `pack:` ref, `verify` on a pack record, and `snapshot --from-candidate` when a layer names its registry by name | The registries the key can see: name and uid. |
| `GET /v1/packs?filters=spec.name=<name>&limit=100&offset=<n>` | `snapshot --from-candidate`, `packs`, `behind`, a `pack:` ref, and `verify` on a pack record | Each version of the pack in each registry: the pack uid, the layer, the type, the default values, the digest when the registry gives one, and when Palette first listed the version. |
| `GET /v1/packs/<name>/registries/<uid>` | `snapshot --from-candidate`, `packs`, `behind`, a `pack:` ref, and `verify` on a pack record | The tags of the pack in one registry, with its values and its layer. |
| `GET /v1/registries/oci/summary` | `snapshot --from-candidate`, a `pack:` ref, and `verify` on a pack record | The provider type of each OCI registry: `pack`, `helm`, or `zarf`. A record names no provider type when Palette does not answer. |
| `GET /v1/registries/pack/{uid}/sync/status` | `verify` on a record of a pack of type `spectro` | The last sync of the registry: `isSyncSupported`, `lastSyncedTime`, `status`, and `message`. |
| `GET /v1/registries/helm/{uid}/sync/status` | `verify` on a record of a pack of type `helm` | The same, for a Helm registry. |
| `GET /v1/registries/oci/{uid}/basic/sync/status`, then `GET /v1/registries/oci/{uid}/ecr/sync/status` | `verify` on a record of a pack of type `oci` | The same, for an OCI registry. Strata sends the second request only when Palette answers the first with a 4xx: the registry is then an ECR registry. When Palette answers no route, the verdict has no sync note. |
| `GET /v1/audits?resourceKind=<kind>&resourceUid=<uid>&startTime=<time>&limit=100` | `verify --who` | The writes to the object since the record, every page. The kind is `clusterprofile`, `spectrocluster`, or `edgehost`. |

`list --packs`, and `list --pack`, read `GET /v1/clusterprofiles/{uid}` for each
profile version. `list clusters --applied`, and `list --cluster`, read
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
fetch time and the same flag.
