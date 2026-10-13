//! What a user-supplied ref means.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub mod version;

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
    /// packs. The version [`LATEST`] picks the highest version of that name.
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
        if let Some(rest) = text.strip_prefix(PACK_PREFIX)
            && let Some((left, version)) = name_at_version(rest)
        {
            let (registry, name) = match left.split_once('/') {
                Some((registry, name)) => (Some(registry), name),
                None => (None, left),
            };
            if !name.is_empty() {
                return Ref::Pack {
                    registry: registry.filter(|r| !r.is_empty()).map(str::to_string),
                    name: name.to_string(),
                    version: version.to_string(),
                };
            }
        }
        if let Some(rest) = text.strip_prefix(CLUSTER_PREFIX) {
            let (rest, pending) = match rest.strip_suffix(PENDING_SUFFIX) {
                Some(r) => (r, true),
                None => (rest, false),
            };
            let (uid, profile) = uid_and_profile(rest);
            if !uid.is_empty() {
                return Ref::Cluster {
                    uid: uid.to_string(),
                    profile: profile.map(str::to_string),
                    pending,
                };
            }
        }
        if let Some(rest) = text.strip_prefix(EDGE_HOST_PREFIX) {
            let (uid, profile) = uid_and_profile(rest);
            if !uid.is_empty() {
                return Ref::EdgeHost {
                    uid: uid.to_string(),
                    profile: profile.map(str::to_string),
                };
            }
        }
        if let Some(rest) = text.strip_prefix(TAG_PREFIX)
            && !rest.is_empty()
        {
            return Ref::ClusterSet {
                tags: rest
                    .split('+')
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
                    .collect(),
                filter: None,
            };
        }
        if let Some(name) = text.strip_prefix(FILTER_PREFIX)
            && !name.is_empty()
        {
            return Ref::ClusterSet {
                tags: Vec::new(),
                filter: Some(name.to_string()),
            };
        }
        if let Some(rest) = text.strip_prefix(APP_PROFILE_PREFIX)
            && !rest.is_empty()
            && !rest.contains('/')
        {
            return match name_at_version(rest) {
                Some((name, version)) => Ref::AppProfile {
                    id: name.to_string(),
                    version: Some(version.to_string()),
                },
                None => Ref::AppProfile {
                    id: rest.to_string(),
                    version: None,
                },
            };
        }
        if let Some(uid) = text.strip_prefix(CLUSTER_GROUP_PREFIX)
            && !uid.is_empty()
            && !uid.contains('/')
        {
            return Ref::ClusterGroup {
                uid: uid.to_string(),
            };
        }
        // A ref that is shaped like a path is a file, whether or not the file is there: the loader
        // then says which file is missing, instead of Palette answering 404 for a uid it never had.
        if looks_like_a_path(text) {
            return Ref::Snapshot(PathBuf::from(text));
        }
        if let Some(rest) = text.strip_suffix(DRAFT_SUFFIX)
            && !rest.is_empty()
        {
            return match name_at_version(rest) {
                Some((name, version)) => Ref::Draft {
                    id: name.to_string(),
                    version: Some(version.to_string()),
                },
                None => Ref::Draft {
                    id: rest.to_string(),
                    version: None,
                },
            };
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

/// `<name>@<version>`, split at the last `@`, when both parts are there.
fn name_at_version(text: &str) -> Option<(&str, &str)> {
    text.rsplit_once('@')
        .filter(|(name, version)| !name.is_empty() && !version.is_empty())
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ref::Snapshot(p) => write!(f, "{}", p.display()),
            Ref::NameVersion { name, version } => write!(f, "{name}@{version}"),
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
                profile: Some("aws-stack".into()),
                pending: false,
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
    fn a_profile_ref_with_draft_reads_the_draft_of_the_version() {
        let no_files = |_: &Path| false;
        let by_name = Ref::parse_with("aws-stack@1.2.0@draft", no_files);
        assert_eq!(
            by_name,
            Ref::Draft {
                id: "aws-stack".into(),
                version: Some("1.2.0".into()),
            }
        );
        assert_eq!(by_name.to_string(), "aws-stack@1.2.0@draft");
        let latest = Ref::parse_with("aws-stack@latest@draft", no_files);
        assert_eq!(
            latest,
            Ref::Draft {
                id: "aws-stack".into(),
                version: Some(LATEST.into()),
            }
        );
        let by_uid = Ref::parse_with("64f0a1@draft", no_files);
        assert_eq!(
            by_uid,
            Ref::Draft {
                id: "64f0a1".into(),
                version: None,
            }
        );
        assert_eq!(by_uid.to_string(), "64f0a1@draft");
        assert_eq!(
            Ref::parse_with("@draft", no_files),
            Ref::Uid("@draft".into()),
            "no profile, no draft"
        );
        assert_eq!(
            Ref::parse_with("releases/dev@draft", no_files),
            Ref::Snapshot("releases/dev@draft".into()),
            "a path stays a file"
        );
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
                profile: None,
                pending: false,
            }
        );
        let selected = Ref::parse_with("cluster:c1/aws-stack", no_files);
        assert_eq!(
            selected,
            Ref::Cluster {
                uid: "c1".into(),
                profile: Some("aws-stack".into()),
                pending: false,
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

    #[test]
    fn a_cluster_ref_with_pending_reads_the_update_that_waits() {
        let no_files = |_: &Path| false;
        let all = Ref::parse_with("cluster:c1@pending", no_files);
        assert_eq!(
            all,
            Ref::Cluster {
                uid: "c1".into(),
                profile: None,
                pending: true,
            }
        );
        assert_eq!(all.to_string(), "cluster:c1@pending");
        let one = Ref::parse_with("cluster:c1/aws-stack@pending", no_files);
        assert_eq!(
            one,
            Ref::Cluster {
                uid: "c1".into(),
                profile: Some("aws-stack".into()),
                pending: true,
            }
        );
        assert_eq!(one.to_string(), "cluster:c1/aws-stack@pending");
    }

    #[test]
    fn edge_host_forms() {
        let no_files = |_: &Path| false;
        let all = Ref::parse_with("edgehost:e1", no_files);
        assert_eq!(
            all,
            Ref::EdgeHost {
                uid: "e1".into(),
                profile: None,
            }
        );
        assert_eq!(all.to_string(), "edgehost:e1");
        let one = Ref::parse_with("edgehost:e1/edge-stack", no_files);
        assert_eq!(
            one,
            Ref::EdgeHost {
                uid: "e1".into(),
                profile: Some("edge-stack".into()),
            }
        );
        assert_eq!(one.to_string(), "edgehost:e1/edge-stack");
        assert_eq!(
            Ref::parse_with("edgehost:", no_files),
            Ref::Uid("edgehost:".into()),
            "no uid, no edge host"
        );
    }

    #[test]
    fn app_profile_forms() {
        let no_files = |_: &Path| false;
        let by_uid = Ref::parse_with("appprofile:a1", no_files);
        assert_eq!(
            by_uid,
            Ref::AppProfile {
                id: "a1".into(),
                version: None,
            }
        );
        assert_eq!(by_uid.to_string(), "appprofile:a1");
        let by_name = Ref::parse_with("appprofile:shop@1.0.0", no_files);
        assert_eq!(
            by_name,
            Ref::AppProfile {
                id: "shop".into(),
                version: Some("1.0.0".into()),
            }
        );
        assert_eq!(by_name.to_string(), "appprofile:shop@1.0.0");
        assert_eq!(
            Ref::parse_with("appprofile:", no_files),
            Ref::Uid("appprofile:".into()),
            "no uid, no app profile"
        );
    }

    #[test]
    fn cluster_set_forms() {
        let no_files = |_: &Path| false;
        let tags = Ref::parse_with("tag:env=prod+team=search", no_files);
        assert_eq!(
            tags,
            Ref::ClusterSet {
                tags: vec!["env=prod".into(), "team=search".into()],
                filter: None,
            }
        );
        assert_eq!(tags.to_string(), "tag:env=prod+team=search");
        let filter = Ref::parse_with("filter:prod", no_files);
        assert_eq!(
            filter,
            Ref::ClusterSet {
                tags: vec![],
                filter: Some("prod".into()),
            }
        );
        assert_eq!(filter.to_string(), "filter:prod");
        assert_eq!(
            Ref::parse_with("tag:", no_files),
            Ref::Uid("tag:".into()),
            "no label, no set"
        );
    }

    #[test]
    fn one_named_gives_the_one_hit_and_names_the_sorted_uids_of_several() {
        let not_found = || Error::Usage {
            message: "none".to_string(),
        };
        let ambiguous = |uids: Vec<String>| Error::Usage {
            message: uids.join(","),
        };
        let one = one_named(vec!["u1".to_string()], String::as_str, not_found, ambiguous);
        assert_eq!(one.unwrap(), "u1");
        let none = one_named(Vec::<String>::new(), String::as_str, not_found, ambiguous);
        assert_eq!(none.unwrap_err().to_string(), "none");
        let several = vec!["u2".to_string(), "u1".to_string()];
        let err = one_named(several, String::as_str, not_found, ambiguous).unwrap_err();
        assert_eq!(err.to_string(), "u1,u2", "the uids are sorted");
    }

    #[test]
    fn a_profile_version_is_a_name_at_a_version_a_uid_or_the_draft_of_one() {
        let no_files = |_: &Path| false;
        for text in [
            "aws-stack@1.2.0",
            "64f0a1",
            "aws-stack@1.2.0@draft",
            "64f0a1@draft",
        ] {
            assert!(
                Ref::parse_with(text, no_files).is_profile_version(),
                "{text} names a profile version"
            );
        }
        for text in [
            "releases/dev.json",
            "pack:csi-aws-ebs@1.65.0",
            "cluster:c1",
            "cluster:c1/aws-stack@pending",
            "edgehost:e1",
            "appprofile:shop@1.0.0",
            "tag:env=prod",
            "filter:prod",
            "clustergroup:g1",
        ] {
            assert!(
                !Ref::parse_with(text, no_files).is_profile_version(),
                "{text} does not name a profile version"
            );
        }
    }

    #[test]
    fn cluster_group_forms() {
        let no_files = |_: &Path| false;
        let group = Ref::parse_with("clustergroup:g1", no_files);
        assert_eq!(group, Ref::ClusterGroup { uid: "g1".into() });
        assert_eq!(group.to_string(), "clustergroup:g1");
        assert_eq!(
            Ref::parse_with("clustergroup:", no_files),
            Ref::Uid("clustergroup:".into()),
            "no uid, no cluster group"
        );
    }
}
