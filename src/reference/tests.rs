//! The tests of the refs: the text of each shape, and the text that a ref prints.

use super::*;

#[test]
fn a_ref_shaped_like_a_path_is_a_file_even_when_the_file_is_not_there() {
    let no_files = |_: &Path| false;
    for text in [
        "/tmp/strata-live-profile-1.1.0.json",
        "releases/dev.json",
        "./dev.json",
        "../dev.json",
        "~/records/dev.json",
        "dev.json",
        "candidate.yaml",
        "candidate.yml",
        "C:\\records\\dev.json",
        // a file whose name carries a version: the path wins over name@version
        "/tmp/aws-stack@1.1.1.json",
    ] {
        assert_eq!(
            Ref::parse_with(text, no_files),
            Ref::Snapshot(PathBuf::from(text)),
            "{text} reads as a file"
        );
    }

    // and the forms that are not paths keep their meaning
    assert_eq!(
        Ref::parse_with("aws-stack@1.1.1", no_files),
        Ref::NameVersion {
            name: "aws-stack".into(),
            version: "1.1.1".into()
        }
    );
    assert_eq!(
        Ref::parse_with("64f0a1", no_files),
        Ref::Uid("64f0a1".into())
    );
    assert_eq!(
        Ref::parse_with("cluster:payments/aws-stack", no_files),
        Ref::Cluster {
            uid: "payments".into(),
            profile: Some("aws-stack".into()),
            pending: false,
        }
    );
    assert_eq!(
        Ref::parse_with("pack:Public Repo/csi-aws-ebs@1.65.0", no_files),
        Ref::Pack {
            registry: Some("Public Repo".into()),
            name: "csi-aws-ebs".into(),
            version: "1.65.0".into()
        }
    );
}

#[test]
fn parse_forms() {
    let no_files = |_: &Path| false;
    assert_eq!(
        Ref::parse_with("64f0a1", no_files),
        Ref::Uid("64f0a1".into())
    );
    assert_eq!(
        Ref::parse_with("aws-stack@1.2.0", no_files),
        Ref::NameVersion {
            name: "aws-stack".into(),
            version: "1.2.0".into()
        }
    );
    assert_eq!(
        Ref::parse_with("@1.2.0", no_files),
        Ref::Uid("@1.2.0".into())
    );
    assert_eq!(
        Ref::parse_with("aws-stack@", no_files),
        Ref::Uid("aws-stack@".into())
    );
    assert_eq!(
        Ref::parse_with("rel.json", |_| true),
        Ref::Snapshot("rel.json".into())
    );
    assert_eq!(Ref::parse("aws-stack@1.2.0").to_string(), "aws-stack@1.2.0");
}

#[test]
fn a_profile_ref_with_draft_reads_the_draft_of_the_version() {
    let no_files = |_: &Path| false;
    let by_name = Ref::parse_with("aws-stack@1.2.0@draft", no_files);
    assert_eq!(
        by_name,
        Ref::Draft {
            id: "aws-stack".into(),
            version: Some("1.2.0".into()),
        }
    );
    assert_eq!(by_name.to_string(), "aws-stack@1.2.0@draft");
    let latest = Ref::parse_with("aws-stack@latest@draft", no_files);
    assert_eq!(
        latest,
        Ref::Draft {
            id: "aws-stack".into(),
            version: Some(LATEST.into()),
        }
    );
    let by_uid = Ref::parse_with("64f0a1@draft", no_files);
    assert_eq!(
        by_uid,
        Ref::Draft {
            id: "64f0a1".into(),
            version: None,
        }
    );
    assert_eq!(by_uid.to_string(), "64f0a1@draft");
    assert_eq!(
        Ref::parse_with("@draft", no_files),
        Ref::Uid("@draft".into()),
        "no profile, no draft"
    );
    assert_eq!(
        Ref::parse_with("releases/dev@draft", no_files),
        Ref::Snapshot("releases/dev@draft".into()),
        "a path stays a file"
    );
}

#[test]
fn pack_forms() {
    let no_files = |_: &Path| false;
    assert_eq!(
        Ref::parse_with("pack:csi-aws-ebs@1.65.0", no_files),
        Ref::Pack {
            registry: None,
            name: "csi-aws-ebs".into(),
            version: "1.65.0".into()
        }
    );
    let with_registry = Ref::parse_with("pack:Public Repo/csi-aws-ebs@latest", no_files);
    assert_eq!(
        with_registry,
        Ref::Pack {
            registry: Some("Public Repo".into()),
            name: "csi-aws-ebs".into(),
            version: LATEST.into()
        }
    );
    assert_eq!(
        with_registry.to_string(),
        "pack:Public Repo/csi-aws-ebs@latest"
    );
    assert_eq!(
        Ref::parse_with("pack:csi-aws-ebs", no_files),
        Ref::Uid("pack:csi-aws-ebs".into()),
        "a pack ref needs a version"
    );
}

