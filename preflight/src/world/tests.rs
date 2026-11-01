use super::*;
use strata::api::{cloud_account_path, cloud_config_path, subject_path};

#[test]
fn a_world_of_a_cluster_reads_its_cloud_type_as_the_route_of_its_cloud_config_does() {
    // the document names its cloud config, and no spec.cloudType
    let api = strata::api::MemoryApi::new()
        .with(
            subject_path(SubjectKind::Cluster, "c1"),
            json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
        )
        .with(cloud_config_path("aws", "cc1"), json!({"spec": {}}));
    let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
    assert_eq!(world.cloud, "aws");
    let none = strata::api::MemoryApi::new().with(
        subject_path(SubjectKind::Cluster, "c1"),
        json!({"metadata": {"uid": "c1"}, "spec": {}}),
    );
    let err = World::from_cluster(&Palette::new(none), "c1", &Progress::silent()).unwrap_err();
    assert!(err.to_string().contains("names no cloud type"), "{err}");
}

#[test]
fn a_world_of_a_cluster_by_name_reads_the_cluster_that_the_name_names() {
    let api = strata::api::MemoryApi::new()
        .with(
            "/v1/dashboard/spectroclusters/meta",
            json!([{"metadata": {"name": "barney", "uid": "c1"}}]),
        )
        .with(
            subject_path(SubjectKind::Cluster, "c1"),
            json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
        )
        .with(cloud_config_path("aws", "cc1"), json!({"spec": {}}));
    let world = World::from_cluster(&Palette::new(api), "barney", &Progress::silent()).unwrap();
    assert_eq!(world.cluster_uid.as_deref(), Some("c1"));
    assert_eq!(world.source, "cluster:c1");
    assert_eq!(str_at(&world.entity, "/metadata/name"), "barney");
}

