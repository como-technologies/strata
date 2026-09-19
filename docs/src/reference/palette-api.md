# Palette API requests

Strata sends only `GET` requests. Each request has the header `ApiKey`, and
`ProjectUid` when a project is set.

## A profile version

| Request | Gives |
| --- | --- |
| `GET /v1/clusterprofiles/{uid}` | The metadata, the consumers, and the published pack references. |
| `GET /v1/clusterprofiles/{uid}/packs/manifests` | The registry digests and the manifest contents. |
| `GET /v1/clusterprofiles/{uid}/variables` | The profile variables. |
| `GET /v1/dashboard/clusterprofiles/metadata` | The listing: resolves `<name>@<version>` and `<name>@latest`. |

A missing variables endpoint, HTTP 404 or 405 on an older Palette, counts as no
variables.

## A cluster

| Request | Gives |
| --- | --- |
| `GET /v1/spectroclusters/{uid}` | `spec.clusterProfileTemplates`: one applied profile each, with its packs. The state, in `status.state`. |
| `GET /v1/spectroclusters/{uid}/profiles/packs/manifests` | The registry digests and manifest contents, one entry for each applied profile. |
| `GET /v1/spectroclusters/{uid}/variables` | The variable definitions and values, one entry for each applied profile. |
| `GET /v1/dashboard/spectroclusters/meta` | The listing: resolves a cluster name. |

## Other reads

| Request | Command |
| --- | --- |
| `GET /v1/dashboard/projects/metadata` | `list projects`, and the name in `PALETTE_PROJECT` |
| `GET /v1/registries/metadata` | `snapshot --from-candidate`, `packs`, and a `pack:` ref |
| `GET /v1/packs?filters=spec.name=<name>&limit=100&offset=<n>` | `snapshot --from-candidate`, `packs`, and a `pack:` ref |
| `GET /v1/packs/<name>/registries/<uid>` | `snapshot --from-candidate`, `packs`, and a `pack:` ref |

`list --packs` reads `GET /v1/clusterprofiles/{uid}` for each profile version.
`consumers` reads each cluster in its own project. `verify` on a cluster record
reads each profile that the cluster runs.

`--raw-dir` saves the three documents of a profile or a cluster, and
`scope.json` with the project.
