//! `behind`: ask Palette whether it accepts the bump of a pin.
//!
//! A bump moves one pack to a newer version. Palette checks such a move against the packs of the
//! clusters. A pack can declare how far a cluster can move from the version it runs, and Palette
//! refuses a move that goes further. The record of a profile version goes to the route of that
//! version, which reads each cluster that runs it. The record of a cluster goes to the route of
//! that cluster. No request changes anything.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use super::behind::Pin;
use super::consumer_list::consumer_names;
use super::repave::moved_pack_body;
use super::validate::{cluster_packs_answer, moved_profile_body, profile_packs_answer};
use crate::api::{Palette, PaletteApi};
use crate::error::Result;
use crate::snapshot::{ClusterSource, Pack, Snapshot, SubjectKind};
use crate::validation::{PaletteRefusal, ProfileValidation};

/// What Palette answers about the bump of a pin, and why it gave no answer.
#[derive(Debug, Default)]
pub(super) struct BumpAnswer {
    /// The errors of Palette for the bump. `None` when no record of the pin could be asked, or when
    /// a request failed and no error stands: Palette accepts only when each record answered.
    pub(super) validation: Option<ProfileValidation>,
    /// Why a request failed, when one did.
    pub(super) error: Option<String>,
}

/// The questions of `behind` to Palette in one scope. It reads the document of a cluster, and the
/// clusters that run a profile, one time each.
pub(super) struct BumpQuestions<'a, A: PaletteApi> {
    palette: &'a Palette<A>,
    /// The documents of the clusters, by uid.
    clusters: BTreeMap<String, Value>,
    /// The names of the clusters that run a profile version, by uid of the version.
    running: BTreeMap<String, Vec<String>>,
}

impl<'a, A: PaletteApi> BumpQuestions<'a, A> {
    pub(super) fn new(palette: &'a Palette<A>) -> Self {
        BumpQuestions {
            palette,
            clusters: BTreeMap::new(),
            running: BTreeMap::new(),
        }
    }

    /// Ask Palette whether it accepts the move of `pin` to `to`, once for each record of
    /// `records`. A record that cannot be asked adds nothing: a candidate, an edge host, a cluster
    /// group, an app profile, a pack, or the update that waits for a cluster.
    ///
    /// The answer holds the errors of all the records, and it accepts when there is none. A request
    /// that fails does not make the pin fail: the answer gives its message, and does not accept.
    pub(super) fn ask(&mut self, records: &[&Snapshot], pin: &Pin, to: &Pack) -> BumpAnswer {
        let mut errors = Vec::new();
        let mut asked = 0;
        let mut failed: Vec<String> = Vec::new();
        for record in records {
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
        let answered = asked > 0 && (failed.is_empty() || !errors.is_empty());
        BumpAnswer {
            validation: answered.then(|| ProfileValidation::refused(errors)),
            error: (!failed.is_empty()).then(|| failed.join("; ")),
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
        if record.source.is_profile_version() {
            // a candidate has no uid, and no cluster runs it
            return if record.source.uid.is_empty() {
                Ok(None)
            } else {
                self.ask_profile(record, pin, to).map(Some)
            };
        }
        match &record.source.cluster {
            Some(cluster) if cluster.kind.is_cluster() && !cluster.pending => {
                self.ask_cluster(cluster, pin, to)
            }
            _ => Ok(None),
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
                .errors()
                .iter()
                .map(|e| e.clone().on_cluster(&cluster.name))
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