#[test]
fn a_pack_ref_prints_as_the_pack_ref_that_it_is() {
    let no_files = |_: &Path| false;
    // an empty registry is none, and a name with a `/` after none prints as a registry
    for text in ["pack:/x@1", "pack:Public Repo/x@1", "pack:a/b/c@1"] {
        let read = Ref::parse_with(text, no_files);
        assert!(matches!(read, Ref::Pack { .. }), "{text}");
        assert_eq!(Ref::parse_with(&read.to_string(), no_files), read, "{text}");
    }
    for text in ["pack://x@1", "pack:/x/y@1"] {
        let read = Ref::parse_with(text, no_files);
        assert_eq!(
            read,
            Ref::Snapshot(text.into()),
            "{text}: a path, as the form says"
        );
        assert_eq!(read.to_string(), text);
    }
}

#[test]
fn cluster_forms() {
    let no_files = |_: &Path| false;
    assert_eq!(
        Ref::parse_with("cluster:c1", no_files),
        Ref::Cluster {
            uid: "c1".into(),
            profile: None,
            pending: false,
        }
    );
    let selected = Ref::parse_with("cluster:c1/aws-stack", no_files);
    assert_eq!(
        selected,
        Ref::Cluster {
            uid: "c1".into(),
            profile: Some("aws-stack".into()),
            pending: false,
        }
    );
    assert_eq!(selected.to_string(), "cluster:c1/aws-stack");
    assert_eq!(
        Ref::parse_with("cluster:c1/", no_files).to_string(),
        "cluster:c1"
    );
    assert_eq!(
        Ref::parse_with("cluster:", no_files),
        Ref::Uid("cluster:".into()),
        "no uid, no cluster"
    );
}

#[test]
fn a_name_and_a_version_make_the_ref_that_reads_back_as_they_are() {
    let no_files = |_: &Path| false;
    let text = name_version("aws-stack", "1.2.0");
    assert_eq!(text, "aws-stack@1.2.0");
    let read = Ref::parse_with(&text, no_files);
    assert_eq!(
        read,
        Ref::NameVersion {
            name: "aws-stack".into(),
            version: "1.2.0".into(),
        }
    );
    assert_eq!(read.to_string(), text);
}

#[test]
fn a_cluster_ref_with_pending_reads_the_update_that_waits() {
    let no_files = |_: &Path| false;
    let all = Ref::parse_with("cluster:c1@pending", no_files);
    assert_eq!(
        all,
        Ref::Cluster {
            uid: "c1".into(),
            profile: None,
            pending: true,
        }
    );
    assert_eq!(all.to_string(), "cluster:c1@pending");
    let one = Ref::parse_with("cluster:c1/aws-stack@pending", no_files);
    assert_eq!(
        one,
        Ref::Cluster {
            uid: "c1".into(),
            profile: Some("aws-stack".into()),
            pending: true,
        }
    );
    assert_eq!(one.to_string(), "cluster:c1/aws-stack@pending");
}

#[test]
fn a_slash_after_pending_with_no_profile_names_the_same_cluster_as_the_text_without_it() {
    let no_files = |_: &Path| false;
    for (text, same_as) in [
        ("cluster:c1@pending/", "cluster:c1@pending"),
        ("cluster:c1/@pending/", "cluster:c1@pending"),
        (
            "cluster:c1/aws-stack@pending/",
            "cluster:c1/aws-stack@pending",
        ),
    ] {
        let parsed = Ref::parse_with(text, no_files);
        assert_eq!(parsed, Ref::parse_with(same_as, no_files), "{text}");
        assert!(
            matches!(parsed, Ref::Cluster { pending: true, .. }),
            "{text} names the update that waits"
        );
        assert_eq!(parsed.to_string(), same_as, "{text}");
    }
    let with_profile = Ref::parse_with("cluster:c1@pending/aws-stack", no_files);
    assert_eq!(
        with_profile,
        Ref::Cluster {
            uid: "c1@pending".into(),
            profile: Some("aws-stack".into()),
            pending: false,
        },
        "a profile after the slash is a profile, so the uid is the text before the slash"
    );
}

#[test]
fn the_text_of_a_ref_parses_as_the_same_ref() {
    let no_files = |_: &Path| false;
    for text in [
        "cluster:c1@pending/",
        "cluster:c1/@pending/",
        "cluster:c1/aws-stack@pending/",
        "cluster:c1@pending/aws-stack",
        "cluster:c1@pending//",
        "cluster:@pending/",
        "cluster:@pending",
        "cluster:@pending@pending/",
        "cluster:c1@pending/@pending/",
        "tag:+",
        "tag:++",
        "tag:env=prod+",
        "tag:",
    ] {
        let parsed = Ref::parse_with(text, no_files);
        let again = Ref::parse_with(&parsed.to_string(), no_files);
        assert_eq!(again, parsed, "{text} prints as {parsed}");
    }
}

