//! The tests of the download: the files of two builds, the packs with no archive, a pack file that
//! is no archive or that the download does not hold and its reason, and the refusals of the whole
//! download.

use super::written::{
    DownloadPack, archive_top, chart_archive, chart_file, pack_archive, profile_download,
    profile_download_without, tgz,
};
use super::*;
use crate::api::download_path;

const UID: &str = "64f0000000000000000007a1";

/// A pack of a pack registry, with the archive of its build at `version` under `top`, and
/// `files` beside `pack.json`.
fn built<'a>(
    name: &'a str,
    version: &'a str,
    top: &str,
    files: &[(&str, &str)],
) -> DownloadPack<'a> {
    DownloadPack {
        name,
        version,
        kind: PackType::Spectro,
        private: false,
        archive: Some(pack_archive(name, version, "k8s", top, files)),
        manifests: &[],
    }
}

/// The download of `packs`, read.
fn read(packs: &[DownloadPack]) -> ProfileDownload {
    ProfileDownload::read(
        &download_path(UID),
        &profile_download(UID, "aws-stack", packs),
    )
    .unwrap()
}

/// The message of the error of the read of `file`.
fn refusal(file: &[u8]) -> String {
    match ProfileDownload::read(&download_path(UID), file) {
        Err(Error::NotArchive { path, message, .. }) => {
            assert_eq!(path, download_path(UID));
            message
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_builds_of_one_content_differ_only_by_the_version_in_pack_json() {
    let values = [
        ("values.yaml", "pack:\n  k8sHardening: true\n"),
        ("README.md", "# k8s\n"),
    ];
    let a = read(&[built(
        "kubernetes",
        "1.36.2-rc.1",
        "kubernetes-0123456789ab",
        &values,
    )]);
    let b = read(&[built(
        "kubernetes",
        "1.36.2",
        "kubernetes-ba9876543210",
        &values,
    )]);
    let (a, b) = (a.pack("kubernetes").unwrap(), b.pack("kubernetes").unwrap());
    assert_eq!(
        (a.version.as_str(), b.version.as_str()),
        ("1.36.2-rc.1", "1.36.2")
    );
    let (a, b) = (a.archive().unwrap(), b.archive().unwrap());
    // the top folder differs between the two builds, so the paths are those under it
    let paths: Vec<&str> = a.files().map(|(path, _)| path).collect();
    assert_eq!(paths, ["README.md", "pack.json", "values.yaml"]);
    assert!(a.changed_files(b).is_empty(), "{a:?} {b:?}");
    assert_eq!(a, b);
    // an OCI registry of packs names the top folder by the name and the version of the pack
    let oci = |version: &str| {
        let top = archive_top(PackType::Oci, "kubernetes", version, "");
        let download = read(&[built("kubernetes", version, &top, &values)]);
        download
            .pack("kubernetes")
            .unwrap()
            .archive()
            .cloned()
            .unwrap()
    };
    let (c, d) = (oci("1.36.2-rc.1"), oci("1.36.2"));
    assert!(c.changed_files(&d).is_empty(), "{c:?} {d:?}");
    assert_eq!(&c, a);
}

#[test]
fn each_file_that_differs_in_two_builds_is_a_changed_file() {
    let a = read(&[built(
        "kubernetes",
        "1.36.2-rc.1",
        "kubernetes-0123456789ab",
        &[("values.yaml", "a: 1\n"), ("charts/k8s.tgz", "one")],
    )]);
    let b = read(&[built(
        "kubernetes",
        "1.36.2",
        "kubernetes-ba9876543210",
        &[("values.yaml", "a: 2\n"), ("presets.yaml", "b: 1\n")],
    )]);
    let (a, b) = (
        a.pack("kubernetes").unwrap().archive(),
        b.pack("kubernetes").unwrap().archive(),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(
        a.changed_files(b),
        ["charts/k8s.tgz", "presets.yaml", "values.yaml"]
    );
    assert_eq!(b.changed_files(a), a.changed_files(b));
}

#[test]
fn pack_json_with_another_field_is_a_changed_file() {
    let pack = |layer: &str| {
        let archive = pack_archive(
            "csi-aws-ebs",
            "1.65.0",
            layer,
            "csi-aws-ebs-0123456789ab",
            &[],
        );
        read(&[DownloadPack {
            archive: Some(archive),
            ..built("csi-aws-ebs", "1.65.0", "csi-aws-ebs-0123456789ab", &[])
        }])
    };
    let (a, b) = (pack("csi"), pack("storage"));
    let a = a.pack("csi-aws-ebs").unwrap().archive().unwrap();
    let b = b.pack("csi-aws-ebs").unwrap().archive().unwrap();
    assert_eq!(a.changed_files(b), ["pack.json"]);
}

/// The archive of the chart `nginx` of a Helm registry at `version`, read from a download. Its
/// files are `files`, under the top folder `nginx`.
fn chart(version: &str, files: &[(&str, &str)]) -> PackArchive {
    let download = read(&[DownloadPack {
        name: "nginx",
        version,
        kind: PackType::Helm,
        private: false,
        archive: Some(chart_archive("nginx", files)),
        manifests: &[],
    }]);
    download.pack("nginx").unwrap().archive().cloned().unwrap()
}

#[test]
fn two_builds_of_one_chart_differ_only_by_the_line_version_of_chart_yaml() {
    let values = ("values.yaml", "replicaCount: 1\n");
    let file = |version: &str, app: Option<&str>| chart_file("nginx", version, app);
    let a = chart(
        "15.0.0-rc.1",
        &[("Chart.yaml", &file("15.0.0-rc.1", Some("1.27.0"))), values],
    );
    let b = chart(
        "15.0.0",
        &[("Chart.yaml", &file("15.0.0", Some("1.27.0"))), values],
    );
    let paths: Vec<&str> = a.files().map(|(path, _)| path).collect();
    assert_eq!(paths, ["Chart.yaml", "values.yaml"]);
    assert!(a.changed_files(&b).is_empty(), "{a:?} {b:?}");
    assert_eq!(a, b);
    // each other line of Chart.yaml counts: the version of the app, a comment, and the version
    // of a dependency, which has an indent
    let top = file("15.0.0", Some("1.27.0"));
    let dependency =
        |version: &str| format!("{top}dependencies:\n- name: common\n  version: {version}\n");
    for (one, other) in [
        (top.clone(), file("15.0.0", Some("1.28.0"))),
        (top.clone(), file("15.0.0", None)),
        (top.clone(), format!("{top}# the chart of the web tier\n")),
        (dependency("2.0.0"), dependency("2.1.0")),
    ] {
        let one = chart("15.0.0", &[("Chart.yaml", &one), values]);
        let other = chart("15.0.0", &[("Chart.yaml", &other), values]);
        assert_eq!(one.changed_files(&other), ["Chart.yaml"], "{other:?}");
    }
    // the Chart.yaml of a chart in the chart keeps its version
    let sub = |version: &str| {
        let sub = chart_file("common", version, None);
        chart(
            "15.0.0",
            &[("Chart.yaml", &top), ("charts/common/Chart.yaml", &sub)],
        )
    };
    assert_eq!(
        sub("2.0.0").changed_files(&sub("2.1.0")),
        ["charts/common/Chart.yaml"]
    );
}

#[test]
fn a_pack_with_no_archive_in_the_download_is_listed_with_none_and_a_manifest_pack_is_not_listed() {
    let none = |name: &'static str, kind: PackType, private: bool| DownloadPack {
        name,
        version: "1.0.0",
        kind,
        private,
        archive: None,
        manifests: &[],
    };
    let download = read(&[
        none("private-pack", PackType::Spectro, true),
        none("private-chart", PackType::Helm, true),
        none("zarf-package", PackType::Oci, false),
        DownloadPack {
            manifests: &[("extra-rules", "kind: ConfigMap\n")],
            ..none("platform-rules", PackType::Manifest, false)
        },
        DownloadPack {
            manifests: &[("extra", "kind: ConfigMap\n")],
            ..built("kubernetes", "1.36.2", "kubernetes-0123456789ab", &[])
        },
    ]);
    for name in ["private-pack", "private-chart", "zarf-package"] {
        assert_eq!(
            download.pack(name).unwrap().file,
            PackFile::NoArchive,
            "{name}"
        );
    }
    assert!(download.pack("platform-rules").is_none());
    // a manifest of a pack with an archive is no file of the archive
    let names: Vec<&str> = download.packs().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "kubernetes",
            "private-chart",
            "private-pack",
            "zarf-package"
        ]
    );
    let k8s = download.pack("kubernetes").unwrap().archive().unwrap();
    assert_eq!(k8s.files().count(), 1);
}

#[test]
fn a_file_that_is_not_a_download_is_refused_with_what_strata_cannot_read() {
    assert!(!refusal(b"{\"Reader\":{}}").is_empty());
    let no_metadata = tgz(&[(format!("{UID}/spc.json"), b"{}".to_vec())]);
    assert_eq!(refusal(&no_metadata), "no metadata.json at the top");
    let bad = tgz(&[(format!("{UID}/metadata.json"), b"no json".to_vec())]);
    assert!(refusal(&bad).starts_with(&format!("{UID}/metadata.json is not JSON")));
    let no_list = tgz(&[(
        format!("{UID}/metadata.json"),
        br#"{"profiles":[]}"#.to_vec(),
    )]);
    assert_eq!(
        refusal(&no_list),
        "metadata.json names 0 lists of packs, and not one"
    );
}

/// The criterion of #833: the list names the file of a pack that the download does not hold.
/// Strata gives that pack no archive and a reason that names the path, and the other packs keep
/// their archives. Before, strata refused the whole download.
#[test]
fn a_pack_file_that_the_download_does_not_hold_gets_a_reason_and_the_other_packs_keep_their_archives()
 {
    let packs = [
        built(
            "kubernetes",
            "1.36.2",
            "kubernetes-0123456789ab",
            &[("values.yaml", "a: 1\n")],
        ),
        chart_file_of(&chart_archive("nginx", &[("values.yaml", "a: 1\n")])),
    ];
    let file = profile_download_without(UID, "aws-stack", &packs, &["nginx"]);
    let download = ProfileDownload::read(&download_path(UID), &file).unwrap();
    let k8s = download.pack("kubernetes").unwrap().archive().unwrap();
    let paths: Vec<&str> = k8s.files().map(|(path, _)| path).collect();
    assert_eq!(paths, ["pack.json", "values.yaml"]);
    let nginx = download.pack("nginx").unwrap();
    assert_eq!(nginx.version, "25.1.5");
    assert_eq!(nginx.archive(), None);
    assert_eq!(
        nginx.file,
        PackFile::NotReadable(
            "the download does not hold packs/nginx/nginx-25.1.5.tgz".to_string()
        )
    );
}

/// The chart `nginx` of a Helm registry whose file in the download is `file`.
fn chart_file_of(file: &[u8]) -> DownloadPack<'static> {
    DownloadPack {
        name: "nginx",
        version: "25.1.5",
        kind: PackType::Helm,
        private: false,
        archive: Some(file.to_vec()),
        manifests: &[],
    }
}

