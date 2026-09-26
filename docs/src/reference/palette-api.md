# Palette API requests

Strata reads. Most requests are `GET` requests. Four are `POST` requests, and
none of them changes anything: three are searches that Palette takes as a
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

## An edge host, a cluster group, and an app profile

| Request | Gives |
| --- | --- |
| `GET /v1/edgehosts/{uid}` | `spec.clusterProfileTemplates`: the profiles bound to the edge host. |
| `POST /v1/dashboard/edgehosts/search?limit=50` | Every edge host, every page, with a summary of its profiles: the uid of an edge host name, and the edge hosts that `consumers` counts. The body has no filter. |
| `GET /v1/clustergroups/{uid}` | `spec.clusterProfileTemplates`: the profiles that the group pins for its virtual clusters. |
| `GET /v1/clustergroups/hostCluster/metadata` | The name and the uid of each cluster group: the uid of a group name. |
| `POST /v1/dashboard/clustergroups/{uid}/virtualClusters?limit=50` | The virtual clusters of the group, every page: `members[]` of the record. The body has no filter. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` and `GET /v1/clusterprofiles/{uid}/variables` | For an edge host and a cluster group: the manifests and the variable definitions of each profile. |
| `GET /v1/appProfiles/{uid}` | The metadata and the version of an app profile. |
| `GET /v1/appProfiles/{uid}/tiers` | Each tier in full: its type, version, source tier, registry, values, properties, and manifests. |
| `GET /v1/appProfiles/{uid}/tiers/{tierUid}/manifests/{manifestUid}` | The content of one manifest of a tier. |
| `GET /v1/dashboard/appProfiles/metadata` | The name, the version, and the uid of each app profile: the uid of `<name>@<version>`. |

What each object holds, and what Palette does to it after an edit in place, is
in [Edge hosts, cluster groups, and app profiles](../internals/palette/other-objects.md).

## Other reads

| Request | Read by | Gives |
| --- | --- | --- |
| `GET /v1/dashboard/projects/metadata` | `list projects`; the name in `PALETTE_PROJECT` or `--project-name`; the warning when `PALETTE_PROJECT_UID` and `PALETTE_PROJECT` name two projects; `list` and `consumers`, to name each project | Every project the key can see: name and uid. |
| `GET /v1/registries/metadata` | `packs`, a `pack:` ref, `verify` on a pack record, and `snapshot --from-candidate` when a layer names its registry by name | The registries the key can see: name and uid. |
| `GET /v1/packs?filters=spec.name=<name>&limit=100&offset=<n>` | `snapshot --from-candidate`, `packs`, a `pack:` ref, and `verify` on a pack record | Each version of the pack in each registry: the pack uid, the layer, the type, the default values, and the digest when the registry gives one. |
| `GET /v1/packs/<name>/registries/<uid>` | `snapshot --from-candidate`, `packs`, a `pack:` ref, and `verify` on a pack record | The tags of the pack in one registry, with its values and its layer. |
| `GET /v1/registries/oci/summary` | `snapshot --from-candidate`, a `pack:` ref, and `verify` on a pack record | The provider type of each OCI registry: `pack`, `helm`, or `zarf`. A record names no provider type when Palette does not answer. |
| `GET /v1/audits?resourceKind=<kind>&resourceUid=<uid>&startTime=<time>&limit=100` | `verify --who` | The writes to the object since the record, every page. The kind is `clusterprofile`, `spectrocluster`, or `edgehost`. |

`list --packs`, and `list --pack`, read `GET /v1/clusterprofiles/{uid}` for each
profile version. `list clusters --applied`, and `list --cluster`, read
`GET /v1/spectroclusters/{uid}` for each cluster. `consumers` reads each cluster
that the profile lists, in its own project. Then it reads the cluster listing at
tenant scope, and `GET /v1/spectroclusters/{uid}` for each cluster the profile
did not name: [consumers](../commands/consumers.md) says why, and
`--list-only` stops it. Then it searches the edge hosts. `verify` on a cluster
record reads each profile that the cluster runs.

Palette serves `/v1/packs` 100 entries at a time. Strata raises `offset` by the
entries it got, until a page is short or `offset` reaches `listmeta.count`.

`--raw-dir` saves the three documents of a profile or a cluster, and
`scope.json`. `scope.json` holds the project, the time of the read, and
`variablesUnavailable`. So `--from-raw` rebuilds the same record, with the same
fetch time and the same flag.
