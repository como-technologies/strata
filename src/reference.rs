//! What a user-supplied ref means.

use std::fmt;
use std::path::{Path, PathBuf};

/// The version that stands for the highest published version of a name: `<name>@latest`.
pub const LATEST: &str = "latest";

/// A way to name a profile version on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// A snapshot file written earlier by `snapshot`.
    Snapshot(PathBuf),
    /// `<name>@<version>`, resolved through the dashboard listing. The version [`LATEST`] picks
    /// the highest version of that name in the scope, by natural order.
    NameVersion {
        /// Profile name.
        name: String,
        /// Profile version string, or [`LATEST`].
        version: String,
    },
    /// A profile uid.
    Uid(String),
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
    /// `cluster:<uid>` or `cluster:<uid>/<profile>`: what a cluster runs. Without a profile,
    /// every profile applied to the cluster, in order; with one, only the profile of that name.
    Cluster {
        /// Cluster uid.
        uid: String,
        /// The applied profile to select, by name.
        profile: Option<String>,
    },
}

/// The prefix of a cluster ref.
pub const CLUSTER_PREFIX: &str = "cluster:";

/// The prefix of a pack ref.
pub const PACK_PREFIX: &str = "pack:";

impl Ref {
    /// A path to an existing file is a snapshot; `cluster:<uid>[/<profile>]` with a non-empty uid
    /// is a cluster; `name@version` with both parts non-empty is a name and version; anything else
    /// is a uid.
    pub fn parse(text: &str) -> Ref {
        Self::parse_with(text, |p| p.is_file())
    }

    /// [`Ref::parse`] with a caller-supplied "is this a file" check, for tests.
    pub fn parse_with(text: &str, is_file: impl Fn(&Path) -> bool) -> Ref {
        if is_file(Path::new(text)) {
            return Ref::Snapshot(PathBuf::from(text));
        }
        if let Some(rest) = text.strip_prefix(PACK_PREFIX)
            && let Some((left, version)) = rest.rsplit_once('@')
        {
            let (registry, name) = match left.split_once('/') {
                Some((registry, name)) => (Some(registry), name),
                None => (None, left),
            };
            if !name.is_empty() && !version.is_empty() {
                return Ref::Pack {
                    registry: registry.filter(|r| !r.is_empty()).map(str::to_string),
                    name: name.to_string(),
                    version: version.to_string(),
                };
            }
        }
        if let Some(rest) = text.strip_prefix(CLUSTER_PREFIX) {
            let (uid, profile) = match rest.split_once('/') {
                Some((uid, profile)) => (uid, Some(profile).filter(|p| !p.is_empty())),
                None => (rest, None),
            };
            if !uid.is_empty() {
                return Ref::Cluster {
                    uid: uid.to_string(),
                    profile: profile.map(str::to_string),
                };
            }
        }
        // A ref that is shaped like a path is a file, whether or not the file is there: the loader
        // then says which file is missing, instead of Palette answering 404 for a uid it never had.
        if looks_like_a_path(text) {
            return Ref::Snapshot(PathBuf::from(text));
        }
        if let Some((name, version)) = text.rsplit_once('@')
            && !name.is_empty()
            && !version.is_empty()
        {
            return Ref::NameVersion {
                name: name.to_string(),
                version: version.to_string(),
            };
        }
        Ref::Uid(text.to_string())
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ref::Snapshot(p) => write!(f, "{}", p.display()),
            Ref::NameVersion { name, version } => write!(f, "{name}@{version}"),
            Ref::Uid(u) => write!(f, "{u}"),
            Ref::Pack {
                registry,
                name,
                version,
            } => match registry {
                Some(r) => write!(f, "{PACK_PREFIX}{r}/{name}@{version}"),
                None => write!(f, "{PACK_PREFIX}{name}@{version}"),
            },
            Ref::Cluster { uid, profile: None } => write!(f, "{CLUSTER_PREFIX}{uid}"),
            Ref::Cluster {
                uid,
                profile: Some(p),
            } => write!(f, "{CLUSTER_PREFIX}{uid}/{p}"),
        }
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
mod tests {
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
                profile: Some("aws-stack".into())
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
    fn cluster_forms() {
        let no_files = |_: &Path| false;
        assert_eq!(
            Ref::parse_with("cluster:c1", no_files),
            Ref::Cluster {
                uid: "c1".into(),
                profile: None
            }
        );
        let selected = Ref::parse_with("cluster:c1/aws-stack", no_files);
        assert_eq!(
            selected,
            Ref::Cluster {
                uid: "c1".into(),
                profile: Some("aws-stack".into())
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
}
