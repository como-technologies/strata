//! What `snapshot` and `candidate` read: the refs and the flags choose it, before any request.

use std::path::Path;

use crate::api::{Palette, PaletteApi};
use crate::error::{Error, Result};
use crate::reference::Ref;
use crate::snapshot::Snapshot;
use crate::term::progress::Progress;

use super::{
    ClusterQuery, SnapshotOptions, snapshot_for, snapshot_from_candidate, snapshot_from_dir,
};

/// The arguments of `snapshot` that choose what it reads.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotRequest<'a> {
    /// The ref to record, as typed.
    pub reference: Option<&'a str>,
    /// `--from-raw`: a directory of the raw answers of Palette.
    pub from_raw: Option<&'a Path>,
    /// `--from-candidate`: a candidate file, or a profile export of Palette.
    pub from_candidate: Option<&'a Path>,
    /// `--dir`: write one record for each cluster of a set into this directory.
    pub dir: Option<&'a Path>,
    /// `--prune`: with `dir`, remove the records of the clusters that the query no longer selects.
    pub prune: bool,
    /// What the record holds beyond the content, and what strata asks Palette.
    pub options: &'a SnapshotOptions,
}

/// What `snapshot` reads: one object, or each cluster of a set.
#[derive(Debug)]
pub enum SnapshotSource<'a> {
    /// Each cluster that `query` selects, as one record in `dir`.
    Set {
        /// The labels and the saved filter that select the clusters.
        query: ClusterQuery,
        /// The directory of the records.
        dir: &'a Path,
        /// Remove the records of the clusters that the query no longer selects.
        prune: bool,
    },
    /// One object.
    Object(ObjectSource<'a>),
}

/// The place that `snapshot` reads one record from.
#[derive(Debug)]
pub enum ObjectSource<'a> {
    /// A candidate file, or a profile export of Palette. Strata resolves its packs.
    Candidate(&'a Path),
    /// The raw answers of Palette that `--raw-dir` saved. The record names `reference`, else the
    /// directory.
    RawDir {
        /// The directory.
        dir: &'a Path,
        /// The ref as typed.
        reference: Option<&'a str>,
    },
    /// What a ref names in Palette.
    Ref(Ref),
}

impl<'a> SnapshotRequest<'a> {
    /// What the arguments say that `snapshot` reads.
    ///
    /// # Errors
    ///
    /// - `Usage` for `validate` with `from_raw` or `dir`.
    /// - `Usage` for `dir` with a ref that does not name a set of clusters.
    /// - `Usage` for no ref, no `from_raw`, and no `from_candidate`.
    /// - `RecordNotASource` for a ref to a record file: `snapshot` writes a record and reads none.
    pub fn source(&self) -> Result<SnapshotSource<'a>> {
        if self.options.validate && (self.from_raw.is_some() || self.dir.is_some()) {
            return Err(Error::Usage {
                message: "--validate asks Palette about the packs of one profile version or \
                          candidate. --from-raw sends no request, and --dir records clusters"
                    .to_string(),
            });
        }
        let reference = self.reference.map(Ref::parse);
        // a set of clusters is one record for each, in a directory
        match (&reference, self.dir) {
            (Some(Ref::ClusterSet { tags, filter }), Some(dir)) => {
                return Ok(SnapshotSource::Set {
                    query: ClusterQuery {
                        tags: tags.clone(),
                        filter: filter.clone(),
                    },
                    dir,
                    prune: self.prune,
                });
            }
            (_, Some(_)) => {
                return Err(Error::Usage {
                    message: "--dir takes a tag: or filter: ref, which names a set of clusters"
                        .to_string(),
                });
            }
            _ => {}
        }
        let object = match (self.from_candidate, self.from_raw, reference) {
            (Some(file), _, _) => ObjectSource::Candidate(file),
            (None, Some(dir), _) => ObjectSource::RawDir {
                dir,
                reference: self.reference,
            },
            // A ref to a file reaches Palette nowhere else, and a path as a uid asks for
            // /v1/clusterprofiles/<path>, which gets a 404 after the retries.
            (None, None, Some(Ref::Snapshot(path))) => {
                return Err(Error::RecordNotASource {
                    path: path.display().to_string(),
                });
            }
            (None, None, Some(reference)) => ObjectSource::Ref(reference),
            (None, None, None) => {
                return Err(Error::Usage {
                    message: "snapshot needs a profile ref or --from-raw DIR".to_string(),
                });
            }
        };
        Ok(SnapshotSource::Object(object))
    }
}

/// Read the record that `source` names, with `options`.
///
/// A candidate and a ref connect through `connect`. Saved answers need no connection.
///
/// # Errors
///
/// `Usage` for an option that does not fit the ref, before any request. Then the error of a read.
pub fn snapshot_object<A, F>(
    source: &ObjectSource<'_>,
    options: &SnapshotOptions,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    match source {
        ObjectSource::Candidate(file) => {
            snapshot_from_candidate(file, options.validate, connect, fetched_at, progress)
        }
        ObjectSource::RawDir { dir, reference } => {
            let reference = reference.map_or_else(|| dir.display().to_string(), str::to_string);
            snapshot_from_dir(dir, &reference, fetched_at)
        }
        ObjectSource::Ref(reference) => {
            snapshot_for(reference, options, connect, fetched_at, progress)
        }
    }
}

