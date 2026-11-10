//! The tests of `packs`: the cloud filter, the architectures, the latest marks, the digests, and the
//! tag requests.

use super::*;
use crate::api::MemoryApi;
use crate::model::cloud::CLOUD_ALL;

fn version(uid: &str, version: &str, clouds: &[&str]) -> Value {
    serde_json::json!({
        "metadata": {"uid": uid},
        "spec": {"name": "strata-live-dep", "version": version, "registryUid": "reg-own",
                 "layer": "addon", "type": "oci", "cloudTypes": clouds,
                 "digest": format!("sha256:{uid}")},
    })
}

fn packs_for(cloud: &str, versions: Vec<Value>) -> PackListing {
    let options = PackOptions {
        cloud: Some(cloud.to_string()),
        ..Default::default()
    };
    packs_with(&options, versions)
}

/// `packs strata-live-dep` with `options`, from a listing of `versions`.
fn packs_with(options: &PackOptions, versions: Vec<Value>) -> PackListing {
    read_with(options, versions).0
}

/// [`packs_with`], and the architectures that the listing names.
fn read_with(
    options: &PackOptions,
    versions: Vec<Value>,
) -> (PackListing, BTreeMap<String, Architectures>) {
    // Palette gives a tag row to each version that it serves
    let tags: Vec<Value> = versions
        .iter()
        .map(|v| serde_json::json!({"tag": v["spec"]["version"], "packUid": v["metadata"]["uid"]}))
        .collect();
    let api = MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [
                {"name": "strata-live-registry", "uid": "reg-own", "kind": "oci"},
            ]}),
        )
        .with(
            "/v1/packs?filters=spec.name=strata-live-dep&limit=100&offset=0",
            serde_json::json!({"items": versions}),
        )
        .with(
            "/v1/packs/strata-live-dep/registries/reg-own",
            serde_json::json!({"layer": "addon", "tags": tags}),
        );
    let palette = Palette::new(api);
    registry_packs_in(
        &mut Registries::new(&palette),
        "strata-live-dep",
        options,
        "2026-10-05T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap()
}

#[test]
fn a_version_with_the_cloud_type_all_supports_each_cloud_and_is_the_latest_when_it_is_the_newest() {
    let listing = packs_for(
        "aws",
        vec![
            version("pk-1", "1.0.0", &[CLOUD_ALL]),
            version("pk-0", "0.9.0", &["aws"]),
            version("pk-e", "1.1.0", &["edge-native"]),
        ],
    );
    let kept: Vec<(&str, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest))
        .collect();
    assert_eq!(kept, vec![("1.0.0", true), ("0.9.0", false)]);
    // the document keeps the cloud types as Palette gives them
    assert_eq!(
        listing.registries[0].versions[0].cloud_types,
        vec![CLOUD_ALL]
    );
}

#[test]
fn a_version_keeps_the_architectures_of_the_listing_sorted_and_the_document_does_not_show_them() {
    let mut both = version("pk-2", "2.0.0", &["aws"]);
    both["spec"]["annotations"] = serde_json::json!({"architecture": "arm64,amd64"});
    let (listing, architectures) = read_with(
        &PackOptions::default(),
        vec![both, version("pk-1", "1.0.0", &["aws"])],
    );
    let of = |version: &str| &architectures["reg-own"][version];
    assert_eq!(of("2.0.0"), &["amd64", "arm64"]);
    assert!(of("1.0.0").is_empty(), "the listing names none");
    // `packs` is not a catalog: the document omits the architectures
    let json = serde_json::to_value(&listing).unwrap();
    assert!(
        json["registries"][0]["versions"][0]
            .get("architectures")
            .is_none()
    );
}

/// A version of `strata-live-dep` with the mark of the listing that says whether Palette
/// calls it the latest.
fn marked(uid: &str, version_text: &str, palette_latest: bool) -> Value {
    let mut summary = version(uid, version_text, &["aws"]);
    summary["metadata"]["annotations"] =
        serde_json::json!({"latestVersion": palette_latest.to_string()});
    summary
}