/// The criterion of #828: the download holds the error answer of the host of a chart in place
/// of the chart. Strata keeps the other packs with their archives, and gives the chart no
/// archive and a reason.
#[test]
fn a_pack_file_that_is_not_an_archive_gets_a_reason_and_the_other_packs_keep_their_archives() {
    let download = read(&[
        built(
            "kubernetes",
            "1.36.2",
            "kubernetes-0123456789ab",
            &[("values.yaml", "a: 1\n")],
        ),
        chart_file_of(written::CHART_HOST_REFUSAL.as_bytes()),
    ]);
    let k8s = download.pack("kubernetes").unwrap().archive().unwrap();
    let paths: Vec<&str> = k8s.files().map(|(path, _)| path).collect();
    assert_eq!(paths, ["pack.json", "values.yaml"]);
    let nginx = download.pack("nginx").unwrap();
    assert_eq!(nginx.version, "25.1.5");
    assert_eq!(nginx.archive(), None);
    assert_eq!(
        nginx.file,
        PackFile::NotReadable(
            "263 bytes of text: <?xml version=\"1.0\" encoding=\"UTF-8\"?> \
             <Error><Code>AccessDenied</Code><Message>Access Denied</Message><RequestId>4V6Q3…"
                .to_string()
        )
    );
}

