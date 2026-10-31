//! The tests of the candidate file.

use serde_json::json;

use super::*;
use crate::api::codes::CreateRule;
use crate::api::{MemoryApi, Palette};
use crate::cloud::{CLOUD_ALL, CLOUD_CUSTOM};
use crate::term::progress::Progress;

/// The profile `platform-addons` as the export of Palette writes it: a pack from the Public
/// Repo, and a manifest pack with an empty registry and no values.
const EXPORT: &str = r#"metadata:
  name: platform-addons
  description: ""
  labels: {}
spec:
  version: 1.0.0
  template:
    type: add-on
    cloudType: all
    packs:
    - name: cert-manager
      type: spectro
      layer: addon
      version: 1.21.1
      tag: 1.21.1
      values: |-
        pack:
          namespace: cert-manager
      registry:
        metadata:
          uid: 5eecc89d0b150045ae661cef
          name: Public Repo
          kind: pack
          isPrivate: false
          providerType: ""
          isSyncSupported: true
    - name: namespace-policy
      type: manifest
      layer: addon
      version: 1.0.0
      tag: 1.0.0
      registry:
        metadata:
          uid: ""
          name: ""
          kind: ""
          isPrivate: false
          providerType: ""
          isSyncSupported: false
      manifests:
      - name: namespace-policy
        content: |-
          kind: Namespace
  variables:
  - name: owner
    displayName: Owner
    format: string
    required: true
"#;

/// The candidate file of the same content as [`EXPORT`].
const SAME_CANDIDATE: &str = r#"name: platform-addons
version: 1.0.0
type: add-on
cloudType: all
layers:
- name: cert-manager
  version: 1.21.1
  layer: addon
  tag: 1.21.1
  registryUid: 5eecc89d0b150045ae661cef
  values: |-
    pack:
      namespace: cert-manager
- name: namespace-policy
  version: 1.0.0
  layer: addon
  type: manifest
  tag: 1.0.0
  values: ''
  manifests:
  - name: namespace-policy
    content: |-
      kind: Namespace
variables:
- name: owner
  displayName: Owner
  format: string
  required: true
"#;

#[test]
fn an_export_of_palette_with_a_manifest_pack_loads_as_the_candidate_file_of_the_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let (export, file) = (dir.path().join("export.yaml"), dir.path().join("c.yaml"));
    std::fs::write(&export, EXPORT).unwrap();
    std::fs::write(&file, SAME_CANDIDATE).unwrap();
    let (from_export, from_file) = (load(&export).unwrap(), load(&file).unwrap());
    assert_eq!(from_export.candidate, from_file.candidate);
    assert_eq!(from_export.layers, from_file.layers);
    assert_eq!(
        from_export.layers[1].registry, None,
        "a manifest pack has no registry"
    );
    // the same file as JSON, as the export gives it with format=json
    let doc: Value = serde_yaml::from_str(EXPORT).unwrap();
    let json_export = dir.path().join("export.json");
    std::fs::write(&json_export, serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(load(&json_export).unwrap().candidate, from_file.candidate);
}

#[test]
fn an_export_whose_pack_has_no_registry_is_candidate_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let export = dir.path().join("export.yaml");
    std::fs::write(
        &export,
        EXPORT
            .replace("uid: 5eecc89d0b150045ae661cef", "uid: \"\"")
            .replace("name: Public Repo", "name: \"\""),
    )
    .unwrap();
    match load(&export).unwrap_err() {
        Error::CandidateInvalid { message, .. } => assert_eq!(
            message,
            "layer 1 (cert-manager) has no registry; set registry or registryUid on the layer \
                 or at the top"
        ),
        other => panic!("not candidate-invalid: {other}"),
    }
}

/// A candidate with one `manifest` layer, as `strata candidate` writes it for a manifest pack.
fn manifest_candidate() -> Candidate {
    serde_yaml::from_str(
        "name: platform-addons\nversion: 1.0.0\ntype: add-on\ncloudType: all\n\
             layers:\n- name: namespace-policy\n  version: 1.0.0\n  type: manifest\n  \
             manifests:\n  - name: namespace-policy\n    content: 'kind: Namespace'\n",
    )
    .unwrap()
}

fn refusal(c: Candidate) -> String {
    match check(c, Path::new("c.yaml")).unwrap_err() {
        Error::CandidateInvalid {
            rule: None,
            message,
            ..
        } => message,
        other => panic!("not a problem of the format: {other}"),
    }
}

