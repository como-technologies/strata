//! The resolve of each layer: the pack that the registry gives, and the documents of a published
//! profile.

use serde_json::{Value, json};

use super::pack::find_version;
use super::presets::preset_options;
use super::rules::invalid_at;
use super::{
    CandidateLayer, LoadedCandidate, LoadedLayer, MINTED_LAYER, RegistryRef, ResolvedCandidate,
    gaps,
};
use crate::api::{Palette, PaletteApi, RawProfile};
use crate::closed_set::ClosedSet;
use crate::error::{Error, Result};
use crate::json::{arr, opt_str, opt_text, str_at, strings, text};
use crate::pack_type::PackType;
use crate::registry::{Registries, pack_type_of, registry_layer, retired_state};
use crate::snapshot::PresetOption;
use crate::term::progress::Progress;

/// The fields of one pack that a published profile gives in two documents: the pack reference
/// of the profile and the entry of the manifests document.
struct PackFields<'a> {
    layer: &'a str,
    kind: &'a str,
    tag: &'a str,
    version: &'a str,
    pack_uid: &'a str,
    registry_uid: &'a str,
    digest: &'a str,
    values: &'a str,
}

/// One layer as a published profile gives it, and what strata reads from the registry beside it.
struct ResolvedLayer {
    /// The pack reference of the profile.
    pack_ref: Value,
    /// The entry of the manifests document.
    entry: Value,
    /// The options of the pack template that the values select. Empty when no listing gives a
    /// template for the layer.
    preset_options: Vec<PresetOption>,
}

/// The pack reference and the manifests entry of the layer `l` with the fields `f`. The pack
/// reference has the digest only when it is not empty. The entry holds `manifests`.
fn pack_documents(l: &CandidateLayer, f: &PackFields, manifests: Vec<Value>) -> (Value, Value) {
    let mut pack_ref = json!({
        "name": l.name, "layer": f.layer, "type": f.kind, "tag": f.tag, "version": f.version,
        "packUid": f.pack_uid, "registryUid": f.registry_uid, "values": f.values,
        "manifests": [], "params": l.params, "presets": [],
    });
    if !f.digest.is_empty() {
        pack_ref["digest"] = json!(f.digest);
    }
    let entry = json!({
        "metadata": {"name": l.name, "uid": f.pack_uid},
        "spec": {
            "name": l.name, "layer": f.layer, "version": f.version, "digest": f.digest,
            "registryUid": f.registry_uid, "values": f.values, "manifests": manifests,
        },
        "status": {},
    });
    (pack_ref, entry)
}

/// The state of one resolve: the candidate, the client, and the registry listings, which strata
/// reads one time and uses again for each layer.
struct Resolver<'r, 'p, A: PaletteApi> {
    loaded: &'r LoadedCandidate,
    palette: &'p Palette<A>,
    progress: &'r Progress,
    /// The registry listings: the names, the provider types, and the sync of each registry.
    /// Strata reads each listing one time, and only when a layer needs it.
    registries: &'r mut Registries<'p, A>,
}

impl<A: PaletteApi> Resolver<'_, '_, A> {
    /// A `CandidateInvalid` error for the candidate file with `message`.
    fn invalid(&self, message: String) -> Error {
        invalid_at(&self.loaded.path)(message)
    }

    /// The uid of the registry that `registry_ref` names.
    fn registry_uid(&mut self, registry_ref: &RegistryRef) -> Result<String> {
        match registry_ref {
            RegistryRef::Uid(u) => Ok(u.clone()),
            RegistryRef::Name(n) => Ok(self.registries.by_ref(n)?.1),
        }
    }