/// A file that is not text gets its size and the error of the read. A file with a control
/// character other than white space is not text, so no escape of a terminal reaches the reason.
#[test]
fn a_pack_file_that_is_not_text_gets_its_size_and_the_error_of_the_read() {
    for file in [&b"\xff\xfe\x00\x01 not an archive"[..], b"\x1b[2J<Error/>"] {
        let download = read(&[chart_file_of(file)]);
        let PackFile::NotReadable(reason) = &download.pack("nginx").unwrap().file else {
            panic!("{download:?}");
        };
        let size = format!("{} bytes: ", file.len());
        assert!(reason.starts_with(&size), "{reason}");
        assert!(reason.len() > size.len(), "{reason}");
        assert!(!reason.chars().any(char::is_control), "{reason:?}");
    }
    let download = read(&[chart_file_of(b"x")]);
    let PackFile::NotReadable(reason) = &download.pack("nginx").unwrap().file else {
        panic!("{download:?}");
    };
    assert_eq!(reason, "1 byte of text: x");
}

#[test]
fn the_text_of_a_reason_is_one_line_of_120_characters_at_most_with_no_control_character() {
    assert_eq!(one_line(" a\n\tb\u{1b} c\r\n"), "a b c");
    assert_eq!(one_line("\n \t"), "");
    // a cut falls between two characters, also in a character of more than one byte
    let long = "é".repeat(REASON_TEXT + 1);
    let line = one_line(&long);
    assert_eq!(line, format!("{}…", "é".repeat(REASON_TEXT - 1)));
    assert_eq!(one_line(&"é".repeat(REASON_TEXT)), "é".repeat(REASON_TEXT));
}

