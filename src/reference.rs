//! What a user-supplied ref means.

use std::fmt;
use std::path::{Path, PathBuf};

/// A way to name a profile version on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// A snapshot file written earlier by `snapshot`.
    Snapshot(PathBuf),
    /// `<name>@<version>`, resolved through the dashboard listing.
    NameVersion {
        /// Profile name.
        name: String,
        /// Profile version string.
        version: String,
    },
    /// A profile uid.
    Uid(String),
}

impl Ref {
    /// A path to an existing file is a snapshot; `name@version` with both parts non-empty is a
    /// name and version; anything else is a uid.
    pub fn parse(text: &str) -> Ref {
        Self::parse_with(text, |p| p.is_file())
    }

    /// [`Ref::parse`] with a caller-supplied "is this a file" check, for tests.
    pub fn parse_with(text: &str, is_file: impl Fn(&Path) -> bool) -> Ref {
        if is_file(Path::new(text)) {
            return Ref::Snapshot(PathBuf::from(text));
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
}
