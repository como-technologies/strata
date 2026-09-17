# The offline examples

`examples/` holds real packs from the **Public Repo** registry at several
versions. `just examples` builds cluster profiles and a cluster from them, with
no tenant, so strata has real releases to record, compare, and check.

```bash
just examples          # build from the tracked registry data, no API key
just examples-fetch    # fetch the registry data again, needs PALETTE_API_KEY
```

Only the fetched pack data and the script are tracked. CI runs `just examples`.

| Path | Kind | Content |
| --- | --- | --- |
| `fetch-examples.sh` | tracked | Fetches the packs with GET only, builds the profiles, and runs strata. |
| `packs/<name>/_entity.json` | tracked | `GET /v1/packs/<name>/registries/<publicRepoUid>`, with the readme removed. |
| `packs/<name>/_summaries.json` | tracked | `GET /v1/packs?filters=spec.name=<name>`, only the Public Repo. |
| `packs/<name>/<version>.json` | built | One real pack version: values, digest, pack uid, schema, template, dependencies. |
| `profiles/raw/<profile>@<version>/` | built | `profile.json`, `manifests.json`, `variables.json` in the shapes that strata reads. |
| `profiles/<profile>@<version>.json` | built | The snapshots. |
| `clusters/raw/<cluster>/` | built | `cluster.json`, `manifests.json`, `variables.json`. |
| `clusters/<cluster>.json` | built | The cluster snapshots. |
| `reports/` | built | `compare` output: text reports and one `strata-diff/1` document. |
| `INDEX.md` | built | What was fetched: versions, digests, reports. |
| `live/` | tracked | [The live example](./live-example.md). |
| `library/` | tracked | [The Rust library](../automation/library.md) example. |

## What is real

- **Real:** each file under `packs/`. The values, digests, pack uids, tags,
  schema, and dependencies come from the registry through the Palette API.
- **Built:** the profiles and the cluster. Each profile is the
  `V1ClusterProfile` shape around real packs, so `--from-raw` reads it as it
  reads an API response. The pack-ref digest is a copy of the registry digest.
  The live example showed that Palette puts the same value there: see
  [What Palette does](../internals/palette.md#the-pack-ref-digest).

## The profiles

| Profile | Content | Shows |
| --- | --- | --- |
| `aws-stack@1.0.0` | OS, Kubernetes, CNI, and CSI at the oldest fetched version; the add-on at the newest | the baseline |
| `aws-stack@1.1.0` | the same packs, with the infrastructure layers at the newest fetched version | `IDENTITY` and `UPSTREAM` changes, and diffs between real default values |
| `aws-stack@1.1.1` | 1.1.0 with one attached manifest and one profile variable | `AUTHORED` only, with no pack change |

The cluster `workload-a` runs `aws-stack@1.1.0`, so its snapshot has the root of
the 1.1.0 record.

Between 1.0.0 and 1.1.0, the OS pack moves from 20.04 to 24.04 with `IDENTITY
UPSTREAM` but no `AUTHORED`: the values of the two versions differ only in one
comment line, and the canonical form ignores it. The Cilium pack moves from
1.19.6 to 1.20.1 with many changed values: a chart upgrade, value by value.

## What was fetched

The newest three versions of each pack in the Public Repo, with values:

| Pack | Layer | Versions |
| --- | --- | --- |
| `ubuntu-aws` | os | 20.04, 22.04, 24.04 |
| `kubernetes` | k8s | 1.35.3, 1.35.6, 1.36.2 |
| `cni-cilium-oss` | cni | 1.19.6, 1.20.0, 1.20.1 |
| `csi-aws-ebs` | csi | 1.63.1, 1.64.0, 1.65.0 |
| `cert-manager` | addon | 1.20.3, 1.21.0, 1.21.1 |

`metrics-server` is in the default pack list, but not in the Public Repo, so the
script skips it.

## Fetch again

```bash
export PALETTE_API_KEY=...            # or SPECTROCLOUD_APIKEY, or ~/.palette.env
export PALETTE_PROJECT_UID=...        # optional
just examples-fetch
```

| Variable | Default | Function |
| --- | --- | --- |
| `PACKS` | the five packs and `metrics-server` | The packs to fetch. |
| `VERSIONS_PER_PACK` | `3` | The newest versions of each pack. |
| `CLOUD` | `aws` | Keep the versions for this cloud, or `all`. |
| `EXAMPLES_DIR` | `examples/` | The output directory. |
| `OFFLINE` | not set | `1` builds from the tracked files, with no API calls. |

The tracked files keep full values only for the kept versions. So an offline
build can use the same number of versions for each pack, or fewer. More needs a
fetch. The pack listing takes at most 100 entries for each request, and the
script reads the pages by offset.

The book uses the same data: see
[Write the documentation](./documentation.md#add-an-output).