    /// Resolve the layer `i` of the candidate.
    fn layer(&mut self, i: usize) -> Result<ResolvedLayer> {
        let l = &self.loaded.candidate.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        let manifests: Vec<Value> = loaded_layer
            .manifests
            .iter()
            .map(|(name, content)| json!({"name": name, "uid": "", "digest": "", "content": content}))
            .collect();
        let Some(registry_ref) = &loaded_layer.registry else {
            return Ok(manifest_layer(l, loaded_layer, manifests));
        };
        let registry = self.registry_uid(registry_ref)?;
        match find_version(self.palette, &l.name, &l.version, &registry)? {
            Some(summary) => self.registry_layer(i, registry_ref, &registry, &summary, manifests),
            None => self.unsynced_layer(i, registry_ref, &registry, manifests),
        }
    }

    /// Resolve the layer `i`, which the listing of the registry does not have. Palette lists no
    /// pack of a registry that it does not sync, and makes the pack at the create. A registry
    /// that syncs and lists no such version does not have it.
    fn unsynced_layer(
        &mut self,
        i: usize,
        registry_ref: &RegistryRef,
        registry: &str,
        manifests: Vec<Value>,
    ) -> Result<ResolvedLayer> {
        let l = &self.loaded.candidate.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        let Some(how) = self.registries.unsynced(registry)? else {
            return Err(Error::PackNotFound {
                name: l.name.clone(),
                version: l.version.clone(),
                tag: None,
                registry: registry_ref.text().to_string(),
                unsynced: Vec::new(),
            });
        };
        let from_unsynced = format!(
            "layer {} ({}) is from registry {}, which Palette does not sync",
            i + 1,
            l.name,
            registry_ref.text()
        );
        let not_synced = |message: String| self.invalid(format!("{from_unsynced}; {message}"));
        // the pack that Palette makes has the layer addon and the tag as its version, and a
        // profile takes both from it
        if let Some(layer) = l.layer.as_deref().filter(|&x| x != MINTED_LAYER.as_str()) {
            return Err(not_synced(format!(
                "Palette gives the pack that it makes the layer {MINTED_LAYER}, not \
                 {layer}, so remove layer or set it to {MINTED_LAYER}"
            )));
        }
        let tag = l.tag.as_deref().unwrap_or(&l.version);
        if tag != l.version {
            return Err(not_synced(format!(
                "Palette gives the pack that it makes the tag as its version, so set version \
                 to {tag}, or remove tag"
            )));
        }
        gaps::check_pack_tag(&from_unsynced, tag).map_err(|broken| broken.at(&self.loaded.path))?;
        self.progress.note(&format!(
            "{}@{}: registry {} does not sync, so Palette makes the pack when it creates the \
             profile; the record has no pack uid and no digest",
            l.name,
            l.version,
            registry_ref.text()
        ));
        let values = loaded_layer.values.clone().unwrap_or_default();
        let fields = PackFields {
            layer: MINTED_LAYER.as_str(),
            kind: how.kind.pack_type().as_str(),
            tag,
            version: tag,
            pack_uid: "",
            registry_uid: registry,
            digest: "",
            values: &values,
        };
        let (mut pack_ref, entry) = pack_documents(l, &fields, manifests);
        if let Some(t) = &how.provider {
            pack_ref["annotations"] = json!({ "ociRegistryProviderType": t });
        }
        // the pack that Palette makes has no cloud type, so each cloud takes it
        Ok(ResolvedLayer {
            pack_ref,
            entry,
            preset_options: Vec::new(),
        })
    }

