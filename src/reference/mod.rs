//! What a user-supplied ref means.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub(crate) mod picked;
pub mod version;

/// The version that stands for the highest published version of a name: `<name>@latest`.
pub const LATEST: &str = "latest";

/// A way to name a profile version on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// A snapshot file written earlier by `snapshot`.
    Snapshot(PathBuf),
    /// `<name>@<version>`, resolved through the dashboard listing. The version [`LATEST`] picks
    /// the highest version of that name in the scope that is not a pre-release, by
    /// [`version::compare`].
    NameVersion {
        /// Profile name.
        name: String,
        /// Profile version string, or [`LATEST`].
        version: String,
    },
    /// A profile uid.
    Uid(String),
    /// `<name>@<version>@draft` or `<uid>@draft`: the draft template of a profile version, which
    /// the next publish makes the published template.
    Draft {
        /// Profile uid, or its name when `version` is set.
        id: String,
        /// The version of the name, or [`LATEST`].
        version: Option<String>,
    },
    /// `pack:<name>@<version>` or `pack:<registry>/<name>@<version>`: one pack version in a
    /// registry, with the values the registry ships. The version [`LATEST`] picks the highest
    /// version of that pack in the registry.
    Pack {
        /// The registry, by name or uid. Without one, strata uses the registry that has the pack
        /// version, and says so when several do.
        registry: Option<String>,
        /// Pack name.
        name: String,
        /// Pack version, or [`LATEST`].
        version: String,
    },
    /// `cluster:<uid>` or `cluster:<uid>/<profile>`, either with `@pending`: what a cluster runs.
    /// Without a profile, every profile applied to the cluster, in canonical order: the cluster or
    /// infra profile first, then the others by name. With one, only the profile of that name.
    Cluster {
        /// Cluster uid.
        uid: String,
        /// The applied profile to select, by name.
        profile: Option<String>,
        /// `@pending`: the profiles that the cluster would run after it accepts the update that
        /// waits, and not the copies that it runs now.
        pending: bool,
    },
    /// `edgehost:<uid>` or `edgehost:<uid>/<profile>`: the profiles bound to an edge host, as for
    /// a cluster.
    EdgeHost {
        /// Edge host uid, or its name.
        uid: String,
        /// The bound profile to select, by name.
        profile: Option<String>,
    },
    /// `appprofile:<uid>` or `appprofile:<name>@<version>`: an app profile, whose tiers are its
    /// packs. The version [`LATEST`] picks the highest version of that name that is not a
    /// pre-release.
    AppProfile {
        /// App profile uid, or its name when `version` is set.
        id: String,
        /// The version of the name.
        version: Option<String>,
    },
    /// `tag:<key>=<value>`, several joined with `+`, or `filter:<name>`: the clusters whose labels
    /// meet the query, each in its own project. `snapshot --dir` records each one.
    ClusterSet {
        /// Each label that must be there: `env=prod`, or `env`.
        tags: Vec<String>,
        /// A saved tag filter, by name or uid.
        filter: Option<String>,
    },
    /// `clustergroup:<uid>`: the profiles that a cluster group pins for its virtual clusters.
    ClusterGroup {
        /// Cluster group uid, or its name.
        uid: String,
    },
}

/// The suffix of a cluster ref that names the update that waits for the cluster.
pub const PENDING_SUFFIX: &str = "@pending";

/// The suffix of a profile ref that names the draft template of the version.
pub const DRAFT_SUFFIX: &str = "@draft";

/// The prefix of a cluster ref.
pub const CLUSTER_PREFIX: &str = "cluster:";

/// The prefix of an edge host ref.
pub const EDGE_HOST_PREFIX: &str = "edgehost:";

/// The prefix of a ref of the clusters with some labels.
pub const TAG_PREFIX: &str = "tag:";

/// The prefix of a ref of the clusters that a saved tag filter selects.
pub const FILTER_PREFIX: &str = "filter:";

/// The prefix of an app profile ref.
pub const APP_PROFILE_PREFIX: &str = "appprofile:";

/// The prefix of a cluster group ref.
pub const CLUSTER_GROUP_PREFIX: &str = "clustergroup:";

/// The prefix of a pack ref.
pub const PACK_PREFIX: &str = "pack:";

impl Ref {
    /// A path to an existing file is a snapshot; `pack:[<registry>/]<name>@<version>` is a pack
    /// version; `cluster:<uid>[/<profile>][@pending]` with a non-empty uid is a cluster;
    /// `edgehost:<uid>[/<profile>]` is an edge host; `clustergroup:<uid>` is a cluster group;
    /// `appprofile:<uid>` or `appprofile:<name>@<version>` is an app profile; `tag:<key>=<value>`
    /// and `filter:<name>` are a set of clusters; `name@version@draft` and `uid@draft` are the
    /// draft of a version; `name@version` with both parts non-empty is a name and version;
    /// anything else is a uid.
    pub fn parse(text: &str) -> Ref {
        Self::parse_with(text, |p| p.is_file())
    }

