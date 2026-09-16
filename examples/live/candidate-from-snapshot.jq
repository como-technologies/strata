# A candidate file with the content of a published profile's snapshot: the same packs, tags,
# registries, values, params, manifests, and variables. `strata snapshot --from-candidate` on the
# result must reproduce the snapshot's root.
{
  name: .source.name,
  version: .source.version,
  type: .source.type,
  cloudType: .source.cloudType,
  layers: [
    .packs[] | {
      name, version, tag, layer, registryUid, values, params,
      manifests: [.manifests[] | {name, content}]
    }
  ],
  variables: .variables
}
