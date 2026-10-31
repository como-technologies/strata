//! `behind`: ask Palette whether it accepts the bump of a pin.
//!
//! A bump moves one pack to a newer version. Palette checks such a move against the packs of the
//! clusters. A pack can declare how far a cluster can move from the version it runs, and Palette
//! refuses a move that goes further. The record of a profile version goes to the route of that
//! version, which reads each cluster that runs it. The record of a cluster goes to the route of
//! that cluster. No request changes anything.
//!
//! Palette answers in the project of the connection, so strata asks only the records of that
//! project. It counts each record of another project that it can ask, and says so.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use super::behind::Pin;
use super::consumer_list::consumer_names;
use super::repave::moved_pack_body;
use super::scope_of;
use super::validate::{cluster_packs_answer, moved_profile_body, profile_packs_answer};
use crate::api::{Palette, PaletteApi};
use crate::error::Result;
use crate::snapshot::{ClusterSource, Pack, Snapshot, SubjectKind};
use crate::text::plural;
use crate::validation::{PaletteRefusal, ProfileValidation};

/// What Palette answers about the bump of a pin, and why it gave no answer.
#[derive(Debug, Default)]
pub(super) struct BumpAnswer {
    /// The errors of Palette for the bump. `None` when strata can ask no record of the pin. Also
    /// `None` when a request failed or strata did not ask a record, and no error stands. Strata
    /// says that Palette accepts only when it asked each record and each request answered.
    pub(super) validation: Option<ProfileValidation>,
    /// Why a request failed, and the records of another project that strata did not ask, when
    /// there are some.
    pub(super) error: Option<String>,
}

/// What Palette checks a bump on, in a record.
enum Asked<'r> {
    /// A profile version that Palette holds.
    Profile,
    /// A cluster that runs its profiles now.
    Cluster(&'r ClusterSource),
}

impl<'r> Asked<'r> {
    /// What Palette checks in `record`, or `None` when Palette cannot check it. Palette cannot
    /// check a candidate, an edge host, a cluster group, an app profile, a pack, or the update
    /// that waits for a cluster.
    fn of(record: &'r Snapshot) -> Option<Asked<'r>> {
        if record.source.is_profile_version() {
            // a candidate has no uid, and no cluster runs it
            return (!record.source.uid.is_empty()).then_some(Asked::Profile);
        }
        match &record.source.cluster {
            Some(cluster) if cluster.kind.is_cluster() && !cluster.pending => {
                Some(Asked::Cluster(cluster))
            }
            _ => None,
        }
    }
}

/// The questions of `behind` to Palette in one scope. It reads the document of a cluster, and the
/// clusters that run a profile, one time each.
pub(super) struct BumpQuestions<'a, A: PaletteApi> {
    palette: &'a Palette<A>,
    /// The project of the connection: `None` is the tenant scope.
    scope: Option<&'a str>,
    /// The documents of the clusters, by uid.
    clusters: BTreeMap<String, Value>,
    /// The names of the clusters that run a profile version, by uid of the version.
    running: BTreeMap<String, Vec<String>>,
}

impl<'a, A: PaletteApi> BumpQuestions<'a, A> {
    pub(super) fn new(palette: &'a Palette<A>, scope: Option<&'a str>) -> Self {
        BumpQuestions {
            palette,
            scope,
            clusters: BTreeMap::new(),
            running: BTreeMap::new(),
        }
    }

