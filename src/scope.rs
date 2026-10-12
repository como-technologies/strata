//! The scope of a profile version: a project, the tenant, or the system.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::json::str_at;

/// Where a profile version lives. Palette gives it in `metadata.annotations.scope` of the profile
/// document, and the profile search takes it as the filter `scope`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ProfileScope {
    /// A profile of one project. The listing of that project holds it.
    Project,
    /// A profile of the tenant. The listing of each project holds it, and so does the listing
    /// at tenant scope.
    Tenant,
    /// A profile of the system, which Spectro Cloud publishes. A listing holds it only when
    /// Palette shares it with the tenant.
    System,
}

impl ProfileScope {
    /// The scope as Palette names it: `project`, `tenant`, or `system`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Tenant => "tenant",
            Self::System => "system",
        }
    }

    /// The scope that Palette names `text`, or none for another text.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "project" => Some(Self::Project),
            "tenant" => Some(Self::Tenant),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    /// The scope of a listing: [`ProfileScope::Project`] in a project, and
    /// [`ProfileScope::Tenant`] at tenant scope, where `project_uid` is empty.
    pub(crate) fn of_listing(project_uid: &str) -> Self {
        if project_uid.is_empty() {
            Self::Tenant
        } else {
            Self::Project
        }
    }

    /// The scope of the profile document `doc`, from `metadata.annotations.scope`. None when the
    /// document does not give it.
    pub(crate) fn of_document(doc: &Value) -> Option<Self> {
        Self::parse(str_at(doc, "/metadata/annotations/scope"))
    }

    /// The scopes whose versions a listing of `self` holds beside its own. A project lists the
    /// profiles of the tenant and of the system too. Tenant scope lists the profiles of the
    /// system too. `list` asks the profile search for the versions of each one.
    pub(crate) fn others_listed(self) -> &'static [ProfileScope] {
        match self {
            Self::Project => &[Self::Tenant, Self::System],
            Self::Tenant => &[Self::System],
            Self::System => &[],
        }
    }
}

impl fmt::Display for ProfileScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A profile uid with its scope, when strata could read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScopedUid {
    /// Profile uid.
    pub uid: String,
    /// The scope of the version. None when its document does not give it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ProfileScope>,
}

impl fmt::Display for ScopedUid {
    /// `64e0000000000000000000c3 (tenant)`, or the uid alone when the scope is not known.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.scope {
            Some(scope) => write!(f, "{} ({scope})", self.uid),
            None => f.write_str(&self.uid),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_scope_parses_and_prints_as_palette_names_it() {
        for scope in [
            ProfileScope::Project,
            ProfileScope::Tenant,
            ProfileScope::System,
        ] {
            assert_eq!(ProfileScope::parse(scope.as_str()), Some(scope));
            assert_eq!(scope.to_string(), scope.as_str());
            assert_eq!(
                serde_json::to_value(scope).unwrap(),
                Value::from(scope.as_str())
            );
        }
        assert_eq!(ProfileScope::parse("global"), None);
    }

    #[test]
    fn the_scope_of_a_document_is_its_annotation_and_a_listing_has_the_scope_of_its_project() {
        let doc = json!({"metadata": {"annotations": {"scope": "tenant"}}});
        assert_eq!(ProfileScope::of_document(&doc), Some(ProfileScope::Tenant));
        assert_eq!(ProfileScope::of_document(&json!({"metadata": {}})), None);
        assert_eq!(ProfileScope::of_listing("p1"), ProfileScope::Project);
        assert_eq!(ProfileScope::of_listing(""), ProfileScope::Tenant);
        assert_eq!(
            ProfileScope::Project.others_listed(),
            [ProfileScope::Tenant, ProfileScope::System]
        );
        assert_eq!(ProfileScope::Tenant.others_listed(), [ProfileScope::System]);
    }

    #[test]
    fn a_scoped_uid_prints_its_scope_when_it_has_one() {
        let with = ScopedUid {
            uid: "u1".into(),
            scope: Some(ProfileScope::Tenant),
        };
        let without = ScopedUid {
            uid: "u2".into(),
            scope: None,
        };
        assert_eq!(with.to_string(), "u1 (tenant)");
        assert_eq!(without.to_string(), "u2");
        assert_eq!(
            serde_json::to_value(&without).unwrap(),
            json!({"uid": "u2"})
        );
    }
}