#[test]
fn a_pre_release_is_below_its_version_so_latest_keeps_one_version() {
    let options = PackOptions {
        latest: true,
        ..Default::default()
    };
    let listing = packs_with(
        &options,
        vec![
            marked("pk-29", "1.29.0", false),
            marked("pk-30", "1.30.0", true),
            marked("pk-rc", "1.30.0-rc.1", false),
        ],
    );
    let kept: Vec<(&str, bool, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest, v.palette_latest))
        .collect();
    assert_eq!(kept, vec![("1.30.0", true, true)]);
    let all = packs_with(
        &PackOptions::default(),
        vec![
            marked("pk-rc", "1.30.0-rc.1", false),
            marked("pk-30", "1.30.0", true),
            marked("pk-29", "1.29.0", false),
        ],
    );
    let order: Vec<&str> = all.registries[0]
        .versions
        .iter()
        .map(|v| v.version.as_str())
        .collect();
    assert_eq!(order, ["1.30.0", "1.30.0-rc.1", "1.29.0"]);
}

/// The order of `packs` for a version that does not parse, from issue #616. `1.1.8.develop` reads
/// as `1.1.8`, below it, and `latest` skips it. `10.4.0.1` reads as `10.4.0`, above it. Issue #714:
/// `1.2.develop` holds no three numbers, so it is below each version, and `latest` skips it.
#[test]
fn packs_orders_a_version_that_does_not_parse_by_its_three_numbers_and_latest_skips_a_tail_of_text()
{
    let order = |versions: &[&str]| -> Vec<(String, bool)> {
        let summaries = versions
            .iter()
            .enumerate()
            .map(|(i, v)| version(&format!("pk-{i}"), v, &["aws"]))
            .collect();
        packs_with(&PackOptions::default(), summaries).registries[0]
            .versions
            .iter()
            .map(|v| (v.version.clone(), v.latest))
            .collect()
    };
    let owned = |pairs: &[(&str, bool)]| -> Vec<(String, bool)> {
        pairs.iter().map(|(v, l)| (v.to_string(), *l)).collect()
    };
    assert_eq!(
        order(&["1.1.8", "1.1.8.develop", "1.1.10"]),
        owned(&[("1.1.10", true), ("1.1.8", false), ("1.1.8.develop", false)])
    );
    assert_eq!(
        order(&["10.4.0.1", "10.5.0", "10.4.0"]),
        owned(&[("10.5.0", true), ("10.4.0.1", false), ("10.4.0", false)])
    );
    // a tail of text is a pre-release: the highest version, and still not the one of `latest`
    assert_eq!(
        order(&["1.1.10", "1.2.0.develop"]),
        owned(&[("1.2.0.develop", false), ("1.1.10", true)])
    );
    assert_eq!(
        order(&["1.2.develop", "1.10.0", "1.9.0"]),
        owned(&[("1.10.0", true), ("1.9.0", false), ("1.2.develop", false)])
    );
}

/// With `--cloud`, `latest` marks the highest version that is not a pre-release, of the versions
/// that support the cloud. A pre-release above the release has no mark (#608).
#[test]
fn latest_with_cloud_marks_the_highest_version_that_is_not_a_pre_release() {
    let listing = packs_for(
        "aws",
        vec![
            version("pk-rc", "1.31.0-rc.1", &["aws"]),
            version("pk-29", "1.29.0", &["aws"]),
            version("pk-gcp", "1.30.0", &["gcp"]),
        ],
    );
    let marks: Vec<(&str, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest))
        .collect();
    assert_eq!(marks, vec![("1.31.0-rc.1", false), ("1.29.0", true)]);
}

