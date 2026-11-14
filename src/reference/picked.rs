//! The version that a ref takes from the versions of one name, and the note of [`LATEST`].

use super::{LATEST, version};

/// The version that a ref takes from the versions of one name, and the highest pre-release above
/// it that [`LATEST`] skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Picked {
    /// The version that the ref takes.
    pub(crate) version: String,
    /// The highest pre-release above `version`, which [`LATEST`] skips. None for an exact
    /// version, and when the pick skips none.
    pub(crate) skipped: Option<String>,
}

impl Picked {
    /// The version of `versions` that `wanted` names, by [`version::pick`]. None when no version
    /// is `wanted`.
    ///
    /// For [`LATEST`], `skipped` is the highest pre-release above the pick, by
    /// [`version::skipped_above`].
    pub(crate) fn of(versions: &[&str], wanted: &str) -> Option<Picked> {
        let picked = version::pick(versions.iter().copied(), |v| *v, wanted)?;
        let skipped = if wanted == LATEST {
            version::skipped_above(versions.iter().copied(), picked)
        } else {
            None
        };
        Some(Picked {
            version: picked.to_string(),
            skipped: skipped.map(str::to_string),
        })
    }

    /// The progress note of a pick of [`LATEST`], such as `@latest is barney@1.0.0`, and the
    /// [`Picked::skip_words`].
    ///
    /// `reference` is the ref of the version that the pick takes, as a user writes it:
    /// `barney@1.0.0` or `appprofile:shop@1.0.0`.
    pub(crate) fn note(&self, reference: &str) -> String {
        format!("@latest is {reference}{}", self.skip_words())
    }

    /// The words that a note adds for the skipped pre-release, such as `; it skips 1.0.1-rc.1, a
    /// pre-release by SemVer`. Empty when the pick skips none.
    ///
    /// A skipped version that does not parse, such as `1.2.develop`, is a pre-release by the order
    /// of [`version::compare`], and not by Semantic Versioning. So the words omit `by SemVer` for
    /// it.
    pub(crate) fn skip_words(&self) -> String {
        let Some(skipped) = &self.skipped else {
            return String::new();
        };
        let rule = if version::parses(skipped) {
            " by SemVer"
        } else {
            ""
        };
        format!("; it skips {skipped}, a pre-release{rule}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #625. The note names the version that `@latest` takes, and the highest pre-release
    /// above it, which it skips. A pre-release that parses is one by Semantic Versioning. Another
    /// form, such as `1.2.develop`, is one by the order of strata, so the note omits `by SemVer`.
    #[test]
    fn the_note_of_latest_names_the_version_and_the_highest_pre_release_that_it_skips() {
        let cases: [(&[&str], &str); 4] = [
            (
                &[
                    "1.0.0",
                    "1.0.1-mr1.770600",
                    "1.0.1-mr2.770685",
                    "0.9.0-rc.1",
                ],
                "@latest is x@1.0.0; it skips 1.0.1-mr2.770685, a pre-release by SemVer",
            ),
            // `2.2.8-hotfix` is below `2.2.8` by SemVer, so `@latest` skips nothing above `2.2.8`
            (&["2.2.8", "2.2.8-hotfix"], "@latest is x@2.2.8"),
            (
                &["1.1.10", "1.2.develop"],
                "@latest is x@1.1.10; it skips 1.2.develop, a pre-release",
            ),
            // a name with only pre-releases gives its highest one, and skips none
            (&["1.0.0-rc.1", "1.0.0-rc.2"], "@latest is x@1.0.0-rc.2"),
        ];
        for (versions, note) in cases {
            let picked = Picked::of(versions, LATEST).unwrap();
            assert_eq!(picked.note(&format!("x@{}", picked.version)), note);
        }
    }

    #[test]
    fn an_exact_version_skips_nothing_and_a_missing_version_is_no_pick() {
        let versions = ["1.0.0", "1.0.1-rc.1"];
        let exact = Picked::of(&versions, "1.0.0").unwrap();
        assert_eq!(
            (exact.version.as_str(), exact.skip_words()),
            ("1.0.0", String::new())
        );
        assert_eq!(Picked::of(&versions, "2.0.0"), None);
        assert_eq!(Picked::of(&[], LATEST), None);
    }
}