    /// Ask Palette whether it accepts the move of `pin` to `to`, once for each record of
    /// `holders` in the scope of the connection. A record that Palette cannot check adds nothing:
    /// see [`Asked::of`].
    ///
    /// The answer holds the errors of all the records, and it accepts when there is none. A request
    /// that fails does not make the pin fail: the answer gives its message, and does not accept.
    /// It does not ask a record of another project. It counts the records of another project
    /// that Palette can check, and the answer does not accept.
    pub(super) fn ask(&mut self, holders: &[&Snapshot], pin: &Pin, to: &Pack) -> BumpAnswer {
        let mut errors = Vec::new();
        let mut asked = 0;
        let mut failed: Vec<String> = Vec::new();
        let mut elsewhere: BTreeMap<&str, usize> = BTreeMap::new();
        for record in holders {
            if scope_of(&record.source.project_uid) != self.scope {
                if Asked::of(record).is_some() {
                    *elsewhere.entry(&record.source.project_uid).or_default() += 1;
                }
                continue;
            }
            match self.ask_record(record, pin, to) {
                Ok(Some(found)) => {
                    asked += 1;
                    errors.extend(found);
                }
                Ok(None) => {}
                Err(e) => failed.push(e.to_string()),
            }
        }
        failed.dedup();
        let complete = failed.is_empty() && elsewhere.is_empty();
        let answered = asked > 0 && (complete || !errors.is_empty());
        let problems: Vec<String> = failed
            .into_iter()
            .chain(
                elsewhere
                    .into_iter()
                    .map(|(project, n)| not_asked(project, n)),
            )
            .collect();
        BumpAnswer {
            validation: answered.then(|| ProfileValidation::refused(errors)),
            error: (!problems.is_empty()).then(|| problems.join("; ")),
        }
    }

    /// The errors of Palette for the move in one record, or `None` for a record that cannot be
    /// asked.
    fn ask_record(
        &mut self,
        record: &Snapshot,
        pin: &Pin,
        to: &Pack,
    ) -> Result<Option<Vec<PaletteRefusal>>> {
        match Asked::of(record) {
            Some(Asked::Profile) => self.ask_profile(record, pin, to).map(Some),
            Some(Asked::Cluster(cluster)) => self.ask_cluster(cluster, pin, to),
            None => Ok(None),
        }
    }

    /// `POST /v1/clusterprofiles/{uid}/validate/packs` with the packs of the record, and `to` in
    /// the place of the pin. Palette checks the packs against each cluster that runs the version.
    fn ask_profile(
        &mut self,
        record: &Snapshot,
        pin: &Pin,
        to: &Pack,
    ) -> Result<Vec<PaletteRefusal>> {
        let uid = &record.source.uid;
        let body = moved_profile_body(record, |p| pin.pins(p), to);
        let errors = profile_packs_answer(self.palette, Some(uid), &body)?;
        if errors.is_empty() {
            return Ok(errors);
        }
        // the refusal stands without the names of the clusters: the line then gives the message
        // of Palette, which holds them
        let names = self.clusters_running(uid).unwrap_or_default();
        Ok(errors.into_iter().map(|e| e.naming(names)).collect())
    }

    /// `POST /v1/spectroclusters/{uid}/validate/packs` with the profiles of the cluster, and `to`
    /// in the place of the pin. `None` when no profile of the cluster holds the pin now.
    fn ask_cluster(
        &mut self,
        cluster: &ClusterSource,
        pin: &Pin,
        to: &Pack,
    ) -> Result<Option<Vec<PaletteRefusal>>> {
        let palette = self.palette;
        let doc = self.cluster_document(&cluster.uid)?;
        let Some(body) = moved_pack_body(doc, |c| pin.is_in_template(c), to) else {
            return Ok(None);
        };
        let answer = cluster_packs_answer(palette, &cluster.uid, &body)?;
        Ok(Some(
            answer
                .answer
                .into_errors()
                .into_iter()
                .map(|e| e.on_cluster(&cluster.name))
                .collect(),
        ))
    }

    /// The document of the cluster `uid`, read one time.
    fn cluster_document(&mut self, uid: &str) -> Result<&Value> {
        let doc = match self.clusters.entry(uid.to_string()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => {
                new.insert(self.palette.subject_document(SubjectKind::Cluster, uid)?)
            }
        };
        Ok(doc)
    }

    /// The names of the clusters that run the profile version `uid`, read one time.
    fn clusters_running(&mut self, uid: &str) -> Result<&[String]> {
        let names = match self.running.entry(uid.to_string()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => new.insert(consumer_names(&self.palette.profile_document(uid)?)),
        };
        Ok(names)
    }
}

/// `1 record of project p2 was not asked`, for the `n` records of `project`. An empty `project` is
/// the tenant scope.
fn not_asked(project: &str, n: usize) -> String {
    let place = if project.is_empty() {
        "the tenant scope".to_string()
    } else {
        format!("project {project}")
    };
    format!(
        "{n} {} of {place} {} not asked",
        plural(n, "record", "records"),
        plural(n, "was", "were")
    )
}