#[test]
fn latest_marks_the_version_that_a_pack_ref_records_and_not_a_pre_release_above_it() {
    // the highest version is a pre-release, and Palette marks it: `pack:...@latest` skips it
    let listing = packs_with(
        &PackOptions::default(),
        vec![
            marked("pk-29", "1.29.0", false),
            marked("pk-rc", "1.31.0-rc.1", true),
        ],
    );
    let marks: Vec<(&str, bool, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest, v.palette_latest))
        .collect();
    assert_eq!(
        marks,
        vec![("1.31.0-rc.1", false, true), ("1.29.0", true, false)]
    );
    let recorded = version::pick(["1.29.0", "1.31.0-rc.1"], |v| *v, LATEST);
    assert_eq!(recorded, Some("1.29.0"));

    // `--latest` keeps the version of `@latest` and the version of Palette
    let kept = packs_with(
        &PackOptions {
            latest: true,
            ..Default::default()
        },
        vec![
            marked("pk-29", "1.29.0", false),
            marked("pk-rc", "1.31.0-rc.1", true),
        ],
    );
    let names: Vec<&str> = kept.registries[0]
        .versions
        .iter()
        .map(|v| v.version.as_str())
        .collect();
    assert_eq!(names, ["1.31.0-rc.1", "1.29.0"]);

    // a registry with pre-releases alone: the highest pre-release is the latest, as for a ref
    let only = packs_with(
        &PackOptions::default(),
        vec![
            marked("pk-1", "1.0.0-rc.1", false),
            marked("pk-2", "1.0.0-rc.2", true),
        ],
    );
    let latest: Vec<&str> = only.registries[0]
        .versions
        .iter()
        .filter(|v| v.latest)
        .map(|v| v.version.as_str())
        .collect();
    assert_eq!(latest, ["1.0.0-rc.2"]);
}

/// Review of Wave 14, D4. `--cloud` cuts the list before the mark, and `pack:<name>@latest` picks
/// from each version of the registry. So with `--cloud`, the mark and the ref can name two
/// versions. The book says so.
#[test]
fn latest_with_cloud_marks_the_highest_version_of_that_cloud_and_the_ref_records_another() {
    let versions = || {
        vec![
            version("pk-2", "2.0.0", &["aws"]),
            version("pk-1", "1.0.0", &["gcp"]),
        ]
    };
    let options = PackOptions {
        cloud: Some("gcp".to_string()),
        latest: true,
        ..Default::default()
    };
    let kept = packs_with(&options, versions());
    let marks: Vec<(&str, bool)> = kept.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest))
        .collect();
    assert_eq!(marks, [("1.0.0", true)]);
    // the ref has no cloud: it picks from each version of the registry
    assert_eq!(
        version::pick(["2.0.0", "1.0.0"], |v| *v, LATEST),
        Some("2.0.0")
    );

    // with no `--cloud`, the mark is on the version that the ref records
    let all = packs_with(&PackOptions::default(), versions());
    let marked: Vec<&str> = all.registries[0]
        .versions
        .iter()
        .filter(|v| v.latest)
        .map(|v| v.version.as_str())
        .collect();
    assert_eq!(marked, ["2.0.0"]);
}

/// Review of Wave 14, A7. `packs` omits a version with no tag, so the mark is on the highest
/// version that the registry serves. `pack:<name>@latest` picks from the same list: see the test
/// of `picked_in` in `candidate::pack`.
#[test]
fn latest_marks_the_highest_version_that_has_a_tag() {
    let api = nginx_api(
        vec![nginx("pk-30", "1.30.0", ""), nginx("pk-29", "1.29.0", "")],
        &[tag("pk-29", "1.29.0", "1.29.0")],
        &[],
    );
    let registry = nginx_packs(&api);
    let marks: Vec<(&str, bool)> = registry
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest))
        .collect();
    assert_eq!(marks, [("1.29.0", true)]);
}