#[test]
fn a_manifest_layer_needs_no_registry_and_resolves_with_no_request() {
    // the registry at the top is for the packs from a registry, not for a manifest layer
    let mut c = manifest_candidate();
    c.registry = Some("Public Repo".into());
    let loaded = check(c, Path::new("c.yaml")).unwrap();
    // the API knows no path: each request fails
    let raw = resolve(
        &loaded,
        &Palette::new(MemoryApi::new()),
        &Progress::silent(),
    )
    .unwrap()
    .raw;
    let pack = &raw.profile["spec"]["published"]["packs"][0];
    assert_eq!(pack["type"], "manifest");
    assert_eq!(
        pack["layer"], "addon",
        "the layer of Palette's manifest pack"
    );
    assert_eq!(pack["tag"], "1.0.0", "the tag defaults to the version");
    assert_eq!(pack["packUid"], "", "Palette mints the uid at the create");
    assert_eq!(pack["registryUid"], "");
    assert!(pack.get("digest").is_none());
    let entry = &raw.manifests["spec"]["packs"][0]["spec"];
    assert_eq!(entry["manifests"][0]["content"], "kind: Namespace");
}

#[test]
fn a_manifest_layer_that_names_a_registry_is_candidate_invalid() {
    let mut by_name = manifest_candidate();
    by_name.layers[0].registry = Some("Public Repo".into());
    let mut by_uid = manifest_candidate();
    by_uid.layers[0].registry_uid = Some("r1".into());
    for c in [by_name, by_uid] {
        let message = refusal(c);
        assert!(
            message.contains("layer 1 (namespace-policy)") && message.contains("names a registry"),
            "{message}"
        );
    }
}

#[test]
fn a_manifest_layer_with_no_manifest_or_another_type_is_candidate_invalid() {
    let mut empty = manifest_candidate();
    empty.layers[0].manifests.clear();
    let message = refusal(empty);
    assert!(
        message.contains("layer 1 (namespace-policy)") && message.contains("no manifest"),
        "{message}"
    );
    // a type that is not manifest stops at the read of the file
    let yaml = to_yaml(&manifest_candidate())
        .unwrap()
        .replace("type: manifest", "type: helm");
    let message = serde_yaml::from_str::<Candidate>(&yaml)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("helm is not a type that a layer gives")
            && message.contains("only the type manifest"),
        "{message}"
    );
}

#[test]
fn a_manifest_layer_takes_only_the_layer_that_palette_gives_a_manifest_pack() {
    let mut k8s = manifest_candidate();
    k8s.layers[0].layer = Some("k8s".into());
    let message = refusal(k8s);
    assert!(
        message.contains("layer 1 (namespace-policy) is a manifest layer with layer k8s"),
        "{message}"
    );
    // the layer that Palette gives, written out, is the same as no layer
    let mut addon = manifest_candidate();
    addon.layers[0].layer = Some("addon".into());
    assert!(check(addon, Path::new("c.yaml")).is_ok());
}

/// A cluster candidate that Palette takes, with each layer given: the core layers and one
/// add-on with one manifest.
fn stack() -> Candidate {
    serde_yaml::from_str(
        "name: aws-stack\nversion: 1.1.1\ncloudType: aws\nregistryUid: r1\nlayers:\n\
             - {name: ubuntu-aws, version: '24.04', layer: os}\n\
             - {name: kubernetes, version: 1.33.6, layer: k8s, values: 'a: 1'}\n\
             - {name: cni-cilium-oss, version: 1.18.4, layer: cni}\n\
             - {name: csi-aws-ebs, version: 1.48.0, layer: csi}\n\
             - name: cert-manager\n  version: 1.19.1\n  layer: addon\n  manifests:\n  \
             - {name: namespace-labels, content: 'kind: Namespace'}\n",
    )
    .unwrap()
}

/// The refusal of `c` for `rule`, a rule that only the create of Palette checks: its
/// message.
fn refused(c: Candidate, rule: CreateRule) -> String {
    match check(c, Path::new("c.yaml")).unwrap_err() {
        Error::CandidateInvalid {
            rule: found,
            message,
            ..
        } => {
            assert_eq!(found, Some(rule), "{message}");
            message
        }
        other => panic!("not candidate-invalid: {other}"),
    }
}

#[test]
fn the_stack_is_a_candidate_that_palette_takes() {
    assert!(check(stack(), Path::new("c.yaml")).is_ok());
}

