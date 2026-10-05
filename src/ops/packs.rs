//! `packs`: the versions a registry offers for a pack.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::progress::Progress;
use crate::snapshot::Tool;
use crate::version;

use super::text;

/// The schema id of the document `packs` writes.
pub const PACKS_SCHEMA: &str = "strata-packs/1";

/// The document `packs` writes: `strata-packs/1`. One entry per registry that has the pack, and
/// one row per version, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackListing {
    /// The schema id of this document
    #[schemars(extend("const" = PACKS_SCHEMA))]
    pub schema: String,
    /// Which tool wrote the document.
    pub tool: Tool,
    /// When, RFC 3339 UTC.
    pub generated_at: String,
    /// The pack that was asked for.
    pub name: String,
    /// With `--cloud`: the cloud the versions were kept for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<String>,
    /// The registries that have the pack, in the order Palette lists them.
    pub registries: Vec<PackRegistry>,
}

/// One registry's copy of the pack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackRegistry {
    /// The registry name.
    pub registry: String,
    /// Its uid.
    pub registry_uid: String,
    /// The layer the pack belongs to, as the registry gives it.
    pub layer: String,
    /// The versions, newest first. `--latest` keeps one of them.
    pub versions: Vec<PackBuild>,
    /// How many versions of the pack the registry holds, before `--latest` or `--cloud` cut the
    /// list. A heading states the registry's inventory, and not the count of rows a flag left.
    pub versions_offered: usize,
    /// True when Palette listed the pack under this registry and then answered 404 for the tags
    /// of the registry, also with the states query: the versions come from the listing alone,
    /// with no tags.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tags_unavailable: bool,
    /// The state that every version of the pack in this registry has, when they all have the
    /// same one: `deprecated`, `disabled`, or `deleted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// One version of a pack in one registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackBuild {
    /// The version.
    pub version: String,
    /// The pack uid of this build.
    pub pack_uid: String,
    /// The digest the registry reports, empty when it reports none.
    pub digest: String,
    /// The pack type: spectro, helm, oci, or manifest.
    #[serde(rename = "type")]
    pub kind: String,
    /// The clouds this version supports.
    pub cloud_types: Vec<String>,
    /// Every tag that points at this version, the exact tag first.
    pub tags: Vec<String>,
    /// True for the newest version of the registry's copy, by the order of strata.
    pub latest: bool,
    /// True for the version that Palette calls the latest of the registry's copy. Palette orders
    /// by semantic version, so it can mark a version below the newest: `1.30.0` below
    /// `1.30.0-rc.1`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_latest: bool,
    /// The state that the registry listing gives this version: `deprecated`, `disabled`, or
    /// `deleted`. Palette serves no disabled or deleted pack, and refuses a profile that pins one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// When Palette first listed this version, `metadata.creationTimestamp` of the listing; empty
    /// when the listing gives no time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub listed: String,
}

/// What `packs` was asked for.
#[derive(Debug, Clone, Default)]
pub struct PackOptions {
    /// Only this registry, by name or uid.
    pub registry: Option<String>,
    /// Only the versions that support this cloud. A version with the cloud type `all` supports
    /// each cloud.
    pub cloud: Option<String>,
    /// Only the newest version of each registry, and the version that Palette calls the latest
    /// when it is a different one.
    pub latest: bool,
}

/// True when a version with the cloud types `cloud_types` supports `cloud`: its cloud types hold
/// that cloud, or `all`. Palette gives `all` to a pack that supports each cloud, and its filter on
/// one cloud keeps that pack.
pub(super) fn supports(cloud_types: &[String], cloud: &str) -> bool {
    cloud_types.iter().any(|c| c == cloud || c == "all")
}

/// The annotation of the pack listing that marks the version that Palette calls the latest. At
/// the sync of a registry, Palette gives `"true"` to one version of each pack, by the order of
/// semantic versions, and `"false"` to each other version.
const PALETTE_LATEST: &str = "/metadata/annotations/latestVersion";

/// The value of the `states` query that asks Palette for the tags of the retired versions of a
/// pack. With it, Palette answers with the versions in these states only.
const RETIRED_STATES: &str = "deprecated,disabled,deleted";