    /// Resolve the layer `i` from `summary`, the entry of its version in the listing of the
    /// registry, and from the tags and the values that the registry gives for the pack.
    fn registry_layer(
        &mut self,
        i: usize,
        registry_ref: &RegistryRef,
        registry: &str,
        summary: &Value,
        manifests: Vec<Value>,
    ) -> Result<ResolvedLayer> {
        let c = &self.loaded.candidate;
        let l = &c.layers[i];
        let loaded_layer = &self.loaded.layers[i];
        // Palette serves no disabled or deleted pack: the registry answers 404 for it, and a
        // profile that pins one is refused. Say so before that read.
        if let Some(state) = retired_state(summary) {
            // a `pack:` ref comes here with the uid of the registry; the error gives its name
            let registry = match registry_ref {
                RegistryRef::Name(n) => n.clone(),
                RegistryRef::Uid(u) => self.registries.name_of(u)?.unwrap_or_else(|| u.clone()),
            };
            return Err(Error::PackUnavailable {
                name: l.name.clone(),
                version: l.version.clone(),
                registry,
                state: state.as_str().to_string(),
            });
        }
        // only the create checks that the pack supports the cloud of the profile
        let clouds = strings(summary, "/spec/cloudTypes");
        gaps::check_cloud(&c.cloud_type, i, l, &clouds)
            .map_err(|broken| broken.at(&self.loaded.path))?;
        let pack_uid = text(summary, "/metadata/uid");
        let entity = self.palette.pack_tags(&l.name, registry, None)?;
        let tags: Vec<String> = arr(&entity, "/tags")
            .iter()
            .filter(|t| str_at(t, "/packUid") == pack_uid)
            .filter_map(|t| opt_text(t, "/tag"))
            .collect();
        // Palette stores the tag a profile was authored with, which can be the version itself
        // rather than one of the registry's tag names (a Terraform-built profile pins `24.04`
        // where the registry says `LTS__24.x`). Accept both; anything else is a typo.
        let tag = match &l.tag {
            Some(t) if tags.contains(t) || *t == l.version => t.clone(),
            Some(t) => {
                return Err(Error::PackNotFound {
                    name: l.name.clone(),
                    version: l.version.clone(),
                    tag: Some(t.clone()),
                    registry: registry_ref.text().to_string(),
                    unsynced: Vec::new(),
                });
            }
            None => tags
                .iter()
                .find(|t| **t == l.version)
                .or_else(|| tags.first())
                .cloned()
                .unwrap_or_else(|| l.version.clone()),
        };
        // the default values of the version: the listing, and the tags route when the listing
        // gives none. The file keeps `values: ""` as empty values, so this rule does not read it
        let pack_values = arr(&entity, "/packValues")
            .iter()
            .find(|v| str_at(v, "/packUid") == pack_uid)
            .and_then(|v| opt_text(v, "/values"));
        let values = loaded_layer
            .values
            .clone()
            .or_else(|| opt_text(summary, "/spec/values"))
            .or(pack_values)
            .unwrap_or_default();
        // Palette gives the pack the layer of the registry and drops the layer of the request with
        // no error. So another layer in the file gives a root that the published version does not
        // have.
        let theirs = registry_layer(summary, std::slice::from_ref(&entity));
        if let (Some(given), Some(theirs)) = (l.layer.as_deref(), theirs.as_deref())
            && given != theirs
        {
            return Err(self.invalid(format!(
                "layer {} ({}) has the layer {given}, and its registry gives the layer {theirs}; \
                 Palette takes the layer from the registry, so remove layer or set it to {theirs}",
                i + 1,
                l.name
            )));
        }
        let layer = theirs.or_else(|| l.layer.clone()).unwrap_or_default();
        let kind = pack_type_of(summary);
        let digest = text(summary, "/spec/digest");
        let fields = PackFields {
            layer: &layer,
            kind: kind.as_str(),
            tag: &tag,
            version: &l.version,
            pack_uid: &pack_uid,
            registry_uid: registry,
            digest: &digest,
            values: &values,
        };
        let (mut pack_ref, entry) = pack_documents(l, &fields, manifests);
        // the provider type of an OCI registry, as a published profile carries it on the pack
        // reference
        if kind == PackType::Oci
            && let Some(t) = self.registries.oci_provider_type(registry)?
        {
            pack_ref["annotations"] = json!({ "ociRegistryProviderType": t });
        }
        Ok(ResolvedLayer {
            pack_ref,
            entry,
            preset_options: preset_options(summary, &values),
        })
    }
}