#[test]
fn a_candidate_that_a_route_of_palette_checks_before_the_create_passes_the_check() {
    // `--validate` asks the name route and the import validate route for these rules, so
    // strata does not copy them. The rules are the version, a pack twice, a manifest name
    // twice in one pack, and a name that is not a DNS label
    let mut version = stack();
    version.version = "2.2.2.develop".into();
    let mut pack_twice = stack();
    pack_twice.layers.push(pack_twice.layers[4].clone());
    let mut manifest_twice = stack();
    let again = manifest_twice.layers[4].manifests[0].clone();
    manifest_twice.layers[4].manifests.push(again);
    let mut manifest_name = stack();
    manifest_name.layers[4].manifests[0].name = "Namespace_Labels".into();
    let mut pack_name = manifest_candidate();
    pack_name.layers[0].name = "a".repeat(33);
    for c in [
        version,
        pack_twice,
        manifest_twice,
        manifest_name,
        pack_name,
    ] {
        assert!(check(c.clone(), Path::new("c.yaml")).is_ok(), "{c:?}");
    }
}

#[test]
fn a_layer_that_the_type_of_the_profile_does_not_take_is_candidate_invalid() {
    let mut c = stack();
    c.kind = "add-on".into();
    let message = refused(c, CreateRule::TypeLayer);
    assert!(
        message.contains("layer 1 (ubuntu-aws) has the layer os")
            && message.contains("type add-on takes only the layers addon, cni, and csi"),
        "{message}"
    );
    // an add-on profile takes a cni and a csi layer
    let mut c = stack();
    c.kind = "add-on".into();
    c.layers.drain(0..2);
    assert!(check(c, Path::new("c.yaml")).is_ok());
    let mut c = stack();
    c.kind = "infra".into();
    let message = refused(c, CreateRule::TypeLayer);
    assert!(
        message.contains("layer 5 (cert-manager) has the layer addon"),
        "{message}"
    );
    // a manifest layer has the layer addon
    let mut c = manifest_candidate();
    c.kind = "infra".into();
    refused(c, CreateRule::TypeLayer);
}

#[test]
fn a_cluster_or_infra_profile_with_no_core_layer_is_candidate_invalid() {
    let mut c = stack();
    c.layers.remove(0);
    let message = refused(c, CreateRule::CoreLayers);
    assert!(
        message.contains("type cluster needs the layers os, k8s, cni, and csi")
            && message.contains("has no layer os"),
        "{message}"
    );
    let mut c = stack();
    c.kind = "infra".into();
    c.layers.truncate(2);
    let message = refused(c, CreateRule::CoreLayers);
    assert!(message.contains("has no layer cni and csi"), "{message}");
    // edge-native needs no csi
    let mut c = stack();
    c.cloud_type = "edge-native".into();
    c.layers.remove(3);
    assert!(check(c, Path::new("c.yaml")).is_ok());
    // a layer that the file does not give comes from the registry: resolve checks it
    let mut c = stack();
    c.layers.remove(0);
    c.layers[0].layer = None;
    assert!(check(c, Path::new("c.yaml")).is_ok());
}

#[test]
fn a_candidate_that_the_validate_route_checks_gives_a_record() {
    // `--validate` asks Palette for these rules, so strata does not copy them. They are values
    // and a manifest that are not YAML, more than 200 packs, and more than 200 manifests
    let mut c = stack();
    c.layers[1].values = Some("replicas: {{ .spectro.var.replicas }}".into());
    c.layers[4].manifests[0].content = Some("- kind: Namespace".into());
    assert!(check(c, Path::new("c.yaml")).is_ok());
    let mut c = stack();
    let addon = c.layers[4].clone();
    c.layers.extend((0..197).map(|i| CandidateLayer {
        name: format!("addon-{i}"),
        ..addon.clone()
    }));
    assert_eq!(c.layers.len(), 202);
    assert!(check(c, Path::new("c.yaml")).is_ok());
}

/// A registry that serves each layer of [`stack`] with the layer `layers[i]` and the cloud
/// types `clouds[i]`. The listing gives each version empty values, and the tags route gives
/// none.
fn served(layers: &[&str], clouds: &[Value]) -> Palette<MemoryApi> {
    served_with_values(layers, clouds, "")
}

