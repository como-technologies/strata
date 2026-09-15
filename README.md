# palette-profile-diff

Diff two Spectro Cloud Palette cluster profile versions and say *what kind* of
change happened, not just that something did. Bash + curl + jq, with a small
embedded Python step (PyYAML) that turns pack values and manifests into
canonical JSON and hashes them.

```
palette-profile-diff my-profile@1.2.0 my-profile@1.3.0
palette-profile-diff 64f0…a1 64f0…b2 --full
palette-profile-diff snapshot my-profile@1.2.0 -o my-profile-1.2.0.json   # the record step
palette-profile-diff verify my-profile-1.2.0.json                          # immutability check
```

Set `PALETTE_API_KEY`, and `PALETTE_PROJECT_UID` for project-scoped profiles.
`PALETTE_API` overrides the base URL for self-hosted Palette.

## Where it sits in the lifecycle model

The lifecycle outline (draft 0.1) treats a profile version as a *release*: a
published definition artifact with a name and a version that is never changed
after publication, with a build/test/sign/publish/record pipeline and a
consumer check before end of life. This tool is the mechanism behind three of
those statements.

| outline                                          | tool                                                                      |
|--------------------------------------------------|---------------------------------------------------------------------------|
| pipeline step **record**                         | `snapshot` writes the published version's Merkle root and canonical contents |
| **a published artifact is immutable; tools enforce this** | `verify` re-fetches the same uid and exits 1 if the recorded root moved |
| **identity: name, version, digest, or all three?** | all three, as separate hashes per pack, so the report says which one moved |
| **consumers and blast radius**                   | consumer count from the profile's `inUseClusterUids`; `impact:` hint per changed layer |
| fact to confirm: **in-place edit to a published profile version** | `snapshot`, edit in the UI, `verify` |

### Terms

Palette terms are used where Palette has one. Three flags are ours; the table
maps them to the nearest Palette concept and to the outline's 12-factor frame.

| our term   | hashed from                                                  | Palette concept                              | 12-factor reading                          |
|------------|--------------------------------------------------------------|----------------------------------------------|--------------------------------------------|
| `IDENTITY` | pack name, layer, type, tag, version, packUid, registryUid    | which pack version the layer references      | the release points at a different build    |
| `UPSTREAM` | the `digest` on the profile's pack ref                        | pack content in the registry                 | the build changed under the same version, which the outline forbids |
| `AUTHORED` | canonical values YAML, attached manifest contents, params     | the pack's values and manifests on the profile | release-level config                       |

Profile variables are hashed separately (`variablesHash`). They are the
per-deploy config interface, not part of any pack.

## What the report means

```
lab-profile@1.2.0 (64f000000000)  ->  lab-profile@1.3.0 (64f000000000)
root       73c75fb7dba6 -> 3f25b888f168   CHANGED
consumers  0 -> 2 clusters

  variables CHANGED
        ~ cluster_env.defaultValue: "dev" -> "staging"
        + db_password.name: "db_password"

  [os   ] ubuntu-aws                 22.04              unchanged
~ [k8s  ] kubernetes                 1.29.4 -> 1.30.2   IDENTITY UPSTREAM AUTHORED
      upstream digest  sha256:k8s12 -> sha256:k8s13   (packRef.digest)
      values: 3 changed, 1 added, 0 removed
        ~ kubeadmconfig.apiServer.extraArgs.audit-log-maxage: "30" -> "90"
        + kubeadmconfig.apiServer.extraArgs.profiling: "false"
      impact: node repave (Kubernetes version change)
~ [cni  ] cni-cilium-oss             1.15.3             UPSTREAM
      upstream digest  sha256:cni-a -> sha256:cni-b   (packRef.digest)
      impact: pack content changed under the same tag, applied on next profile sync
~ [addon] prometheus-operator        55.8.3             AUTHORED
      ~ manifest extra-rules
      + manifest dashboards
      impact: in-place addon upgrade
+ [addon] cert-manager               1.14.0             ADDED
- [addon] external-dns               0.14.0             REMOVED
```

Each pack is a Merkle node: `node = sha256(identity + upstream + authored)`,
and `root = sha256(variablesHash, [key, node] in profile order)`. Two profiles
with the same root are the same release. The report prints `IDENTICAL` and
exits 0; any difference exits 1; errors exit 2.

`impact:` lines are hints derived from the layer and the flags. OS or
Kubernetes identity changes mean a node repave; addon edits are in-place. They
are not a Palette API answer.

## Canonicalization

Palette rewrites values on read: whitespace, key order, expanded
`{{.spectro.var.X}}` templates. Text diffs churn on that; these hashes do not,
because values and manifests are parsed as YAML and re-serialized with sorted
keys before hashing. Values that do not parse as YAML (unquoted Go templates,
for example) are hashed as trimmed text and the report says so. `--full`
prints a unified diff for every changed pack's raw values and for every
changed manifest.

## Snapshots

`snapshot` writes one JSON file per profile with the hashes, the canonical
values, the raw values and manifests, and the consumer list at the time. Keep
it next to the release record, or feed it back to `compare` or `verify` later.
`--raw-dir DIR` also keeps the three raw API responses, and `--from-raw DIR`
rebuilds a snapshot from them offline.

Endpoints used per profile:

- `GET /v1/clusterprofiles/{uid}` for metadata, consumers, and the published pack refs
- `GET /v1/clusterprofiles/{uid}/packs/manifests` for registry digests and manifest contents
- `GET /v1/clusterprofiles/{uid}/variables` for profile variables
- `GET /v1/dashboard/clusterprofiles/metadata` to resolve `name@version`

A profile with no published template falls back to its draft; the snapshot
records which under `source.template`.

## Examples

`examples/fetch-examples.sh` pulls real packs from the Public Repo registry at
several versions, builds three profiles from them, and writes snapshots and
reports under `examples/`. See `examples/README.md` for what is real and what
is synthesized there.

## Tests

`test/run.sh` builds raw-API-shaped fixtures with no network and asserts on
the report, the JSON output, and exit codes.

## Known gaps

- The `digest` Palette stamps on a profile's pack ref has not been checked
  against a live profile to confirm it tracks registry content. The tenant used
  on 2026-09-15 had no cluster profiles to look at. The snapshot records which
  field fed `UPSTREAM` under `hash.upstreamSource`, so this can be confirmed
  from the first real profile. It belongs with the outline's section 14 facts.
- Array diffs are positional. An image inserted in the middle of a list shows
  as one added index plus shifted neighbors rather than one insertion.
- Scalars longer than 120 characters are truncated in the text report;
  `--full` and `--json` carry the whole value.
- Only profiles are supported as refs. A cluster's applied profile templates
  (`spec.clusterProfileTemplates` on the cluster) use the same pack ref shape
  and are the natural next input, giving release-vs-run drift.
- Pack presets and schema are not hashed; a selected preset shows up as a
  values change anyway.
