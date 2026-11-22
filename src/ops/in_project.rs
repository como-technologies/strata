//! A ref that names its project, `project:<project>/<ref>`: the command reads the ref in that
//! project, and the record names the ref as typed.

use crate::error::{Error, Result};
use crate::reference::{self, FILTER_PREFIX, Ref, TAG_PREFIX};
use crate::snapshot::Snapshot;

/// `Usage` when `project:<project>/<ref>` holds a ref that belongs to no project, or that already
/// reads each project. Strata sends no request.
///
/// A file and a pack belong to no project: a record names its own project, and a registry serves
/// a pack. `tag:` and `filter:` already search each project that the key can read. A ref names one
/// project, so it holds no second `project:`.
///
/// # Errors
///
/// [`Error::Usage`] for each such ref.
pub(super) fn check(project: &str, inner: &Ref) -> Result<()> {
    let why: String = match inner {
        Ref::Snapshot(_) => {
            "a file belongs to no project, because a record names its own project".into()
        }
        Ref::Pack { .. } => "a pack belongs to no project, because a registry serves it".into(),
        Ref::ClusterSet { .. } => format!(
            "{TAG_PREFIX} and {FILTER_PREFIX} already search each project that the key can read"
        ),
        Ref::InProject { .. } => "a ref names one project".into(),
        _ => return Ok(()),
    };
    Err(Error::Usage {
        message: format!(
            "{}: {why}. Give {inner} alone",
            reference::in_project(project, &inner.to_string()),
        ),
    })
}

/// The ref that a command reads for `reference`: the ref of `project:<project>/<ref>`, else
/// `reference`.
///
/// # Errors
///
/// The `Usage` of [`check`].
pub(super) fn checked(reference: &Ref) -> Result<&Ref> {
    if let Ref::InProject {
        project,
        reference: inner,
    } = reference
    {
        check(project, inner)?;
    }
    Ok(reference.without_project())
}

/// The record that `read` makes of the ref with no project, for `reference`.
///
/// For `project:<project>/<ref>`, strata first refuses a ref that [`check`] refuses. The caller
/// connects in the project, so the record names it in `source.projectUid`. The record names its
/// source as `project:<project>/<ref>`, with the ref that `read` names. Each other ref is the ref
/// that `read` gets.
///
/// # Errors
///
/// The `Usage` of [`check`], before `read` runs. Then the error of `read`.
pub(super) fn read(
    reference: &Ref,
    read: impl FnOnce(&Ref) -> Result<Snapshot>,
) -> Result<Snapshot> {
    let Ref::InProject {
        project,
        reference: inner,
    } = reference
    else {
        return read(reference);
    };
    check(project, inner)?;
    let mut snap = read(inner).map_err(|e| denied(reference, e))?;
    snap.source.reference = reference::in_project(project, &snap.source.reference);
    Ok(snap)
}

/// `e`, an error of a read for `reference`. When the ref names its project and Palette refused
/// the key, it is [`Error::ProjectDenied`], which names the project.
///
/// Strata reads each project with one key. A role of the key can give access to one project and
/// not to another, so the error must say which project refused it. A refusal is an answer that
/// states a denial, such as the code `ResourceAccessDenied`: see [`Error::denial`].
pub(super) fn denied(reference: &Ref, e: Error) -> Error {
    match reference.project() {
        Some(project) if e.denial().is_some() => Error::ProjectDenied {
            project: project.to_string(),
            source: Box::new(e),
        },
        _ => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Method;
    use crate::api::codes::Denial;
    use crate::api::routes::PROFILE_LISTING;

    /// An answer of Palette with `status` and the code `code`.
    fn answer(status: u16, code: &str) -> Error {
        Error::Api {
            method: Method::Get,
            path: PROFILE_LISTING.into(),
            status,
            body: serde_json::json!({"code": code, "message": "m"}).to_string(),
            retry_after: None,
            key_proved: true,
        }
    }

    #[test]
    fn only_a_denial_of_a_read_of_a_ref_in_a_project_names_the_project() {
        let in_prod = Ref::parse_with(&reference::in_project("prod", "aws-stack@1.0.0"), |_| false);
        let denial = || answer(500, Denial::Project.as_str());
        let e = denied(&in_prod, denial());
        assert!(matches!(e, Error::ProjectDenied { .. }), "{e:?}");
        assert!(e.to_string().contains("project 'prod'"), "{e}");
        // the same denial for a ref with no project, and another answer, stay as they are
        let e = denied(in_prod.without_project(), denial());
        assert!(matches!(e, Error::Api { .. }), "{e:?}");
        let e = denied(&in_prod, answer(404, "ResourceNotFound"));
        assert!(matches!(e, Error::Api { .. }), "{e:?}");
    }
}