#[test]
fn a_world_of_a_cluster_tries_an_id_of_24_hex_digits_as_a_uid_and_then_as_a_name() {
    let id = "5f1c0b7a9d3e2a4b6c8d0e1f";
    let document = |uid: &str| {
        json!({"metadata": {"uid": uid, "name": "barney"},
                "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}})
    };
    let config = json!({"spec": {}});
    // the id is a uid: one request for the document, and no listing
    let by_uid = strata::api::MemoryApi::new()
        .with(subject_path(SubjectKind::Cluster, id), document(id))
        .with(format!("/v1/spectroclusters/{id}/variables"), json!([]))
        .with(cloud_config_path("aws", "cc1"), config.clone());
    let world = World::from_cluster(&Palette::new(&by_uid), id, &Progress::silent()).unwrap();
    assert_eq!(world.cluster_uid.as_deref(), Some(id));
    assert_eq!(by_uid.calls(&subject_path(SubjectKind::Cluster, id)), 1);
    assert_eq!(by_uid.calls("/v1/dashboard/spectroclusters/meta"), 0);
    // the id is a name: the uid route answers 404, and the listing gives the uid
    let by_name = strata::api::MemoryApi::new()
        .with(
            "/v1/dashboard/spectroclusters/meta",
            json!([{"metadata": {"name": id, "uid": "c1"}}]),
        )
        .with(subject_path(SubjectKind::Cluster, "c1"), document("c1"))
        .with("/v1/spectroclusters/c1/variables", json!([]))
        .with(cloud_config_path("aws", "cc1"), config);
    let world = World::from_cluster(&Palette::new(&by_name), id, &Progress::silent()).unwrap();
    assert_eq!(world.cluster_uid.as_deref(), Some("c1"));
    assert_eq!(by_name.calls(&subject_path(SubjectKind::Cluster, id)), 1);
    assert_eq!(by_name.calls("/v1/dashboard/spectroclusters/meta"), 1);
}

#[test]
fn the_edge_engine_is_the_cloud_type_edge_native_in_a_route_an_account_and_a_file() {
    assert_eq!(route_cloud(EDGE), CLOUD_EDGE_NATIVE);
    assert_eq!(account_kind(EDGE), CLOUD_EDGE_NATIVE);
    assert_eq!(normalize_cloud(" Edge-Native "), EDGE);
    assert_eq!(normalize_cloud(EDGE), EDGE);
    for (engine, account) in [
        ("eks", "aws"),
        ("gke", "gcp"),
        ("aks", "azure"),
        ("aws", "aws"),
    ] {
        assert_eq!(account_kind(engine), account, "{engine}");
        assert_eq!(route_cloud(engine), engine, "{engine}");
    }
    assert!(clouds().any(|c| c == EDGE));
    assert!(!clouds().any(|c| c == CLOUD_EDGE_NATIVE));
}

#[test]
fn each_engine_has_the_text_of_a_cloud_of_palette_except_edge_and_its_text_parses_back() {
    for engine in ENGINES {
        assert_eq!(Engine::parse(engine.as_str()), Some(engine));
        match engine {
            Engine::Cloud(cloud) => assert_eq!(Cloud::of(engine.as_str()), cloud),
            Engine::Edge => assert_eq!(engine.cloud(), Cloud::EdgeNative),
        }
    }
    assert_eq!(Engine::parse("openstack"), None);
    assert_eq!(Engine::parse(CLOUD_EDGE_NATIVE), None);
    assert_eq!(
        cloud_list(),
        "aws, eks, gcp, gke, azure, aks, maas, edge, vsphere"
    );
}

#[test]
fn the_world_gives_the_account_kind_and_the_route_cloud_of_each_engine() {
    for engine in clouds() {
        let world = World {
            cloud: engine.to_string(),
            ..World::default()
        }
        .to_value()
        .unwrap();
        assert_eq!(world["accountKind"], account_kind(engine), "{engine}");
        assert_eq!(world["routeCloud"], route_cloud(engine), "{engine}");
    }
    let mut no_cloud = json!({"entity": {}});
    add_derived_fields(&mut no_cloud).unwrap();
    assert_eq!(
        no_cloud["accountKind"],
        Value::Null,
        "no cloud, no field of the engine"
    );
    assert_eq!(no_cloud["routeCloud"], Value::Null);
    assert_eq!(no_cloud["edgeHosts"], json!([]));
}

#[test]
fn the_edge_hosts_of_the_world_are_the_hosts_of_each_pool_with_the_pool_in_order() {
    let entity = json!({"spec": {"machinepoolconfig": [
        {"poolConfig": {"name": "cp"}, "cloudConfig": {"edgeHosts": [{"hostUid": "h1"}, {"hostUid": "h2"}]}},
        {"poolConfig": {"name": "none"}, "cloudConfig": {}},
        {"poolConfig": {"name": "worker"}, "cloudConfig": {"edgeHosts": [{"hostUid": "h3"}]}},
    ]}});
    let world = World {
        cloud: EDGE.to_string(),
        entity,
        ..World::default()
    }
    .to_value()
    .unwrap();
    assert_eq!(
        world["edgeHosts"],
        json!([
            {"pool": {"name": "cp"}, "host": {"hostUid": "h1"}},
            {"pool": {"name": "cp"}, "host": {"hostUid": "h2"}},
            {"pool": {"name": "worker"}, "host": {"hostUid": "h3"}},
        ])
    );
    assert_eq!(World::default().to_value().unwrap()["edgeHosts"], json!([]));
}

#[test]
fn the_validate_body_is_the_entity_and_for_an_existing_cluster_it_has_a_name_of_its_own() {
    let entity = json!({"metadata": {"name": "barney", "labels": {"a": "b"}}, "spec": {}});
    let file = World {
        cloud: "aws".to_string(),
        entity: entity.clone(),
        ..World::default()
    }
    .to_value()
    .unwrap();
    assert_eq!(file["validateBody"], entity);
    let cluster = World {
        cloud: "aws".to_string(),
        entity: entity.clone(),
        cluster_uid: Some("c1".to_string()),
        ..World::default()
    }
    .to_value()
    .unwrap();
    assert_eq!(
        cluster["validateBody"]["metadata"]["name"],
        "barney-preflight"
    );
    assert_eq!(
        cluster["validateBody"]["metadata"]["labels"],
        entity["metadata"]["labels"]
    );
    assert_eq!(
        cluster["entity"], entity,
        "the entity itself keeps its name"
    );
    let unnamed = World {
        entity: json!({"spec": {}}),
        cluster_uid: Some("c1".to_string()),
        ..World::default()
    }
    .to_value()
    .unwrap();
    assert_eq!(
        unnamed["validateBody"]["metadata"]["name"],
        "cluster-preflight"
    );
}

#[test]
fn a_cloud_config_pool_splits_into_the_two_parts_of_the_entity() {
    let flat = json!({"name": "worker-pool", "isControlPlane": false, "size": 2,
            "instanceType": "t3.xlarge", "azs": ["us-east-2a"],
            "subnetIds": {"us-east-2a": "subnet-1"}, "amiType": "AL2023_x86_64_STANDARD"});
    let e = split_pool(&flat);
    assert_eq!(e["poolConfig"]["name"], "worker-pool");
    assert_eq!(e["poolConfig"]["size"], 2);
    assert_eq!(e["cloudConfig"]["instanceType"], "t3.xlarge");
    assert_eq!(e["cloudConfig"]["subnets"][0]["id"], "subnet-1");
    assert_eq!(e["cloudConfig"]["amiType"], "AL2023_x86_64_STANDARD");
    assert!(e["cloudConfig"].get("name").is_none());
}

#[test]
fn a_disabled_skip_k8s_upgrade_stays_out_of_the_entity() {
    // Palette stores "disabled" on each pool, and the GKE validate route refuses the field
    let off = split_pool(&json!({"name": "w", "size": 3, "skipK8sUpgrade": "disabled"}));
    assert!(off["poolConfig"].get("skipK8sUpgrade").is_none());
    let on = split_pool(&json!({"name": "w", "size": 3, "skipK8sUpgrade": "enabled"}));
    assert_eq!(on["poolConfig"]["skipK8sUpgrade"], "enabled");
}

#[test]
fn a_change_merges_pools_by_name_and_keeps_the_rest() {
    let mut entity = json!({"spec": {"cloudConfig": {"region": "us-east-2"},
            "machinepoolconfig": [
                {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "worker-pool", "size": 2}},
                {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "cp", "size": 1}}]}});
    let change = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "m5.xlarge"}, "poolConfig": {"name": "worker-pool"}}]}});
    merge_change(&mut entity, &change);
    let pools = entity["spec"]["machinepoolconfig"].as_array().unwrap();
    assert_eq!(pools.len(), 2);
    assert_eq!(pools[0]["cloudConfig"]["instanceType"], "m5.xlarge");
    assert_eq!(
        pools[0]["poolConfig"]["size"], 2,
        "a field the change omits stays"
    );
    assert_eq!(pools[1]["poolConfig"]["name"], "cp");
    assert_eq!(entity["spec"]["cloudConfig"]["region"], "us-east-2");
}