    /// [`Ref::parse`] with a caller-supplied "is this a file" check, for tests.
    pub fn parse_with(text: &str, is_file: impl Fn(&Path) -> bool) -> Ref {
        if is_file(Path::new(text)) {
            return Ref::Snapshot(PathBuf::from(text));
        }
        // a prefix with nothing valid after it is no ref of its kind, so the text goes to the
        // next rule
        let prefixed = pack_ref(text)
            .or_else(|| cluster_ref(text))
            .or_else(|| edge_host_ref(text))
            .or_else(|| tag_ref(text))
            .or_else(|| filter_ref(text))
            .or_else(|| app_profile_ref(text))
            .or_else(|| cluster_group_ref(text));
        if let Some(parsed) = prefixed {
            return parsed;
        }
        // A ref that is shaped like a path is a file, whether or not the file is there: the loader
        // then says which file is missing, instead of Palette answering 404 for a uid it never had.
        if looks_like_a_path(text) {
            return Ref::Snapshot(PathBuf::from(text));
        }
        if let Some(draft) = draft_ref(text) {
            return draft;
        }
        if let Some((name, version)) = name_at_version(text) {
            return Ref::NameVersion {
                name: name.to_string(),
                version: version.to_string(),
            };
        }
        Ref::Uid(text.to_string())
    }

    /// True when the ref names a profile version: `<name>@<version>`, a uid, or either with
    /// `@draft`. A record file can hold one too, and the record says.
    pub fn is_profile_version(&self) -> bool {
        matches!(
            self,
            Ref::NameVersion { .. } | Ref::Uid(_) | Ref::Draft { .. }
        )
    }
}

/// `<uid>/<profile>` or `<uid>`: the uid, and the profile when the text names one after the
/// `/`. An empty profile is none.
fn uid_and_profile(text: &str) -> (&str, Option<&str>) {
    match text.split_once('/') {
        Some((uid, profile)) => (uid, Some(profile).filter(|p| !p.is_empty())),
        None => (text, None),
    }
}