/// Read one answer of a registry for the tags of a pack: none when Palette answers 404.
fn tags_answer<A: PaletteApi>(palette: &Palette<A>, path: &str) -> Result<Option<Value>> {
    match palette.api().get_json(path) {
        Ok(answer) => Ok(Some(answer)),
        Err(Error::Api { status: 404, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Join the tag rows of the `answers` of one registry, and keep each row one time.
///
/// A deprecated version is in the answer with no query and in the answer with the states query,
/// so its rows come two times.
fn merged_tags(answers: &[Value]) -> Vec<Value> {
    let mut seen = std::collections::BTreeSet::new();
    answers
        .iter()
        .filter_map(|a| a.get("tags").and_then(Value::as_array))
        .flatten()
        .filter(|t| seen.insert((text(t, "/packUid"), text(t, "/version"), text(t, "/tag"))))
        .cloned()
        .collect()
}

/// List what the registries offer for one pack.
///
/// Reads the paged listing for the versions, their digests and their types, and each registry's
/// own answer for the tags that point at a version. A registry that does not have the pack is not
/// in the answer; no registry with it at all is `pack-not-found`.
pub fn registry_packs<A, F>(
    name: &str,
    options: &PackOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<PackListing>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let palette = connect(None)?;
    let visible = crate::candidate::registries(&palette)?;
    let wanted = match &options.registry {
        Some(r) => {
            let found = visible
                .iter()
                .find(|(n, u)| n == r || u == r)
                .ok_or_else(|| Error::RegistryNotFound {
                    registry: r.to_string(),
                    available: visible.iter().map(|(n, _)| n.clone()).collect(),
                })?;
            vec![found.clone()]
        }
        None => visible.clone(),
    };
    progress.note(&format!("reading the versions of {name}"));
    let entries = crate::candidate::versions_of(&palette, name)?;

    let mut registries = Vec::new();
    let mut had_the_pack = false;
    for (registry, uid) in wanted {
        let mut versions: Vec<PackBuild> = Vec::new();
        let mine: Vec<&Value> = entries
            .iter()
            .filter(|e| text(e, "/spec/registryUid") == uid)
            .collect();
        if mine.is_empty() {
            continue;
        }
        had_the_pack = true;
        // every version of this pack that the registry holds, counted before --cloud and --latest
        // cut the list: the heading states the inventory, not the rows that survived a flag
        let versions_offered = mine
            .iter()
            .filter_map(|e| Some(text(e, "/spec/version")).filter(|v| !v.is_empty()))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        // one state for the registry, when every version of the pack in it has the same one
        let states: std::collections::BTreeSet<Option<String>> = mine
            .iter()
            .map(|e| crate::candidate::pack_state(e))
            .collect();
        let state = match states.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(one)] => Some(one.clone()),
            _ => None,
        };
        // the registry's own answer holds the tags, which the listing does not. With no query,
        // the answer does not include the disabled and the deleted versions, and Palette answers
        // 404 when it has no version. So a registry with a retired version gets a second request
        // with the states query. A registry that answers 404 to both keeps the versions of the
        // listing, with no tags, and the other registries still answer
        progress.note(&format!("reading the tags of {name} in {registry}"));
        let path = format!("/v1/packs/{name}/registries/{uid}");
        let mut answers = vec![tags_answer(&palette, &path)?];
        if mine
            .iter()
            .any(|e| crate::candidate::retired_state(e).is_some())
        {
            answers.push(tags_answer(
                &palette,
                &format!("{path}?states={RETIRED_STATES}"),
            )?);
        }
        let answers: Vec<Value> = answers.into_iter().flatten().collect();
        let tags_unavailable = answers.is_empty();
        let layer = answers
            .iter()
            .map(|a| text(a, "/layer"))
            .find(|l| !l.is_empty())
            .unwrap_or_else(|| text(mine[0], "/spec/layer"));
        let tags = merged_tags(&answers);

        for e in mine {
            let version = text(e, "/spec/version");
            if versions.iter().any(|v: &PackBuild| v.version == version) {
                continue; // the paged listing repeats a version
            }
            let cloud_types: Vec<String> = e
                .pointer("/spec/cloudTypes")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(cloud) = &options.cloud
                && !supports(&cloud_types, cloud)
            {
                continue;
            }
            // a tag row names its version, and names the build it points at; match on either,
            // because a registry that answers with one and not the other still says enough
            let pack_uid = text(e, "/metadata/uid");
            let mut rows: Vec<(usize, String)> = tags
                .iter()
                .filter(|t| {
                    let v = text(t, "/version");
                    if v.is_empty() {
                        text(t, "/packUid") == pack_uid
                    } else {
                        v == version
                    }
                })
                .map(|t| {
                    let above = t
                        .pointer("/parentTags")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    (above, text(t, "/tag"))
                })
                .collect();
            // the exact tag first, then the floating tags above it. A tag row names every tag
            // above it, so the row with the most of them is the most exact. A registry that
            // names none leaves the tags of one version in name order.
            rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
            let for_version: Vec<String> = rows.into_iter().map(|(_, tag)| tag).collect();
            versions.push(PackBuild {
                version,
                pack_uid,
                digest: text(e, "/spec/digest"),
                kind: text(e, "/spec/type"),
                cloud_types,
                tags: for_version,
                latest: false,
                palette_latest: text(e, PALETTE_LATEST) == "true",
                state: crate::candidate::pack_state(e),
                listed: text(e, "/metadata/creationTimestamp"),
            });
        }
        versions.sort_by(|a, b| version::compare(&b.version, &a.version));
        if let Some(first) = versions.first_mut() {
            first.latest = true;
        }
        if options.latest {
            versions.retain(|v| v.latest || v.palette_latest);
        }
        if !versions.is_empty() {
            registries.push(PackRegistry {
                registry,
                registry_uid: uid,
                layer,
                versions,
                versions_offered,
                tags_unavailable,
                state,
            });
        }
    }
    // no registry has the pack at all: strata cannot answer. A cloud that keeps nothing is an
    // answer, and an empty one.
    if !had_the_pack {
        if options.registry.is_none() {
            return Err(Error::PackNameNotFound {
                name: name.to_string(),
            });
        }
        return Err(Error::PackNotFound {
            name: name.to_string(),
            version: String::new(),
            tag: None,
            registry: options.registry.clone().unwrap_or_default(),
        });
    }
    Ok(PackListing {
        schema: PACKS_SCHEMA.to_string(),
        tool: Tool::current(),
        generated_at: generated_at.to_string(),
        name: name.to_string(),
        cloud: options.cloud.clone(),
        registries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::MemoryApi;

    fn version(uid: &str, version: &str, clouds: &[&str]) -> Value {
        serde_json::json!({
            "metadata": {"uid": uid},
            "spec": {"name": "strata-live-dep", "version": version, "registryUid": "reg-own",
                     "layer": "addon", "type": "oci", "cloudTypes": clouds},
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
                serde_json::json!({"layer": "addon", "tags": []}),
            );
        registry_packs(
            "strata-live-dep",
            options,
            |_| Ok(Palette::new(api.clone())),
            "2026-10-05T00:00:00Z",
            &Progress::silent(),
        )
        .unwrap()
    }

    #[test]
    fn a_version_with_the_cloud_type_all_supports_each_cloud_and_is_the_latest_when_it_is_the_newest()
     {
        let listing = packs_for(
            "aws",
            vec![
                version("pk-1", "1.0.0", &["all"]),
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
        assert_eq!(listing.registries[0].versions[0].cloud_types, vec!["all"]);
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
    fn a_pre_release_is_the_newest_and_latest_keeps_the_release_that_palette_calls_the_latest() {
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
        assert_eq!(
            kept,
            vec![("1.30.0-rc.1", true, false), ("1.30.0", false, true)]
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
            crate::color::Paint::off(),
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
            lines[1].starts_with("  1.30.0-rc.1 ") && !lines[1].contains("Palette"),
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
            crate::color::Paint::off(),
            false,
        )
        .render_packs(&listing)
        .unwrap();
        assert!(!text.contains("Palette"), "{text}");
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
    /// Public Repo: `plain` with no query, and `retired` with the states query. An empty answer
    /// is a 404.
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
        assert_eq!(registry.state.as_deref(), Some("disabled"));
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
}