/// Resolve a manifest layer. No registry holds it, so it needs no request. Palette mints its
/// pack uid at the create, so the candidate has none, and it has no digest.
fn manifest_layer(
    l: &CandidateLayer,
    loaded_layer: &LoadedLayer,
    manifests: Vec<Value>,
) -> ResolvedLayer {
    let values = loaded_layer.values.clone().unwrap_or_default();
    let fields = PackFields {
        layer: l.layer.as_deref().unwrap_or(MINTED_LAYER.as_str()),
        kind: PackType::Manifest.as_str(),
        tag: l.tag.as_deref().unwrap_or(&l.version),
        version: &l.version,
        pack_uid: "",
        registry_uid: "",
        digest: "",
        values: &values,
    };
    let (pack_ref, entry) = pack_documents(l, &fields, manifests);
    ResolvedLayer {
        pack_ref,
        entry,
        preset_options: Vec::new(),
    }
}

/// Look each layer's pack up in its registry and build the three documents a published profile
/// with this content returns: `GET /v1/registries/metadata` for registry names,
/// `GET /v1/packs?filters=spec.name=<name>` for the pack uid, digest, layer, type, and default
/// values, and `GET /v1/packs/<name>/registries/<uid>` for the tags. A `manifest` layer needs
/// none of these. When the listing does not have the version, and the registry is one that
/// Palette does not sync, the pack is the one that Palette makes at the create: no pack uid, no
/// digest, the layer `addon`, and the tag as the version.
///
/// The documents hold only what Palette gives. What strata reads beside them, the options of the
/// pack template that the values of each pack select, is a field of the result.
///
/// # Errors
///
/// `CandidateInvalid` for a layer that breaks a rule of Palette at the create. `PackNotFound` and
/// `PackUnavailable` for a version that the registry does not give. Each error of a request.
pub fn resolve<A: PaletteApi>(
    loaded: &LoadedCandidate,
    palette: &Palette<A>,
    progress: &Progress,
) -> Result<ResolvedCandidate> {
    resolve_with(loaded, &mut Registries::new(palette), progress)
}

/// [`resolve`] with `registries`, the registry listings that the caller read already, so that
/// one command reads each listing one time.
pub(crate) fn resolve_with<A: PaletteApi>(
    loaded: &LoadedCandidate,
    registries: &mut Registries<'_, A>,
    progress: &Progress,
) -> Result<ResolvedCandidate> {
    let c = &loaded.candidate;
    progress.note(&format!(
        "resolving {}@{} against the registry",
        c.name, c.version
    ));
    let mut resolver = Resolver {
        loaded,
        palette: registries.palette(),
        progress,
        registries,
    };
    let mut refs = Vec::with_capacity(c.layers.len());
    let mut entries = Vec::with_capacity(c.layers.len());
    let mut preset_options = Vec::with_capacity(c.layers.len());
    for i in 0..c.layers.len().min(loaded.layers.len()) {
        let resolved = resolver.layer(i)?;
        refs.push(resolved.pack_ref);
        entries.push(resolved.entry);
        preset_options.push(resolved.preset_options);
    }
    // each pack reference has the layer that Palette gives the pack, so check the layers again
    let known: Vec<Option<&str>> = refs.iter().map(|r| opt_str(r, "/layer")).collect();
    gaps::check_layers(c, &known).map_err(|broken| broken.at(&loaded.path))?;
    let raw = RawProfile {
        profile: json!({
            "metadata": {"name": c.name, "uid": ""},
            "spec": {
                "version": c.version,
                "published": {"cloudType": c.cloud_type, "type": c.kind, "packs": refs},
            },
            "status": {},
        }),
        manifests: json!({"metadata": {"name": c.name, "uid": ""}, "spec": {"packs": entries}}),
        variables: json!({ "variables": c.variables }),
        variables_unavailable: false,
        fetched_at: None,
        project_uid: None,
        manifest_entities: None,
        resolved_values: None,
    };
    Ok(ResolvedCandidate {
        raw,
        preset_options,
    })
}
