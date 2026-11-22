//! The parts of the messages of the errors that are not answers of Palette. They are a list, a
//! ref, a version, a tag, and what to try next.

use crate::config::PROJECT_NAME_VAR;

/// What the text of a lookup names: a name, or a uid. The message of the error of the lookup names
/// the text as such.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identifier {
    /// A name, as `--project-name` gives it.
    Name,
    /// A uid, as the text of a ref `project:<project>/<ref>` can give it.
    Uid,
}

impl Identifier {
    /// `named 'dev'`, or `with the uid '64f2...'`: the words of a message for `text`.
    pub(super) fn words(self, text: &str) -> String {
        match self {
            Identifier::Name => format!("named '{text}'"),
            Identifier::Uid => format!("with the uid '{text}'"),
        }
    }
}

/// `a, b, c`, or `none` for an empty list.
pub(super) fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

/// What to try after a profile name matched nothing. At tenant scope the listing advice is a dead
/// end for a profile of a project. Tenant scope holds only the profiles of the tenant, so the
/// reader needs a project first.
pub(super) fn profile_not_found_hint(name: &str, tenant_scope: bool) -> String {
    if tenant_scope {
        format!(
            "this is tenant scope, which holds only the profiles of the tenant: set {PROJECT_NAME_VAR}, \
             or give --project-name, and `strata list projects` shows the projects this key can see"
        )
    } else {
        format!(
            "`strata list` shows the profiles in it, and `strata list --pack {name}` the profiles \
             with a pack of that name"
        )
    }
}

/// `cluster:<cluster>@pending`: the ref of the update that waits for `cluster`.
pub(super) fn pending_ref(cluster: &str) -> String {
    crate::reference::Ref::Cluster {
        uid: cluster.to_string(),
        profile: None,
        pending: true,
    }
    .to_string()
}

/// ` at version '1.2.3'`, or nothing when the ref and the flags give no version.
pub(super) fn at_version(version: &str) -> String {
    if version.is_empty() {
        String::new()
    } else {
        format!(" at version '{version}'")
    }
}

/// ` with tag '2.x'`, or nothing when the ref and the flags give no tag.
pub(super) fn with_tag(tag: Option<&str>) -> String {
    tag.map(|t| format!(" with tag '{t}'")).unwrap_or_default()
}