#[test]
fn a_change_adds_a_pool_with_an_empty_name_and_does_not_merge_it_into_another() {
    let mut entity = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "t3.xlarge"}, "poolConfig": {"name": "", "size": 2}}]}});
    let change = json!({"spec": {"machinepoolconfig": [
            {"cloudConfig": {"instanceType": "m5.xlarge"}, "poolConfig": {"name": "", "size": 3}}]}});
    merge_change(&mut entity, &change);
    let pools = entity["spec"]["machinepoolconfig"].as_array().unwrap();
    assert_eq!(pools.len(), 2);
    assert_eq!(pools[0]["poolConfig"]["size"], 2);
    assert_eq!(pools[1]["poolConfig"]["size"], 3);
}

#[test]
fn the_entity_of_a_cluster_keeps_the_labels_and_the_annotations_that_are_text() {
    let api = strata::api::MemoryApi::new()
        .with(
            subject_path(SubjectKind::Cluster, "c1"),
            json!({"metadata": {"uid": "c1", "name": "barney",
                    "labels": {"env": "dev", "n": 1}, "annotations": {"owner": "ops"}},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"}}}),
        )
        .with(cloud_config_path("aws", "cc1"), json!({"spec": {}}));
    let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
    assert_eq!(
        world.entity["metadata"]["labels"],
        json!({"env": "dev"}),
        "a value that is not text is no label"
    );
    assert_eq!(
        world.entity["metadata"]["annotations"],
        json!({"owner": "ops"})
    );
    let bare = strata::api::MemoryApi::new()
        .with(
            subject_path(SubjectKind::Cluster, "c2"),
            json!({"metadata": {"uid": "c2", "name": "wilma"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc2"}}}),
        )
        .with(cloud_config_path("aws", "cc2"), json!({"spec": {}}));
    let world = World::from_cluster(&Palette::new(bare), "c2", &Progress::silent()).unwrap();
    assert_eq!(world.entity["metadata"]["labels"], json!({}));
    assert_eq!(world.entity["metadata"]["annotations"], json!({}));
}

#[test]
fn the_cloud_comes_from_the_shape_when_it_fits_one() {
    let gke = json!({"spec": {"cloudConfig": {"project": "p", "region": "r", "managedClusterConfig": {"location": "r"}}}});
    assert_eq!(infer_cloud(&gke).as_deref(), Some("gke"));
    let gcp = json!({"spec": {"cloudConfig": {"project": "p", "region": "r"}}});
    assert_eq!(infer_cloud(&gcp).as_deref(), Some("gcp"));
    let eks = json!({"spec": {"cloudConfig": {"region": "r", "endpointAccess": {"public": true}}}});
    assert_eq!(infer_cloud(&eks).as_deref(), Some("eks"));
    let aws = json!({"spec": {"cloudConfig": {"region": "r", "sshKeyName": "k"}}});
    assert_eq!(infer_cloud(&aws), None, "aws and eks share this shape");
    let maas = json!({"spec": {"cloudConfig": {"domain": "maas"}}});
    assert_eq!(infer_cloud(&maas).as_deref(), Some("maas"));
    let edge = json!({"spec": {"machinepoolconfig": [{"cloudConfig": {"edgeHosts": []}}]}});
    assert_eq!(infer_cloud(&edge).as_deref(), Some("edge"));
}

#[test]
fn an_envelope_names_the_cloud() {
    let (cloud, entity) =
        unwrap_envelope(json!({"cloudType": "edge-native", "entity": {"spec": {}}}));
    assert_eq!(cloud.as_deref(), Some("edge"));
    assert!(entity.get("spec").is_some());
    let (cloud, entity) = unwrap_envelope(json!({"spec": {"a": 1}}));
    assert_eq!(cloud, None);
    assert_eq!(entity["spec"]["a"], 1);
}

#[test]
fn a_world_of_a_file_needs_a_cloud_it_can_name() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("c.json");
    std::fs::write(&p, r#"{"spec": {"cloudConfig": {"region": "us-east-2"}}}"#).unwrap();
    let err = World::from_file(&p, None).unwrap_err();
    assert!(err.to_string().contains("give --cloud"), "{err}");
    let w = World::from_file(&p, Some("eks")).unwrap();
    assert_eq!(w.cloud, "eks");
    assert_eq!(
        w.to_value().unwrap()["entity"]["spec"]["cloudConfig"]["region"],
        "us-east-2"
    );
    let err = World::from_file(&p, Some("mars")).unwrap_err();
    assert!(err.to_string().contains("not a cloud"), "{err}");
}

#[test]
fn a_world_never_carries_a_secret() {
    let mut account = json!({"spec": {"credentialType": "sts", "sts": {"arn": "arn:x", "externalId": "e"},
            "secretKey": "s", "nested": [{"password": "p"}]}});
    redact(&mut account);
    let text = account.to_string();
    assert!(!text.contains("\"s\"") && !text.contains("\"p\""), "{text}");
    assert_eq!(account["spec"]["sts"]["arn"], "arn:x");
    assert_eq!(
        account["spec"]["sts"]["externalId"], "e",
        "the validate route needs it at run time"
    );
    redact_for_output(&mut account);
    assert_eq!(account["spec"]["sts"]["externalId"], "<redacted>");
}

#[test]
fn an_account_the_key_cannot_read_is_null_and_a_bad_key_is_an_error() {
    let base = || World {
        cloud: "aws".into(),
        source: "c.json".into(),
        entity: json!({"metadata": {"name": "fred"}, "spec": {"cloudAccountUid": "acc1"}}),
        ..World::default()
    };
    for status in [403u16, 404] {
        let api =
            strata::api::MemoryApi::new().failing(cloud_account_path("aws", "acc1"), status, "{}");
        let w = base()
            .with_palette(&Palette::new(&api), "t", &Progress::silent())
            .unwrap();
        assert!(w.account.is_none(), "{status}: the world says null");
    }
    for status in [401u16, 429] {
        let api =
            strata::api::MemoryApi::new().failing(cloud_account_path("aws", "acc1"), status, "{}");
        let err = base()
            .with_palette(&Palette::new(&api), "t", &Progress::silent())
            .unwrap_err()
            .to_string();
        assert!(err.contains(&format!("HTTP {status}")), "{status}: {err}");
    }
}

#[test]
fn a_cluster_becomes_a_create_body() {
    let cluster = json!({"metadata": {"uid": "c1", "name": "fred", "labels": {"env": "lab"}},
            "spec": {"cloudType": "eks", "clusterType": "AlloyAssist", "clusterConfig": {"timezone": "Etc/UTC"},
                "clusterProfileTemplates": [{"uid": "p1", "packs": [{"name": "kubernetes-eks", "tag": "1.32", "type": "spectro", "values": "x", "manifests": []}]}]}});
    let config = json!({"spec": {"cloudAccountRef": {"uid": "acc1"},
            "clusterConfig": {"region": "us-east-2", "vpcId": "", "endpointAccess": {"public": true}},
            "machinePoolConfig": [{"name": "worker-pool", "size": 2, "instanceType": "t3.xlarge", "azs": ["us-east-2a", "us-east-2b"]}]}});
    let e = entity_of(&cluster, &config, Some("acc1"), &json!([]));
    assert_eq!(e["metadata"]["name"], "fred");
    assert_eq!(e["spec"]["cloudAccountUid"], "acc1");
    assert_eq!(e["spec"]["cloudConfig"]["region"], "us-east-2");
    assert_eq!(e["spec"]["machinepoolconfig"][0]["poolConfig"]["size"], 2);
    assert_eq!(
        e["spec"]["machinepoolconfig"][0]["cloudConfig"]["azs"][1],
        "us-east-2b"
    );
    assert_eq!(e["spec"]["profiles"][0]["uid"], "p1");
    assert_eq!(
        e["spec"]["profiles"][0]["packValues"][0]["name"],
        "kubernetes-eks"
    );
    assert_eq!(e["spec"]["clusterType"], "AlloyAssist");
    assert_eq!(e["spec"]["profiles"][0]["variables"], json!([]));
}

#[test]
fn a_pack_of_a_template_keeps_its_manifests_and_gives_empty_text_for_a_missing_field() {
    let cluster = json!({"spec": {"clusterProfileTemplates": [{"uid": "p1", "packs": [
        {"name": "k8s", "tag": "1.32", "type": "spectro", "values": "x: 1\n"},
        {"name": "lab", "type": "manifest", "manifests": [{"name": "m", "uid": "m1", "content": "a"}]},
    ]}]}});
    let e = entity_of(&cluster, &json!({}), None, &json!([]));
    let packs = &e["spec"]["profiles"][0]["packValues"];
    assert_eq!(
        packs[0],
        json!({"name": "k8s", "tag": "1.32", "type": "spectro", "values": "x: 1\n",
                "manifests": []})
    );
    assert_eq!(
        packs[1],
        json!({"name": "lab", "tag": "", "type": "manifest", "values": "",
                "manifests": [{"name": "m", "uid": "m1"}]})
    );
}

/// A cluster of two profiles, and the answer of its variables route: the first profile
/// declares two variables, one of them sensitive, and the second declares none.
fn cluster_api(variables: serde_json::Value) -> strata::api::MemoryApi {
    strata::api::MemoryApi::new()
            .with(
                subject_path(SubjectKind::Cluster, "c1"),
                json!({"metadata": {"uid": "c1", "name": "barney"},
                    "spec": {"cloudConfigRef": {"kind": "aws", "uid": "cc1"},
                        "clusterProfileTemplates": [{"uid": "p1", "packs": []}, {"uid": "p2", "packs": []}]}}),
            )
            .with(cloud_config_path("aws", "cc1"), json!({"spec": {}}))
            .with("/v1/spectroclusters/c1/variables", variables)
}

#[test]
fn the_entity_of_a_cluster_carries_the_variable_values_of_each_profile() {
    let api = cluster_api(json!([{"profileUid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.244.0.0/16", "format": "string", "required": true},
            {"name": "db_password", "value": "********", "isSensitive": true}]}]));
    let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
    let profiles = &world.entity["spec"]["profiles"];
    assert_eq!(
        profiles[0]["variables"],
        json!([
            {"name": "pod_cidr", "value": "10.244.0.0/16"},
            {"name": "db_password", "value": "********"}
        ]),
        "the name and the value, and a sensitive value as Palette masks it"
    );
    assert_eq!(
        profiles[1]["variables"],
        json!([]),
        "a profile with no entry has no variable"
    );
}

#[test]
fn a_cluster_on_a_palette_with_no_variables_route_sends_no_variable() {
    let api = cluster_api(json!([]));
    let older = cluster_api(json!([])).failing("/v1/spectroclusters/c1/variables", 405, "{}");
    for api in [api, older] {
        let world = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap();
        assert_eq!(world.entity["spec"]["profiles"][0]["variables"], json!([]));
    }
}

#[test]
fn a_variables_route_that_fails_stops_the_world_of_a_cluster() {
    let api = cluster_api(json!([])).failing("/v1/spectroclusters/c1/variables", 500, "boom");
    let err = World::from_cluster(&Palette::new(api), "c1", &Progress::silent()).unwrap_err();
    assert!(err.to_string().contains("HTTP 500"), "{err}");
}

#[test]
fn a_change_merges_the_variables_of_a_profile_by_name_and_keeps_the_rest() {
    let mut entity = json!({"spec": {"profiles": [{"uid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.244.0.0/16"},
            {"name": "owner", "value": "platform"}]}]}});
    let change = json!({"spec": {"profiles": [{"uid": "p1", "variables": [
            {"name": "pod_cidr", "value": "10.0.0.0/8"},
            {"name": "region", "value": "eu-west-3"}]}]}});
    merge_change(&mut entity, &change);
    assert_eq!(
        entity["spec"]["profiles"][0]["variables"],
        json!([
            {"name": "pod_cidr", "value": "10.0.0.0/8"},
            {"name": "owner", "value": "platform"},
            {"name": "region", "value": "eu-west-3"}
        ]),
        "a variable of the change replaces its value, a new name is added, the rest stays"
    );
}

#[test]
fn a_world_of_an_existing_cluster_whose_entity_or_metadata_is_not_an_object_is_an_input_error() {
    let existing = |entity: Value| World {
        cloud: "aws".to_string(),
        entity,
        cluster_uid: Some("c1".to_string()),
        ..World::default()
    };
    for metadata in [json!(5), json!("barney"), json!([1]), json!(true)] {
        let err = existing(json!({"metadata": metadata, "spec": {}}))
            .to_value()
            .unwrap_err();
        assert_eq!(err.kind(), "input", "{metadata}");
        assert!(
            err.to_string()
                .starts_with("entity.metadata is not an object"),
            "{err}"
        );
    }
    for entity in [json!(5), json!("barney"), json!([{"spec": {}}])] {
        let err = existing(entity.clone()).to_value().unwrap_err();
        assert_eq!(err.kind(), "input", "{entity}");
        assert!(
            err.to_string().starts_with("entity is not an object"),
            "{err}"
        );
    }
}

#[test]
fn a_world_of_an_existing_cluster_names_the_validate_body_when_the_metadata_or_the_entity_is_null()
{
    for entity in [Value::Null, json!({"metadata": null, "spec": {}})] {
        let world = World {
            entity,
            cluster_uid: Some("c1".to_string()),
            ..World::default()
        }
        .to_value()
        .unwrap();
        assert_eq!(
            world["validateBody"]["metadata"]["name"],
            "cluster-preflight"
        );
    }
}

#[test]
fn a_world_with_no_cluster_sends_a_metadata_that_is_not_an_object_as_it_is() {
    let entity = json!({"metadata": "barney", "spec": {}});
    let world = World {
        entity: entity.clone(),
        ..World::default()
    }
    .to_value()
    .unwrap();
    assert_eq!(
        world["validateBody"], entity,
        "Palette answers for a body that it cannot read"
    );
}