/// The budget covers the files of each pack archive too. A pack archive that holds more bytes
/// than the budget has left refuses the whole download, and gets no reason. The message names
/// the limit of the read (#833).
#[test]
fn a_pack_archive_over_the_budget_refuses_the_whole_download() {
    let large = "a".repeat(100_000);
    let file = profile_download(
        UID,
        "aws-stack",
        &[built(
            "kubernetes",
            "1.36.2",
            "kubernetes-0123456789ab",
            &[("values.yaml", &large)],
        )],
    );
    assert!(file.len() < 50_000, "the files compress well");
    let message = read_download(&file, 50_000).unwrap_err();
    assert_eq!(
        message,
        "packs/kubernetes/kubernetes-1.36.2.tar.gz: the files hold more than 50000 bytes"
    );
    assert!(read_download(&file, UNPACKED_LIMIT).is_ok());
}

/// The fault names the limit of the read: in MiB when the limit is whole MiB, else in bytes.
#[test]
fn the_budget_refuses_more_bytes_than_it_has_and_names_its_limit() {
    let mut budget = Budget::new(10);
    assert!(budget.take(6).is_ok());
    assert_eq!(budget.take(5), Err(Fault::Limit(10)));
    for (limit, message) in [
        (UNPACKED_LIMIT, "the files hold more than 1024 MiB"),
        (2 * MIB, "the files hold more than 2 MiB"),
        (10, "the files hold more than 10 bytes"),
        (1, "the files hold more than 1 byte"),
    ] {
        assert_eq!(Fault::Limit(limit).to_string(), message);
    }
}