/// Review of Wave 14, A7. The words said that the marks differ when the registry has another
/// version. With two pre-releases the registry has another version, and the marks are on one
/// version. They differ when the registry has a version that is not a pre-release.
#[test]
fn the_marks_agree_for_two_pre_releases_and_differ_for_a_release_below_a_pre_release() {
    let marks = |versions: Vec<Value>| -> Vec<(String, bool, bool)> {
        packs_with(&PackOptions::default(), versions).registries[0]
            .versions
            .iter()
            .map(|v| (v.version.clone(), v.latest, v.palette_latest))
            .collect()
    };
    assert_eq!(
        marks(vec![
            marked("pk-1", "1.0.0-rc.1", false),
            marked("pk-2", "1.0.0-rc.2", true),
        ]),
        [
            ("1.0.0-rc.2".to_string(), true, true),
            ("1.0.0-rc.1".to_string(), false, false),
        ]
    );
    assert_eq!(
        marks(vec![
            marked("pk-1", "1.0.0", false),
            marked("pk-2", "1.1.0-rc.1", true),
        ]),
        [
            ("1.1.0-rc.1".to_string(), false, true),
            ("1.0.0".to_string(), true, false),
        ]
    );
}

#[test]
fn latest_keeps_the_version_of_palette_when_it_is_not_the_newest_and_names_it() {
    // Palette reads a version with four numbers as its first three, so it reads `1.30.0.1` and
    // `1.30.0` as one version, and it marks one of them. Strata puts `1.30.0.1` above `1.30.0`
    let options = PackOptions {
        latest: true,
        ..Default::default()
    };
    let listing = packs_with(
        &options,
        vec![
            marked("pk-29", "1.29.0", false),
            marked("pk-30", "1.30.0", true),
            marked("pk-four", "1.30.0.1", false),
        ],
    );
    let kept: Vec<(&str, bool, bool)> = listing.registries[0]
        .versions
        .iter()
        .map(|v| (v.version.as_str(), v.latest, v.palette_latest))
        .collect();
    assert_eq!(
        kept,
        vec![("1.30.0.1", true, false), ("1.30.0", false, true)]
    );
    let json = serde_json::to_value(&listing).unwrap();
    assert_eq!(json["registries"][0]["versions"][1]["paletteLatest"], true);
    assert!(
        json["registries"][0]["versions"][0]
            .get("paletteLatest")
            .is_none()
    );

    // the terminal names both: the newest first, and the mark on the version of Palette
    let text = crate::render::renderer(
        crate::render::Format::Terminal,
        crate::term::color::Paint::off(),
        false,
    )
    .render_packs(&listing)
    .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "strata-live-dep in strata-live-registry (2 of 3 versions)"
    );
    assert!(
        lines[1].starts_with("  1.30.0.1 ") && !lines[1].contains("Palette"),
        "{text}"
    );
    assert!(
        lines[2].starts_with("  1.30.0 ") && lines[2].ends_with("latest in Palette"),
        "{text}"
    );
}

#[test]
fn a_version_that_strata_and_palette_both_call_the_latest_has_no_mark() {
    let listing = packs_with(
        &PackOptions {
            latest: true,
            ..Default::default()
        },
        vec![
            marked("pk-30", "1.30.0", true),
            marked("pk-29", "1.29.0", false),
        ],
    );
    let v = &listing.registries[0].versions;
    assert_eq!(v.len(), 1);
    assert!(v[0].latest && v[0].palette_latest);
    let text = crate::render::renderer(
        crate::render::Format::Terminal,
        crate::term::color::Paint::off(),
        false,
    )
    .render_packs(&listing)
    .unwrap();
    assert!(!text.contains("Palette"), "{text}");
}

/// A version of `strata-live-dep` for which the registry reports no digest.
fn no_digest(uid: &str, version_text: &str) -> Value {
    let mut summary = version(uid, version_text, &["aws"]);
    summary["spec"]["digest"] = serde_json::json!("");
    summary
}

/// `packs` of `listing` in the terminal and in markdown, with no color.
fn rendered(listing: &PackListing) -> (String, String) {
    let render = |format| {
        crate::render::renderer(format, crate::term::color::Paint::off(), false)
            .render_packs(listing)
            .unwrap()
    };
    (
        render(crate::render::Format::Terminal),
        render(crate::render::Format::Markdown),
    )
}

