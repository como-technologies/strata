//! Palette API access: the three GETs a profile snapshot needs, plus name@version resolution.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

pub const DEFAULT_API: &str = "https://api.spectrocloud.com";

/// The raw API responses a snapshot is built from. Saved as profile.json, manifests.json and
/// variables.json by --raw-dir and read back by --from-raw.
pub struct RawProfile {
    pub profile: Value,
    pub manifests: Value,
    pub variables: Value,
}

pub struct Client {
    base: String,
    key: String,
    project: Option<String>,
    agent: ureq::Agent,
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// KEY=VALUE lines from ~/.palette.env, with optional `export ` and quotes.
fn dotenv_lookup(key: &str) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let text = fs::read_to_string(Path::new(&home).join(".palette.env")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn setting(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| env_nonempty(k))
        .or_else(|| keys.iter().find_map(|k| dotenv_lookup(k)))
}

impl Client {
    pub fn from_env() -> Result<Self> {
        let key = setting(&["PALETTE_API_KEY", "SPECTROCLOUD_APIKEY"]).ok_or_else(|| {
            anyhow!("PALETTE_API_KEY is required to fetch from Palette (or SPECTROCLOUD_APIKEY, or a line in ~/.palette.env)")
        })?;
        let base = setting(&["PALETTE_API", "SPECTROCLOUD_HOST"])
            .unwrap_or_else(|| DEFAULT_API.to_string());
        let base = if base.starts_with("http://") || base.starts_with("https://") {
            base
        } else {
            format!("https://{base}")
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .into();
        Ok(Client {
            base: base.trim_end_matches('/').to_string(),
            key,
            project: setting(&["PALETTE_PROJECT_UID"]),
            agent,
        })
    }

    pub fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let mut req = self
            .agent
            .get(url.as_str())
            .header("ApiKey", self.key.as_str())
            .header("Accept", "application/json");
        if let Some(p) = &self.project {
            req = req.header("ProjectUid", p.as_str());
        }
        let mut resp = req.call().with_context(|| format!("GET {path}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(256 * 1024 * 1024)
            .read_to_string()
            .with_context(|| format!("GET {path}: reading the response body"))?;
        if !(200..300).contains(&status) {
            let snippet: String = body
                .chars()
                .take(300)
                .collect::<String>()
                .replace('\n', " ");
            bail!("GET {path} failed with HTTP {status}: {snippet}");
        }
        serde_json::from_str(&body).with_context(|| format!("GET {path}: response is not JSON"))
    }

    /// `<name>@<version>` is looked up in the dashboard listing; anything else is taken as a uid.
    pub fn resolve_uid(&self, reference: &str) -> Result<String> {
        match reference.rsplit_once('@') {
            Some((name, version)) if !name.is_empty() && !version.is_empty() => {
                let listing = self.get_json("/v1/dashboard/clusterprofiles/metadata")?;
                let items = listing
                    .get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                items
                    .iter()
                    .find(|it| {
                        it.pointer("/metadata/name").and_then(Value::as_str) == Some(name)
                            && it.pointer("/spec/version").and_then(Value::as_str) == Some(version)
                    })
                    .and_then(|it| it.pointer("/metadata/uid").and_then(Value::as_str))
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow!("no cluster profile named '{name}' with version '{version}' in this scope")
                    })
            }
            _ => Ok(reference.to_string()),
        }
    }

    pub fn fetch_raw(&self, uid: &str, save_dir: Option<&Path>, quiet: bool) -> Result<RawProfile> {
        if !quiet && std::env::var_os("STRATA_QUIET").is_none() {
            eprintln!("{}: fetching profile {uid}", crate::TOOL_NAME);
        }
        let profile = self.get_json(&format!("/v1/clusterprofiles/{uid}"))?;
        let manifests = self.get_json(&format!("/v1/clusterprofiles/{uid}/packs/manifests"))?;
        let variables = match self.get_json(&format!("/v1/clusterprofiles/{uid}/variables")) {
            Ok(v) => v,
            Err(e) => {
                if !quiet {
                    eprintln!(
                        "{}: note: variables endpoint unavailable for {uid} ({e:#}); treating as none",
                        crate::TOOL_NAME
                    );
                }
                serde_json::json!({ "variables": [] })
            }
        };
        let raw = RawProfile {
            profile,
            manifests,
            variables,
        };
        if let Some(dir) = save_dir {
            save_raw(&raw, dir)?;
        }
        Ok(raw)
    }
}

pub fn save_raw(raw: &RawProfile, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for (name, v) in [
        ("profile.json", &raw.profile),
        ("manifests.json", &raw.manifests),
        ("variables.json", &raw.variables),
    ] {
        fs::write(dir.join(name), serde_json::to_string_pretty(v)?)
            .with_context(|| format!("writing {}", dir.join(name).display()))?;
    }
    Ok(())
}

pub fn load_raw(dir: &Path) -> Result<RawProfile> {
    let read = |name: &str| -> Result<Value> {
        let p = dir.join(name);
        let text = fs::read_to_string(&p)
            .with_context(|| format!("missing or unreadable {}", p.display()))?;
        if text.trim().is_empty() {
            bail!("{} is empty", p.display());
        }
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", p.display()))
    };
    Ok(RawProfile {
        profile: read("profile.json")?,
        manifests: read("manifests.json")?,
        variables: read("variables.json")?,
    })
}
