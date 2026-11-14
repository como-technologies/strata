//! The mask of the console address of a pack of virtual machines.
//!
//! The console of such a pack forwards to an address that one value of the pack holds. The address
//! names the console of one cluster. Its schema format is not `password`, so Palette serves it in
//! clear. Strata writes `********` over it in each record and in each saved answer. A question to
//! Palette sends the address of the cluster in place of the mask: see [`console_values`].

use std::borrow::Cow;

use serde_json::Value;
use serde_yaml::Value as Yaml;

use super::{MASKED, field_text, mask_values, put_values};
use crate::api::raw::RawProfile;
use crate::closed_set::impl_texts;
use crate::error::Result;
use crate::json::{arr, str_at};
use crate::snapshot::Template;

/// The field under `charts.<pack>` of the values that holds the address that the console of a
/// pack of virtual machines forwards to. Palette reads it, and writes no mask over it.
const ADDRESS_FIELD: &str = "appConfig.clusterInfo.consoleBaseAddress";

/// A pack of virtual machines whose console Palette serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsolePack {
    /// The pack of the orchestrator of virtual machines.
    Orchestrator,
    /// The older name of [`ConsolePack::Orchestrator`].
    Dashboard,
    /// The pack that moves virtual machines into Palette.
    Migration,
}

// the names of the packs, as Palette finds them
impl_texts!(ConsolePack {
    Orchestrator => "virtual-machine-orchestrator",
    Dashboard => "spectro-vm-dashboard",
    Migration => "vm-migration-assistant",
});

impl ConsolePack {
    /// The field of the values that holds the console address, as a path of keys with dots:
    /// `charts.<pack>.appConfig.clusterInfo.consoleBaseAddress`.
    fn address_field(self) -> String {
        format!("charts.{}.{ADDRESS_FIELD}", self.as_str())
    }
}

/// Where a pack of a document of Palette holds its name and its values.
#[derive(Debug, Clone, Copy)]
enum PackForm {
    /// A pack of a template, such as `spec.published.packs[]` of a profile: `name` and `values`.
    Template,
    /// An entry of the answer of `packs/manifests`: `metadata.name` or `spec.name`, and
    /// `spec.values`.
    Manifests,
}

impl PackForm {
    /// The pack of virtual machines that `pk` is, or `None` for another pack.
    fn console_pack(self, pk: &Value) -> Option<ConsolePack> {
        match self {
            PackForm::Template => ConsolePack::from_text(str_at(pk, "/name")),
            PackForm::Manifests => [str_at(pk, "/metadata/name"), str_at(pk, "/spec/name")]
                .into_iter()
                .find_map(ConsolePack::from_text),
        }
    }

    /// The pointer of the values of a pack in this form.
    fn values_at(self) -> &'static str {
        match self {
            PackForm::Template => "/values",
            PackForm::Manifests => "/spec/values",
        }
    }
}

/// Write `********` over the console address of each pack of virtual machines in `packs`, a list
/// of packs in the form `form`. The rules of [`mask_values`] apply: an empty value and a value that
/// holds a variable stay as they are.
fn mask_console_packs(packs: Option<&mut Value>, form: PackForm) -> Result<()> {
    let packs = packs.and_then(Value::as_array_mut);
    for pk in packs.into_iter().flatten() {
        let Some(pack) = form.console_pack(pk) else {
            continue;
        };
        if let Some(v) = pk.pointer_mut(form.values_at())
            && let Some(text) = v.as_str()
        {
            *v = Value::String(mask_values(text, &[pack.address_field()])?);
        }
    }
    Ok(())
}

/// `raw` with `********` over the console address of each pack of virtual machines: in each
/// template of the profile, and in the answer of `packs/manifests`. A profile with no such pack
/// gives `raw` as it is.
///
/// Each record of a profile, of a draft, of a cluster, and of a candidate comes from such
/// documents, so it holds the mask. So does each saved directory of a profile.
pub(crate) fn mask_console_profile(raw: &RawProfile) -> Result<Cow<'_, RawProfile>> {
    let templates = [Template::Published, Template::Draft].map(|t| format!("/spec/{t}/packs"));
    let manifests = "/spec/packs";
    let holds = templates
        .iter()
        .flat_map(|t| arr(&raw.profile, t))
        .any(|pk| PackForm::Template.console_pack(pk).is_some())
        || arr(&raw.manifests, manifests)
            .iter()
            .any(|pk| PackForm::Manifests.console_pack(pk).is_some());
    if !holds {
        return Ok(Cow::Borrowed(raw));
    }
    let mut out = raw.clone();
    for t in &templates {
        mask_console_packs(out.profile.pointer_mut(t), PackForm::Template)?;
    }
    mask_console_packs(out.manifests.pointer_mut(manifests), PackForm::Manifests)?;
    Ok(Cow::Owned(out))
}

/// Write `********` over the console address of each pack of virtual machines in the documents of
/// a cluster, as a saved directory keeps them. `cluster` is the answer of
/// `GET /v1/spectroclusters/{uid}`, and `manifests` the answer of its `profiles/packs/manifests`.
pub(crate) fn mask_console_cluster(cluster: &mut Value, manifests: &mut Value) -> Result<()> {
    let templates = cluster
        .pointer_mut("/spec/clusterProfileTemplates")
        .and_then(Value::as_array_mut);
    for template in templates.into_iter().flatten() {
        mask_console_packs(template.get_mut("packs"), PackForm::Template)?;
    }
    let profiles = manifests
        .pointer_mut("/profiles")
        .and_then(Value::as_array_mut);
    for profile in profiles.into_iter().flatten() {
        mask_console_packs(profile.pointer_mut("/spec/packs"), PackForm::Manifests)?;
    }
    Ok(())
}

/// The values that a question to Palette sends for the pack `name` of a record, in place of its
/// `values`. `None` when the values go as they are.
///
/// At `validate/repave`, Palette saves the address that the console forwards to from the values
/// of the question. It saves the mask of strata as an address too. So a question sends no such
/// mask. `held` is the values of the pack as the cluster runs it, when it runs the pack.
///
/// - When `held` with the mask is `values`, the question sends `held`.
/// - Else the question sends `values` with the address of `held` in place of the mask. A cluster
///   that does not run the pack gives an empty address. Palette then makes the address as for a
///   pack with no address.
pub(crate) fn console_values(
    name: &str,
    values: &str,
    held: Option<&str>,
) -> Result<Option<String>> {
    let Some(pack) = ConsolePack::from_text(name) else {
        return Ok(None);
    };
    let field = [pack.address_field()];
    if field_text(values, &field[0]).as_deref() != Some(MASKED) {
        return Ok(None);
    }
    if let Some(held) = held
        && mask_values(held, &field)? == values
    {
        return Ok(Some(held.to_string()));
    }
    let address = held
        .and_then(|h| field_text(h, &field[0]))
        .unwrap_or_default();
    put_values(
        values,
        &field,
        &address,
        |v: &Yaml| v.as_str() == Some(MASKED),
        "the values of a question to Palette",
    )
    .map(Some)
}
