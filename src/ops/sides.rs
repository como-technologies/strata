//! The project of each side of `compare` and `same-release`, so that one run reads two projects
//! of one tenant.

use crate::diff::Side;
use crate::error::{Error, Result};

/// The project that `compare` and `same-release` read each side in: `--project-a` and
/// `--project-b`, else `--project` for both sides.
///
/// `T` names a project. The command line gives its flags, and finds the uid of a name only when
/// it connects for that side. So a side that is a record file sends no request. `None` everywhere
/// is the configured project of the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideProjects<T> {
    /// `--project` or `--project-name`: the project of each side that has no flag of its own.
    pub both: Option<T>,
    /// `--project-a` or `--project-name-a`: the project of side A, the first ref.
    pub a: Option<T>,
    /// `--project-b` or `--project-name-b`: the project of side B, the second ref.
    pub b: Option<T>,
}

impl<T> Default for SideProjects<T> {
    fn default() -> Self {
        SideProjects {
            both: None,
            a: None,
            b: None,
        }
    }
}

impl<T> SideProjects<T> {
    /// The project of `side`: the flag of the side, else the flag of both sides.
    pub fn of(&self, side: Side) -> Option<&T> {
        match side {
            Side::A => self.a.as_ref(),
            Side::B => self.b.as_ref(),
        }
        .or(self.both.as_ref())
    }

    /// The project of the ref at `index` of `same-release`: the first ref is side A, the second
    /// is side B, and each later ref has the flag of both sides.
    pub fn of_ref(&self, index: usize) -> Option<&T> {
        match index {
            0 => self.of(Side::A),
            1 => self.of(Side::B),
            _ => self.both.as_ref(),
        }
    }

    /// Refuse a flag of one side when the command has `refs` refs and not two, before any
    /// request.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] when `a` or `b` names a project and `refs` is not 2. A side is one ref
    /// of a pair, so a chain of three refs has no side B alone.
    pub fn check(&self, refs: usize) -> Result<()> {
        if refs == 2 || (self.a.is_none() && self.b.is_none()) {
            return Ok(());
        }
        Err(Error::Usage {
            message: format!(
                "--project-a and --project-b take two refs, and the command has {refs}: give a \
                 record file for each ref, which `strata snapshot <ref> --project-name <name> -o \
                 <file>` writes"
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projects(
        both: Option<&'static str>,
        a: Option<&'static str>,
        b: Option<&'static str>,
    ) -> SideProjects<&'static str> {
        SideProjects { both, a, b }
    }

    #[test]
    fn the_flag_of_a_side_has_priority_over_the_flag_of_both_sides() {
        let p = projects(Some("both"), None, Some("prod"));
        assert_eq!(p.of(Side::A), Some(&"both"));
        assert_eq!(p.of(Side::B), Some(&"prod"));
        assert_eq!(projects(None, None, None).of(Side::A), None);
    }

    #[test]
    fn the_first_ref_is_side_a_the_second_is_side_b_and_a_later_ref_has_the_flag_of_both() {
        let p = projects(Some("both"), Some("dev"), Some("prod"));
        let found: Vec<Option<&&str>> = (0..3).map(|i| p.of_ref(i)).collect();
        assert_eq!(found, [Some(&"dev"), Some(&"prod"), Some(&"both")]);
    }

    #[test]
    fn a_flag_of_one_side_takes_two_refs_and_the_flag_of_both_takes_any_count() {
        assert!(projects(None, Some("dev"), None).check(2).is_ok());
        assert!(projects(Some("both"), None, None).check(3).is_ok());
        for p in [
            projects(None, Some("dev"), None),
            projects(None, None, Some("prod")),
        ] {
            let e = p.check(3).unwrap_err();
            assert_eq!(e.kind(), "usage");
            assert!(e.to_string().contains("has 3"), "{e}");
        }
    }
}
