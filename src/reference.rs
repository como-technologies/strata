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
        if let Some((name, version)) = text.rsplit_once('@') {
            if !name.is_empty() && !version.is_empty() {
                return Ref::NameVersion {
                    name: name.to_string(),
                    version: version.to_string(),
                };
            }
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
            Ref::Cluster { uid, profile: None } => write!(f, "{CLUSTER_PREFIX}{uid}"),
            Ref::Cluster {
                uid,
                profile: Some(p),
            } => write!(f, "{CLUSTER_PREFIX}{uid}/{p}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forms() {
        let no_files = |_: &Path| false;
        assert_eq!(
            Ref::parse_with("64f0a1", no_files),
            Ref::Uid("64f0a1".into())
        );
        assert_eq!(
            Ref::parse_with("aws-pxk@1.2.0", no_files),
            Ref::NameVersion {
                name: "aws-pxk".into(),
                version: "1.2.0".into()
            }
        );
        assert_eq!(
            Ref::parse_with("@1.2.0", no_files),
            Ref::Uid("@1.2.0".into())
        );
        assert_eq!(
            Ref::parse_with("aws-pxk@", no_files),
            Ref::Uid("aws-pxk@".into())
        );
        assert_eq!(
            Ref::parse_with("rel.json", |_| true),
            Ref::Snapshot("rel.json".into())
        );
        assert_eq!(Ref::parse("aws-pxk@1.2.0").to_string(), "aws-pxk@1.2.0");
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
        let selected = Ref::parse_with("cluster:c1/aws-pxk", no_files);
        assert_eq!(
            selected,
            Ref::Cluster {
                uid: "c1".into(),
                profile: Some("aws-pxk".into())
            }
        );
        assert_eq!(selected.to_string(), "cluster:c1/aws-pxk");
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