/// [`served`], where the tags route gives each version the default values `values` in
/// `packValues[].values`.
fn served_with_values(layers: &[&str], clouds: &[Value], values: &str) -> Palette<MemoryApi> {
    let mut api = MemoryApi::new();
    for (i, l) in stack().layers.iter().enumerate() {
        let uid = format!("pk-{i}");
        let summary = json!({"metadata": {"uid": uid},
                "spec": {"name": l.name, "version": l.version, "registryUid": "r1",
                         "layer": layers[i], "type": "spectro", "values": "",
                         "cloudTypes": clouds[i]}});
        api = api
            .with(
                format!("/v1/packs?filters=spec.name={}&limit=100&offset=0", l.name),
                json!({"items": [summary], "listmeta": {"count": 1}}),
            )
            .with(
                format!("/v1/packs/{}/registries/r1", l.name),
                json!({"tags": [{"tag": l.version, "packUid": uid}],
                           "packValues": [{"packUid": uid, "values": values}]}),
            );
    }
    Palette::new(api)
}

#[test]
fn a_layer_with_no_values_takes_the_default_values_of_the_tags_route_when_the_listing_gives_none() {
    // the listing gives `values: ""`, so the default values of the version come from the
    // tags route. A layer with values in the file keeps them
    let palette = served_with_values(&STACK_LAYERS, &vec![json!(["aws"]); 5], "b: 2");
    let raw = resolved(stack(), &palette).unwrap();
    let packs = &raw.profile["spec"]["published"]["packs"];
    assert_eq!(
        packs[0]["values"], "b: 2",
        "ubuntu-aws has no values in the file"
    );
    assert_eq!(
        packs[1]["values"], "a: 1",
        "kubernetes has values in the file"
    );
}

const STACK_LAYERS: [&str; 5] = ["os", "k8s", "cni", "csi", "addon"];

/// The documents of the resolve of `c`.
fn resolved(c: Candidate, palette: &Palette<MemoryApi>) -> Result<RawProfile> {
    resolve(
        &check(c, Path::new("c.yaml")).unwrap(),
        palette,
        &Progress::silent(),
    )
    .map(|r| r.raw)
}

#[test]
fn a_pack_that_does_not_support_the_cloud_of_the_profile_is_candidate_invalid() {
    let aws = json!(["aws"]);
    let mut clouds = vec![aws.clone(); 5];
    assert!(resolved(stack(), &served(&STACK_LAYERS, &clouds)).is_ok());
    clouds[2] = json!(["vsphere", "edge-native"]);
    let palette = served(&STACK_LAYERS, &clouds);
    let err = resolved(stack(), &palette).unwrap_err();
    let Error::CandidateInvalid { rule, message, .. } = &err else {
        panic!("not candidate-invalid: {err}");
    };
    assert_eq!(*rule, Some(CreateRule::Cloud), "{message}");
    assert!(
        message.contains("layer 3 (cni-cilium-oss)")
            && message.contains("the clouds vsphere and edge-native")
            && message.ends_with("a profile of the cloud aws"),
        "{message}"
    );
    // the cloud all, and a candidate with no cloud, take each pack
    for cloud in [CLOUD_ALL, ""] {
        let mut c = stack();
        c.cloud_type = cloud.into();
        assert!(resolved(c, &palette).is_ok(), "{cloud:?}");
    }
    // a pack of each cloud, a pack with no cloud type, and a custom pack on a custom cloud
    clouds[2] = json!([CLOUD_ALL]);
    clouds[3] = json!([]);
    assert!(resolved(stack(), &served(&STACK_LAYERS, &clouds)).is_ok());
    let mut c = stack();
    c.cloud_type = "my-cloud".into();
    assert!(resolved(c, &served(&STACK_LAYERS, &vec![json!([CLOUD_CUSTOM]); 5])).is_ok());
}

#[test]
fn resolve_checks_the_layers_that_the_registry_gives() {
    // the file gives no layer for kubernetes, and the registry gives k8s
    let mut c = stack();
    c.kind = "add-on".into();
    c.layers
        .retain(|l| l.name == "kubernetes" || l.name == "cert-manager");
    c.layers[0].layer = None;
    assert!(check(c.clone(), Path::new("c.yaml")).is_ok());
    let clouds = vec![json!(["aws"]); 5];
    let err = resolved(c, &served(&STACK_LAYERS, &clouds)).unwrap_err();
    assert!(
        err.to_string()
            .contains("layer 1 (kubernetes) has the layer k8s"),
        "{err}"
    );
    // and a core layer that is not there, which only the registry shows
    let mut c = stack();
    c.layers[0].layer = None;
    let mut layers = STACK_LAYERS;
    layers[0] = "addon";
    let err = resolved(c, &served(&layers, &clouds)).unwrap_err();
    assert!(err.to_string().contains("has no layer os"), "{err}");
}