/// The pack ref `pack:[<registry>/]<name>@<version>`, when `text` is one.
fn pack_ref(text: &str) -> Option<Ref> {
    let (left, version) = name_at_version(text.strip_prefix(PACK_PREFIX)?)?;
    let (registry, name) = match left.split_once('/') {
        Some((registry, name)) => (Some(registry), name),
        None => (None, left),
    };
    let registry = registry.filter(|r| !r.is_empty());
    // A pack ref with no registry prints with no `/` before the version. So a name with a `/` and
    // no registry is no pack ref: its text parses as another ref.
    (!name.is_empty() && (registry.is_some() || !name.contains('/'))).then(|| Ref::Pack {
        registry: registry.map(str::to_string),
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// The cluster ref `cluster:<uid>[/<profile>][@pending]`, when `text` is one.
///
/// A `/` with no profile after it is nothing, also after `@pending`. So `cluster:<uid>@pending/`
/// names what `cluster:<uid>@pending` names. Without this rule, the text names the uid
/// `<uid>@pending`, and that ref prints as the text of another ref.
fn cluster_ref(text: &str) -> Option<Ref> {
    let rest = text.strip_prefix(CLUSTER_PREFIX)?;
    let rest = rest
        .strip_suffix('/')
        .filter(|r| r.ends_with(PENDING_SUFFIX))
        .unwrap_or(rest);
    let (rest, pending) = match rest.strip_suffix(PENDING_SUFFIX) {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let (uid, profile) = uid_and_profile(rest);
    (!uid.is_empty()).then(|| Ref::Cluster {
        uid: uid.to_string(),
        profile: profile.map(str::to_string),
        pending,
    })
}

/// The edge host ref `edgehost:<uid>[/<profile>]`, when `text` is one.
fn edge_host_ref(text: &str) -> Option<Ref> {
    let (uid, profile) = uid_and_profile(text.strip_prefix(EDGE_HOST_PREFIX)?);
    (!uid.is_empty()).then(|| Ref::EdgeHost {
        uid: uid.to_string(),
        profile: profile.map(str::to_string),
    })
}

/// The ref of the clusters with some labels, `tag:<key>=<value>`, when `text` is one. A text with
/// no label, such as `tag:+`, is none: that ref prints as `tag:`, which is no ref of this kind.
fn tag_ref(text: &str) -> Option<Ref> {
    let tags: Vec<String> = text
        .strip_prefix(TAG_PREFIX)?
        .split('+')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    (!tags.is_empty()).then_some(Ref::ClusterSet { tags, filter: None })
}

/// The ref of the clusters that a saved tag filter selects, `filter:<name>`, when `text` is one.
fn filter_ref(text: &str) -> Option<Ref> {
    let name = text.strip_prefix(FILTER_PREFIX).filter(|n| !n.is_empty())?;
    Some(Ref::ClusterSet {
        tags: Vec::new(),
        filter: Some(name.to_string()),
    })
}

/// The app profile ref `appprofile:<uid>` or `appprofile:<name>@<version>`, when `text` is one.
fn app_profile_ref(text: &str) -> Option<Ref> {
    let rest = text
        .strip_prefix(APP_PROFILE_PREFIX)
        .filter(|r| !r.is_empty() && !r.contains('/'))?;
    Some(match name_at_version(rest) {
        Some((name, version)) => Ref::AppProfile {
            id: name.to_string(),
            version: Some(version.to_string()),
        },
        None => Ref::AppProfile {
            id: rest.to_string(),
            version: None,
        },
    })
}

/// The cluster group ref `clustergroup:<uid>`, when `text` is one.
fn cluster_group_ref(text: &str) -> Option<Ref> {
    let uid = text
        .strip_prefix(CLUSTER_GROUP_PREFIX)
        .filter(|u| !u.is_empty() && !u.contains('/'))?;
    Some(Ref::ClusterGroup {
        uid: uid.to_string(),
    })
}

/// The draft ref `<name>@<version>@draft` or `<uid>@draft`, when `text` is one.
fn draft_ref(text: &str) -> Option<Ref> {
    let rest = text.strip_suffix(DRAFT_SUFFIX).filter(|r| !r.is_empty())?;
    Some(match name_at_version(rest) {
        Some((name, version)) => Ref::Draft {
            id: name.to_string(),
            version: Some(version.to_string()),
        },
        None => Ref::Draft {
            id: rest.to_string(),
            version: None,
        },
    })
}

/// A profile version as a ref writes it: `<name>@<version>`.
pub(crate) fn name_version(name: &str, version: &str) -> String {
    format!("{name}@{version}")
}

/// `<name>@<version>`, split at the last `@`, when both parts are there.
fn name_at_version(text: &str) -> Option<(&str, &str)> {
    text.rsplit_once('@')
        .filter(|(name, version)| !name.is_empty() && !version.is_empty())
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ref::Snapshot(p) => write!(f, "{}", p.display()),
            Ref::NameVersion { name, version } => f.write_str(&name_version(name, version)),
            Ref::Uid(u) => write!(f, "{u}"),
            Ref::Draft { id, version } => match version {
                Some(v) => write!(f, "{id}@{v}{DRAFT_SUFFIX}"),
                None => write!(f, "{id}{DRAFT_SUFFIX}"),
            },
            Ref::Pack {
                registry,
                name,
                version,
            } => match registry {
                Some(r) => write!(f, "{PACK_PREFIX}{r}/{name}@{version}"),
                None => write!(f, "{PACK_PREFIX}{name}@{version}"),
            },
            Ref::Cluster {
                uid,
                profile,
                pending,
            } => {
                write!(f, "{CLUSTER_PREFIX}{uid}")?;
                if let Some(p) = profile {
                    write!(f, "/{p}")?;
                }
                if *pending {
                    write!(f, "{PENDING_SUFFIX}")?;
                }
                Ok(())
            }
            Ref::ClusterGroup { uid } => write!(f, "{CLUSTER_GROUP_PREFIX}{uid}"),
            Ref::ClusterSet { tags, filter } => match filter {
                Some(name) => write!(f, "{FILTER_PREFIX}{name}"),
                None => write!(f, "{TAG_PREFIX}{}", tags.join("+")),
            },
            Ref::AppProfile { id, version } => match version {
                Some(v) => write!(f, "{APP_PROFILE_PREFIX}{id}@{v}"),
                None => write!(f, "{APP_PROFILE_PREFIX}{id}"),
            },
            Ref::EdgeHost { uid, profile } => match profile {
                Some(p) => write!(f, "{EDGE_HOST_PREFIX}{uid}/{p}"),
                None => write!(f, "{EDGE_HOST_PREFIX}{uid}"),
            },
        }
    }
}

/// The one object that a name selects, from `hits`, the objects with the name.
///
/// `uid_of` gives the uid of an object. The hits are sorted by uid, so that a message reads the
/// same on each run: Palette lists its objects in the order it likes. One hit is the answer. None
/// is `not_found()`. Two or more are `ambiguous(uids)`, with the sorted uids.
pub(crate) fn one_named<T>(
    mut hits: Vec<T>,
    uid_of: impl Fn(&T) -> &str,
    not_found: impl FnOnce() -> Error,
    ambiguous: impl FnOnce(Vec<String>) -> Error,
) -> Result<T> {
    hits.sort_by(|a, b| uid_of(a).cmp(uid_of(b)));
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(not_found()),
        _ => Err(ambiguous(
            hits.iter().map(|h| uid_of(h).to_string()).collect(),
        )),
    }
}

/// True when a ref reads as a file path: a separator, a file extension this tool writes, or a
/// leading `.`, `..` or `~`. A uid, a name, and a name@version hold none of these.
fn looks_like_a_path(text: &str) -> bool {
    text.contains('/')
        || text.contains('\\')
        || text.starts_with('.')
        || text.starts_with('~')
        || [".json", ".yaml", ".yml"]
            .iter()
            .any(|end| text.ends_with(end))
}

#[cfg(test)]
mod tests;