#[test]
fn a_version_with_no_digest_says_so_on_its_row_and_the_heading_does_not() {
    let listing = packs_with(
        &PackOptions::default(),
        vec![
            version("pk-2", "1.0.2", &["aws"]),
            no_digest("pk-1", "1.0.1"),
        ],
    );
    assert!(!listing.registries[0].no_digests);
    let (terminal, markdown) = rendered(&listing);
    let lines: Vec<&str> = terminal.lines().collect();
    assert_eq!(
        lines[0],
        "strata-live-dep in strata-live-registry (2 versions)"
    );
    assert!(
        lines[1].starts_with("  1.0.2 ") && !lines[1].contains("digest"),
        "{terminal}"
    );
    assert!(
        lines[2].starts_with("  1.0.1 ") && lines[2].ends_with("no digest"),
        "{terminal}"
    );
    // the markdown states the count of the heading too
    assert!(
        markdown.starts_with(&format!("### {}\n", lines[0])),
        "{markdown}"
    );
    assert!(markdown.contains("| `1.0.1` no digest |"), "{markdown}");
    assert!(!markdown.contains("| `1.0.2` no digest"), "{markdown}");
}

#[test]
fn a_registry_with_no_digest_for_each_version_says_no_digests_in_the_heading_also_with_latest() {
    let listing = packs_with(
        &PackOptions {
            latest: true,
            ..Default::default()
        },
        vec![no_digest("pk-2", "1.0.2"), no_digest("pk-1", "1.0.1")],
    );
    let json = serde_json::to_value(&listing).unwrap();
    assert_eq!(json["registries"][0]["noDigests"], true);
    let (terminal, markdown) = rendered(&listing);
    let lines: Vec<&str> = terminal.lines().collect();
    assert_eq!(
        lines[0],
        "strata-live-dep in strata-live-registry (1 of 2 versions, no digests)"
    );
    assert!(
        lines[1].starts_with("  1.0.2 ") && lines[1].ends_with("no digest"),
        "{terminal}"
    );
    // the markdown says too that a flag cut the list
    assert!(
        markdown.starts_with(&format!("### {}\n", lines[0])),
        "{markdown}"
    );
}

/// The path of the tags request with the states query, for `nginx` in the Public Repo.
const RETIRED_PATH: &str =
    "/v1/packs/nginx/registries/reg-public?states=deprecated,disabled,deleted";

/// A version of `nginx` in the Public Repo, in `state` when it is not empty.
fn nginx(uid: &str, version: &str, state: &str) -> Value {
    let mut summary = serde_json::json!({
        "metadata": {"uid": uid},
        "spec": {"name": "nginx", "version": version, "registryUid": "reg-public",
                 "layer": "addon", "type": "spectro", "cloudTypes": ["all"]},
    });
    if !state.is_empty() {
        summary["spec"]["annotations"] = serde_json::json!({"system_state": state});
    }
    summary
}

/// One tag row of a tags answer.
fn tag(uid: &str, version: &str, tag: &str) -> Value {
    serde_json::json!({"packUid": uid, "version": version, "tag": tag})
}

/// An API with the `versions` of `nginx` in its listing, and the two tags answers of the
/// Public Repo. These are `plain` with no query, and `retired` with the states query. An empty
/// answer is a 404.
fn nginx_api(versions: Vec<Value>, plain: &[Value], retired: &[Value]) -> MemoryApi {
    let mut api = MemoryApi::new()
        .with(
            "/v1/registries/metadata",
            serde_json::json!({"items": [
                {"name": "Public Repo", "uid": "reg-public", "kind": "pack"},
            ]}),
        )
        .with(
            "/v1/packs?filters=spec.name=nginx&limit=100&offset=0",
            serde_json::json!({"items": versions}),
        );
    if !plain.is_empty() {
        api = api.with(
            "/v1/packs/nginx/registries/reg-public",
            serde_json::json!({"layer": "addon", "tags": plain}),
        );
    }
    if !retired.is_empty() {
        api = api.with(
            RETIRED_PATH,
            serde_json::json!({"layer": "addon", "tags": retired}),
        );
    }
    api
}

