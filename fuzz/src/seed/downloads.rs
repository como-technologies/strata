//! The seeds of the download of a profile version, as Palette writes it: see
//! `tests/common/download.rs`.

use strata::model::pack_type::PackType;

use crate::archive::{
    DownloadPack, chart_archive, chart_file, pack_archive, profile_download, tgz,
};
use crate::support::UID;

/// Downloads of a profile version, as Palette writes them. The packs are a pack with its archive
/// and a manifest, a pack of a private registry, and a manifest pack. Another seed holds the chart
/// of a Helm registry. The other seeds are a download with no list, an archive of a pack that is
/// no gzip, and an answer that is JSON.
pub(super) fn files() -> Vec<Vec<u8>> {
    let none = |name, kind, private| DownloadPack {
        name,
        version: "1.0.0",
        kind,
        private,
        archive: None,
        manifests: &[],
    };
    let archive = pack_archive(
        "kubernetes",
        "1.36.2",
        "k8s",
        "kubernetes-0123456789ab",
        &[
            ("values.yaml", "pack:\n  k8sHardening: true\n"),
            ("README.md", "# k8s\n"),
        ],
    );
    let packs = [
        DownloadPack {
            version: "1.36.2",
            archive: Some(archive),
            manifests: &[("extra", "kind: ConfigMap\n")],
            ..none("kubernetes", PackType::Spectro, false)
        },
        none("private-pack", PackType::Spectro, true),
        DownloadPack {
            manifests: &[("rules", "kind: ConfigMap\n")],
            ..none("platform-rules", PackType::Manifest, false)
        },
    ];
    let not_gzip = [DownloadPack {
        archive: Some(br#"{"code":"NotFound"}"#.to_vec()),
        ..none("kubernetes", PackType::Spectro, false)
    }];
    // an archive with no top folder, and a pack whose name holds the label of a pack
    let flat = [DownloadPack {
        archive: Some(tgz(&[(
            "pack.json".to_string(),
            br#"{"name":"flat"}"#.to_vec(),
        )])),
        ..none("flat", PackType::Spectro, false)
    }];
    let labeled = [DownloadPack {
        archive: Some(pack_archive(
            "csi--aws",
            "1.0.0",
            "csi",
            "csi--aws-0123456789ab",
            &[],
        )),
        ..none("csi--aws", PackType::Spectro, false)
    }];
    let chart = chart_file("nginx", "15.0.0", Some("1.27.0"));
    let charted = [DownloadPack {
        version: "15.0.0",
        archive: Some(chart_archive(
            "nginx",
            &[("Chart.yaml", &chart), ("values.yaml", "replicaCount: 1\n")],
        )),
        ..none("nginx", PackType::Helm, false)
    }];
    vec![
        profile_download(UID, "aws-stack", &packs),
        profile_download(UID, "aws-stack", &packs[2..]),
        profile_download(UID, "aws-stack", &flat),
        profile_download(UID, "aws-stack", &labeled),
        profile_download(UID, "aws-stack", &charted),
        profile_download(UID, "aws-stack", &not_gzip),
        tgz(&[(format!("{UID}/spc.json"), b"{}".to_vec())]),
        br#"{"Reader":{}}"#.to_vec(),
    ]
}
