//! What `snapshot` and `candidate` read: the refs and the flags choose it, before any request.

use std::path::Path;

use crate::api::registry::Registries;
use crate::api::{Palette, PaletteApi, RawCluster, RawProfile};
use crate::candidate::LoadedCandidate;
use crate::error::{Error, Result};
use crate::reference::Ref;
use crate::snapshot::Snapshot;
use crate::term::progress::Progress;

use super::{ClusterQuery, SnapshotOptions, snapshot_for, validate};

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
/// `fetched_at` is the time of the read, in RFC 3339.
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
/// A record can hold a profile version too. [`Candidate::from_snapshot`] says. `fetched_at` is the
/// time of the read, in RFC 3339.
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

/// Build a snapshot of a candidate file.
///
/// Strata reads and checks the file (no network), then resolves its packs through the [`Palette`]
/// that `connect` returns. The snapshot has `source.template` `candidate` and no uid. With
/// `validate`, strata then asks Palette whether it takes the packs of the candidate, as
/// [`profile_validation`](validate::profile_validation) does. `source.paletteValidation` of the
/// record holds the answer: the caller decides what a refusal does. `fetched_at` is the time of
/// the read, in RFC 3339.
///
/// # Errors
///
/// The errors of [`candidate::load`](crate::candidate::load), before `connect` runs. Then the
/// error of `connect`, and the errors of [`candidate::resolve`](fn@crate::candidate::resolve). With
/// `validate`, the errors of [`profile_validation`](validate::profile_validation).
///
/// # Examples
///
/// A candidate with one `manifest` layer needs no registry, so Palette gets no request:
///
/// ```
/// use strata::api::{MemoryApi, Palette};
/// use strata::ops;
/// use strata::term::progress::Progress;
///
/// let dir = tempfile::tempdir()?;
/// let path = dir.path().join("candidate.yaml");
/// std::fs::write(
///     &path,
///     r#"
/// name: demo
/// version: 1.0.0
/// type: add-on
/// layers:
///   - name: policy
///     version: 1.0.0
///     type: manifest
///     manifests:
///       - name: policy
///         content: "kind: Namespace"
/// "#,
/// )?;
///
/// let record = ops::snapshot_from_candidate(
///     &path,
///     false,
///     || Ok(Palette::new(MemoryApi::new())),
///     "2026-09-16T00:00:00Z",
///     &Progress::silent(),
/// )?;
/// assert_eq!(record.source.name, "demo");
/// assert!(record.source.uid.is_empty(), "Palette holds no candidate");
/// assert_eq!(record.packs[0].key, "addon/policy");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn snapshot_from_candidate<A, F>(
    path: &Path,
    validate: bool,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    let loaded = crate::candidate::load(path)?;
    let palette = connect()?;
    // one cache for the resolve and the validate: each registry listing is read one time
    let mut registries = Registries::new(&palette);
    let mut snap = candidate_snapshot(&loaded, &mut registries, fetched_at, progress)?;
    if validate {
        snap.source.palette_validation =
            Some(validate::profile_validation_in(&mut registries, &snap)?);
    }
    Ok(snap)
}

/// Build a snapshot of a candidate that is checked and in memory.
///
/// Strata resolves its packs through the [`Palette`] that `connect` returns.
/// [`snapshot_from_candidate`] reads a file and then does this; a caller with a
/// [`Candidate`](crate::candidate::Candidate) value gives it to
/// [`candidate::check`](crate::candidate::check) and starts here. `fetched_at` is the time of the
/// read, in RFC 3339.
///
/// # Errors
///
/// The error of `connect`, and the errors of [`candidate::resolve`](fn@crate::candidate::resolve).
pub fn snapshot_from_loaded<A, F>(
    loaded: &LoadedCandidate,
    connect: F,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot>
where
    A: PaletteApi,
    F: FnOnce() -> Result<Palette<A>>,
{
    candidate_snapshot(
        loaded,
        &mut Registries::new(&connect()?),
        fetched_at,
        progress,
    )
}

fn candidate_snapshot<A: PaletteApi>(
    loaded: &LoadedCandidate,
    registries: &mut Registries<'_, A>,
    fetched_at: &str,
    progress: &Progress,
) -> Result<Snapshot> {
    let palette = registries.palette();
    let mut snap = crate::candidate::resolve::resolve_with(loaded, registries, progress)?
        .into_snapshot(&loaded.path.display().to_string(), fetched_at)?;
    crate::candidate::presets::warn_preset_options(&snap, progress);
    snap.source.template = crate::snapshot::Template::Candidate.into();
    // Palette does not hold a candidate, so no request asked which clusters use it
    snap.source.consumers = None;
    Ok(snap.with_project(palette.project_uid()))
}

/// Build a snapshot from raw API responses saved on disk, in the project they were fetched in.
///
/// A directory with `cluster.json` is a cluster's; a `reference` of the form
/// `cluster:<uid>/<profile>` then selects the profile. `fetched_at` is the time of this read, in
/// RFC 3339. The record states the time in `scope.json` when the directory has one.
///
/// # Errors
///
/// The errors of [`RawProfile::load_dir`] or [`RawCluster::load_dir`], and the errors of
/// [`Snapshot::from_raw`] or [`Snapshot::from_cluster`].
pub fn snapshot_from_dir(dir: &Path, reference: &str, fetched_at: &str) -> Result<Snapshot> {
    if dir.join(RawCluster::MARKER).is_file() {
        let profile = match Ref::parse_with(reference, |_| false) {
            Ref::Cluster { profile, .. } => profile,
            _ => None,
        };
        let raw = RawCluster::load_dir(dir)?;
        // the responses were read when they were saved; this run read a directory
        let at = raw
            .fetched_at
            .clone()
            .unwrap_or_else(|| fetched_at.to_string());
        return Ok(
            Snapshot::from_cluster(&raw, profile.as_deref(), reference, &at)?
                .with_project(raw.project_uid.as_deref()),
        );
    }
    let raw = RawProfile::load_dir(dir)?;
    let fetched_at = &raw
        .fetched_at
        .clone()
        .unwrap_or_else(|| fetched_at.to_string());
    Ok(Snapshot::from_raw(&raw, reference, fetched_at)?.with_project(raw.project_uid.as_deref()))
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