/// The one registry that `packs nginx` lists from `api`.
fn nginx_packs(api: &MemoryApi) -> PackRegistry {
    let listing = registry_packs(
        "nginx",
        &PackOptions::default(),
        |_| Ok(Palette::new(api)),
        "2026-10-05T00:00:00Z",
        &Progress::silent(),
    )
    .unwrap();
    listing.registries.into_iter().next().unwrap()
}

/// The tags of `version` in `registry`, in their order.
fn tags_of<'a>(registry: &'a PackRegistry, version: &str) -> Vec<&'a str> {
    registry
        .versions
        .iter()
        .find(|v| v.version == version)
        .map(|v| v.tags.iter().map(String::as_str).collect())
        .unwrap_or_default()
}

#[test]
fn a_registry_whose_versions_are_all_disabled_gets_its_tags_from_the_states_query() {
    let api = nginx_api(
        vec![
            nginx("pk-43", "0.43.0", "disabled"),
            nginx("pk-26", "0.26.1", "disabled"),
        ],
        &[],
        &[
            tag("pk-43", "0.43.0", "0.43.0"),
            tag("pk-43", "0.43.0", "0.43.x"),
            tag("pk-26", "0.26.1", "0.26.1"),
            tag("pk-26", "0.26.1", "0.26.x"),
        ],
    );
    let registry = nginx_packs(&api);
    assert_eq!(api.calls(RETIRED_PATH), 1);
    assert!(!registry.tags_unavailable);
    assert_eq!(registry.state, Some(PackState::Disabled.into()));
    assert_eq!(tags_of(&registry, "0.26.1"), ["0.26.1", "0.26.x"]);
    assert_eq!(tags_of(&registry, "0.43.0"), ["0.43.0", "0.43.x"]);
}

#[test]
fn a_registry_with_a_deleted_version_joins_both_answers_and_keeps_each_tag_row_one_time() {
    // the answer with no query holds the active and the deprecated versions; the answer with
    // the states query holds the deprecated and the deleted versions
    let api = nginx_api(
        vec![
            nginx("pk-3", "1.3.0", ""),
            nginx("pk-2", "1.2.0", "deprecated"),
            nginx("pk-1", "1.1.0", "deleted"),
        ],
        &[tag("pk-3", "1.3.0", "1.3.0"), tag("pk-2", "1.2.0", "1.2.0")],
        &[tag("pk-2", "1.2.0", "1.2.0"), tag("pk-1", "1.1.0", "1.1.0")],
    );
    let registry = nginx_packs(&api);
    assert_eq!(api.calls(RETIRED_PATH), 1);
    assert_eq!(tags_of(&registry, "1.3.0"), ["1.3.0"]);
    assert_eq!(tags_of(&registry, "1.2.0"), ["1.2.0"]);
    assert_eq!(tags_of(&registry, "1.1.0"), ["1.1.0"]);
}

#[test]
fn a_registry_with_no_disabled_or_deleted_version_sends_no_states_query() {
    let api = nginx_api(
        vec![
            nginx("pk-3", "1.3.0", ""),
            nginx("pk-2", "1.2.0", "deprecated"),
        ],
        &[tag("pk-3", "1.3.0", "1.3.0"), tag("pk-2", "1.2.0", "1.2.0")],
        &[],
    );
    let registry = nginx_packs(&api);
    assert_eq!(api.calls(RETIRED_PATH), 0);
    assert_eq!(tags_of(&registry, "1.2.0"), ["1.2.0"]);
}

#[test]
fn a_registry_that_answers_404_with_and_without_the_states_query_has_no_tags() {
    let api = nginx_api(vec![nginx("pk-26", "0.26.1", "disabled")], &[], &[]);
    let registry = nginx_packs(&api);
    assert_eq!(api.calls(RETIRED_PATH), 1);
    assert!(registry.tags_unavailable);
    assert!(tags_of(&registry, "0.26.1").is_empty());
}