/// The record that `candidate` writes from: the profile version that `reference` names, or a record
/// file.
///
/// A record can hold a profile version too, and [`Candidate::from_snapshot`] says.
///
/// [`Candidate::from_snapshot`]: crate::candidate::Candidate::from_snapshot
///
/// # Errors
///
/// `ProfileRefRequired` for a ref that names no profile version, before any request. Then the
/// error of a read.
pub fn snapshot_for_candidate<A, F>(
    reference: &str,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    let parsed = Ref::parse(reference);
    if !parsed.is_profile_version() && !matches!(parsed, Ref::Snapshot(_)) {
        return Err(Error::ProfileRefRequired {
            reference: reference.to_string(),
        });
    }
    snapshot_for(
        &parsed,
        &SnapshotOptions::default(),
        connect,
        fetched_at,
        progress,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::api::MemoryApi;

    fn request<'a>(options: &'a SnapshotOptions) -> SnapshotRequest<'a> {
        SnapshotRequest {
            reference: None,
            from_raw: None,
            from_candidate: None,
            dir: None,
            prune: false,
            options,
        }
    }

    fn usage(request: &SnapshotRequest<'_>) -> String {
        match request.source() {
            Err(Error::Usage { message }) => message,
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    #[test]
    fn a_ref_names_one_object_and_the_files_name_theirs() {
        let options = SnapshotOptions::default();
        let by_ref = SnapshotRequest {
            reference: Some("aws-stack@1.2.0"),
            ..request(&options)
        };
        assert!(matches!(
            by_ref.source().unwrap(),
            SnapshotSource::Object(ObjectSource::Ref(Ref::NameVersion { .. }))
        ));
        let candidate = Path::new("candidate.yaml");
        let from_candidate = SnapshotRequest {
            from_candidate: Some(candidate),
            ..request(&options)
        };
        assert!(matches!(
            from_candidate.source().unwrap(),
            SnapshotSource::Object(ObjectSource::Candidate(file)) if file == candidate
        ));
        let dir = Path::new("raw");
        let from_raw = SnapshotRequest {
            reference: Some("aws-stack@1.2.0"),
            from_raw: Some(dir),
            ..request(&options)
        };
        assert!(matches!(
            from_raw.source().unwrap(),
            SnapshotSource::Object(ObjectSource::RawDir { dir: d, reference: Some("aws-stack@1.2.0") })
                if d == dir
        ));
    }

    #[test]
    fn a_ref_of_labels_with_a_directory_names_a_set_of_clusters() {
        let options = SnapshotOptions::default();
        let dir = Path::new("records");
        let set = SnapshotRequest {
            reference: Some("tag:env=prod"),
            dir: Some(dir),
            prune: true,
            ..request(&options)
        };
        let SnapshotSource::Set {
            query,
            dir: found,
            prune,
        } = set.source().unwrap()
        else {
            panic!("a set");
        };
        assert_eq!(query.tags, ["env=prod"]);
        assert_eq!((found, prune), (dir, true));
    }

    #[test]
    fn a_directory_needs_a_ref_of_a_set_and_a_validate_needs_a_ref_that_sends_a_request() {
        let options = SnapshotOptions::default();
        let one_profile = SnapshotRequest {
            reference: Some("aws-stack@1.2.0"),
            dir: Some(Path::new("records")),
            ..request(&options)
        };
        assert!(usage(&one_profile).starts_with("--dir takes a tag: or filter: ref"));
        let validating = SnapshotOptions {
            validate: true,
            ..SnapshotOptions::default()
        };
        for conflict in [
            SnapshotRequest {
                from_raw: Some(Path::new("raw")),
                ..request(&validating)
            },
            SnapshotRequest {
                reference: Some("tag:env=prod"),
                dir: Some(Path::new("records")),
                ..request(&validating)
            },
        ] {
            assert!(usage(&conflict).starts_with("--validate asks Palette"));
        }
    }

    #[test]
    fn no_ref_and_no_directory_of_answers_is_a_usage_error_and_a_record_is_not_a_source() {
        let options = SnapshotOptions::default();
        assert_eq!(
            usage(&request(&options)),
            "snapshot needs a profile ref or --from-raw DIR"
        );
        let record = SnapshotRequest {
            reference: Some("releases/dev.json"),
            ..request(&options)
        };
        match record.source() {
            Err(Error::RecordNotASource { path }) => assert_eq!(path, "releases/dev.json"),
            other => panic!("expected RecordNotASource, got {other:?}"),
        }
    }

    #[test]
    fn candidate_refuses_a_ref_that_is_no_profile_version_before_it_connects() {
        let never = || -> Result<Palette<MemoryApi>> { panic!("no request before the refusal") };
        for reference in ["cluster:c1", "tag:env=prod", "pack:nginx@1.0.0"] {
            match snapshot_for_candidate(
                reference,
                never,
                "2024-01-01T00:00:00Z",
                &Progress::new(true),
            ) {
                Err(Error::ProfileRefRequired { reference: shown }) => {
                    assert_eq!(shown, reference);
                }
                other => panic!("{reference}: expected ProfileRefRequired, got {other:?}"),
            }
        }
        let missing = PathBuf::from("no-such-record.json");
        let err = snapshot_for_candidate(
            &missing.display().to_string(),
            never,
            "2024-01-01T00:00:00Z",
            &Progress::new(true),
        )
        .unwrap_err();
        assert_eq!(
            err.kind(),
            "io",
            "a record file is read, and that is no request"
        );
    }
}