#[test]
fn a_layer_from_a_registry_takes_only_the_layer_of_the_registry() {
    let palette = served(&STACK_LAYERS, &vec![json!(["aws"]); 5]);
    // kubernetes first, so that it is layer 1; with no layer on ubuntu-aws, the check of the
    // file does not know each layer, and does not check the core layers
    let first = |layer: Option<&str>| {
        let mut c = stack();
        c.layers.swap(0, 1);
        c.layers[0].layer = layer.map(str::to_string);
        c.layers[1].layer = None;
        c
    };
    let err = resolved(first(Some("addon")), &palette).unwrap_err();
    let Error::CandidateInvalid { message, .. } = &err else {
        panic!("not candidate-invalid: {err}");
    };
    assert!(
        message.contains("layer 1 (kubernetes) has the layer addon")
            && message.contains("its registry gives the layer k8s")
            && message.contains("Palette takes the layer from the registry"),
        "{message}"
    );
    // the layer of the registry, or no layer, gives the pack the layer of the registry
    let records: Vec<RawProfile> = [Some("k8s"), None]
        .into_iter()
        .map(|layer| resolved(first(layer), &palette).unwrap())
        .collect();
    for raw in &records {
        assert_eq!(raw.profile["spec"]["published"]["packs"][0]["layer"], "k8s");
        assert_eq!(raw.manifests["spec"]["packs"][0]["spec"]["layer"], "k8s");
    }
    assert_eq!(records[0].profile, records[1].profile);
    // a wrong layer that the type of the profile takes, and that leaves each core layer
    let mut c = stack();
    c.layers[4].layer = Some("cni".into());
    assert!(check(c.clone(), Path::new("c.yaml")).is_ok());
    let err = resolved(c, &palette).unwrap_err();
    assert!(
        err.to_string().contains(
            "layer 5 (cert-manager) has the layer cni, and its registry gives the layer addon"
        ),
        "{err}"
    );
}

#[test]
fn palette_takes_a_loose_semantic_version_or_a_text_with_three_numbers_as_the_version() {
    let whom = "layer 1 (podinfo) is from registry charts-auth, which Palette does not sync";
    for tag in [
        "1.0.0",
        "v1.2.3",
        "1",
        "1.2",
        "1.2.3-rc.1+b5",
        "01.2.3",
        "chart-1.2.3",
        "1.2.3.4",
        "1.2.3-",
    ] {
        assert!(gaps::check_pack_tag(whom, tag).is_ok(), "{tag}");
    }
    for tag in [
        "",
        "stable",
        "latest",
        "1.2.x",
        "LTS__24.x",
        "1.2-rc..1",
        "v",
    ] {
        assert!(gaps::check_pack_tag(whom, tag).is_err(), "{tag}");
    }
    // only the create checks the tag, so the refusal names its rule and the layer
    let broken = gaps::check_pack_tag(whom, "stable").unwrap_err();
    assert_eq!(broken.rule, CreateRule::PackTag);
    assert_eq!(
        broken.message,
        format!(
            "{whom}; the tag stable is not a semantic version, and it holds no three numbers \
                 with dots between them"
        )
    );
}

#[test]
fn a_variable_with_a_name_and_a_default_loads_with_the_defaults_of_a_record() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("c.yaml");
    let with = |variables: &str| {
        std::fs::write(&file, format!("{SAME_CANDIDATE}{variables}")).unwrap();
        load(&file)
    };
    let c = with("- name: tier\n  defaultValue: gold\n")
        .unwrap()
        .candidate;
    let tier = &c.variables[1];
    assert_eq!(
        (tier.name.as_str(), tier.default_value.as_str()),
        ("tier", "gold")
    );
    assert_eq!(
        (tier.format.as_str(), tier.input_type.as_str()),
        ("string", "text")
    );
    assert!(!tier.required && tier.options.is_empty());
    // a variable of Palette keeps the twelve fields, so a field of another name is not kept
    assert!(with("- name: tier\n  order: 3\n").is_ok());
    match with("- displayName: no name\n").unwrap_err() {
        Error::CandidateInvalid { message, .. } => {
            assert!(message.contains("missing field `name`"), "{message}")
        }
        other => panic!("not candidate-invalid: {other}"),
    }
}