#[test]
fn a_tag_ref_with_no_label_is_no_set_of_clusters() {
    let no_files = |_: &Path| false;
    for text in ["tag:+", "tag:++"] {
        assert_eq!(
            Ref::parse_with(text, no_files),
            Ref::Uid(text.into()),
            "{text} names no label, so it names no set"
        );
    }
}

#[test]
fn edge_host_forms() {
    let no_files = |_: &Path| false;
    let all = Ref::parse_with("edgehost:e1", no_files);
    assert_eq!(
        all,
        Ref::EdgeHost {
            uid: "e1".into(),
            profile: None,
        }
    );
    assert_eq!(all.to_string(), "edgehost:e1");
    let one = Ref::parse_with("edgehost:e1/edge-stack", no_files);
    assert_eq!(
        one,
        Ref::EdgeHost {
            uid: "e1".into(),
            profile: Some("edge-stack".into()),
        }
    );
    assert_eq!(one.to_string(), "edgehost:e1/edge-stack");
    assert_eq!(
        Ref::parse_with("edgehost:", no_files),
        Ref::Uid("edgehost:".into()),
        "no uid, no edge host"
    );
}

#[test]
fn app_profile_forms() {
    let no_files = |_: &Path| false;
    let by_uid = Ref::parse_with("appprofile:a1", no_files);
    assert_eq!(
        by_uid,
        Ref::AppProfile {
            id: "a1".into(),
            version: None,
        }
    );
    assert_eq!(by_uid.to_string(), "appprofile:a1");
    let by_name = Ref::parse_with("appprofile:shop@1.0.0", no_files);
    assert_eq!(
        by_name,
        Ref::AppProfile {
            id: "shop".into(),
            version: Some("1.0.0".into()),
        }
    );
    assert_eq!(by_name.to_string(), "appprofile:shop@1.0.0");
    assert_eq!(
        Ref::parse_with("appprofile:", no_files),
        Ref::Uid("appprofile:".into()),
        "no uid, no app profile"
    );
}

#[test]
fn cluster_set_forms() {
    let no_files = |_: &Path| false;
    let tags = Ref::parse_with("tag:env=prod+team=search", no_files);
    assert_eq!(
        tags,
        Ref::ClusterSet {
            tags: vec!["env=prod".into(), "team=search".into()],
            filter: None,
        }
    );
    assert_eq!(tags.to_string(), "tag:env=prod+team=search");
    let filter = Ref::parse_with("filter:prod", no_files);
    assert_eq!(
        filter,
        Ref::ClusterSet {
            tags: vec![],
            filter: Some("prod".into()),
        }
    );
    assert_eq!(filter.to_string(), "filter:prod");
    assert_eq!(
        Ref::parse_with("tag:", no_files),
        Ref::Uid("tag:".into()),
        "no label, no set"
    );
}

#[test]
fn one_named_gives_the_one_hit_and_names_the_sorted_uids_of_several() {
    let not_found = || Error::Usage {
        message: "none".to_string(),
    };
    let ambiguous = |uids: Vec<String>| Error::Usage {
        message: uids.join(","),
    };
    let one = one_named(vec!["u1".to_string()], String::as_str, not_found, ambiguous);
    assert_eq!(one.unwrap(), "u1");
    let none = one_named(Vec::<String>::new(), String::as_str, not_found, ambiguous);
    assert_eq!(none.unwrap_err().to_string(), "none");
    let several = vec!["u2".to_string(), "u1".to_string()];
    let err = one_named(several, String::as_str, not_found, ambiguous).unwrap_err();
    assert_eq!(err.to_string(), "u1,u2", "the uids are sorted");
}

#[test]
fn a_profile_version_is_a_name_at_a_version_a_uid_or_the_draft_of_one() {
    let no_files = |_: &Path| false;
    for text in [
        "aws-stack@1.2.0",
        "64f0a1",
        "aws-stack@1.2.0@draft",
        "64f0a1@draft",
    ] {
        assert!(
            Ref::parse_with(text, no_files).is_profile_version(),
            "{text} names a profile version"
        );
    }
    for text in [
        "releases/dev.json",
        "pack:csi-aws-ebs@1.65.0",
        "cluster:c1",
        "cluster:c1/aws-stack@pending",
        "edgehost:e1",
        "appprofile:shop@1.0.0",
        "tag:env=prod",
        "filter:prod",
        "clustergroup:g1",
    ] {
        assert!(
            !Ref::parse_with(text, no_files).is_profile_version(),
            "{text} does not name a profile version"
        );
    }
}

#[test]
fn cluster_group_forms() {
    let no_files = |_: &Path| false;
    let group = Ref::parse_with("clustergroup:g1", no_files);
    assert_eq!(group, Ref::ClusterGroup { uid: "g1".into() });
    assert_eq!(group.to_string(), "clustergroup:g1");
    assert_eq!(
        Ref::parse_with("clustergroup:", no_files),
        Ref::Uid("clustergroup:".into()),
        "no uid, no cluster group"
    );
}
