//! `behind`: read the bump of a pin, and ask Palette whether it accepts the bump.
//!
//! A bump moves one pack to a newer version. Palette checks such a move against the packs of the
//! clusters. A pack can declare how far a cluster can move from the version it runs, and Palette
//! refuses a move that goes further. The record of a profile version goes to the route of that
//! version, which reads each cluster that runs it. The record of a cluster goes to the route of
//! the profile version that the cluster runs, because the route of a cluster does not check the
//! sent tag of a profile that the cluster runs. No request changes anything.
//!
//! Palette answers in the project of the connection, so strata asks only the records of that
//! project. It counts each record of another project that it can ask, and says so.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use super::{Bump, Pin, PinKey};
use crate::api::registry::Registries;
use crate::api::{Palette, PaletteApi};
use crate::candidate::pack::pack_snapshot_in;
use crate::diff::Diff;
use crate::error::{Error, Result};
use crate::json::{first_nonempty, str_at};
use crate::model::validation::{PaletteRefusal, ProfileValidation};
use crate::ops::consumer_list::consumer_names;
use crate::ops::scope_of;
use crate::ops::validate::{moved_profile_body, moved_template_bodies, profile_packs_answer};
use crate::reference::Ref;
use crate::snapshot::{ClusterSource, Pack, Snapshot, SubjectKind};
use crate::term::progress::Progress;
use crate::text::plural;

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

    /// Ask about the bump of a record of a profile version: see
    /// [`BumpQuestions::ask_version`]. The body has the packs of the record, and `to` in the place
    /// of the pin.
    fn ask_profile(
        &mut self,
        record: &Snapshot,
        pin: &Pin,
        to: &Pack,
    ) -> Result<Vec<PaletteRefusal>> {
        let body = moved_profile_body(record, |p| pin.pins(p), to);
        self.ask_version(&record.source.uid, &body)
    }

    /// Ask about the bump of a record of a cluster, one time for each profile version that the
    /// cluster runs and whose template holds the pin: see [`BumpQuestions::ask_version`]. The
    /// body has the packs of the template as the cluster runs them, and `to` in the place of the
    /// pin. `None` when no profile of the cluster holds the pin now.
    ///
    /// The route of the cluster does not check the sent tag of a profile that the cluster runs,
    /// so it says nothing about the move.
    fn ask_cluster(
        &mut self,
        cluster: &ClusterSource,
        pin: &Pin,
        to: &Pack,
    ) -> Result<Option<Vec<PaletteRefusal>>> {
        let asks = moved_template_bodies(
            self.cluster_document(&cluster.uid)?,
            |c| pin.is_in_template(c),
            to,
        );
        if asks.is_empty() {
            return Ok(None);
        }
        let mut errors = Vec::new();
        for (uid, body) in &asks {
            errors.extend(self.ask_version(uid, body)?);
        }
        Ok(Some(errors))
    }

    /// `POST /v1/clusterprofiles/{uid}/validate/packs` with `body`. Palette checks the packs
    /// against each cluster that runs the version, and names the clusters in each error.
    fn ask_version(&mut self, uid: &str, body: &Value) -> Result<Vec<PaletteRefusal>> {
        let errors = profile_packs_answer(self.palette, Some(uid), body)?;
        if errors.is_empty() {
            return Ok(errors);
        }
        // the refusal stands without the names of the clusters: the line then gives the message
        // of Palette, which holds them
        let names = self.clusters_running(uid).unwrap_or_default();
        Ok(errors.into_iter().map(|e| e.naming(names)).collect())
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

impl Pin {
    /// True when `p` is the pack that this pin names, as a record holds it.
    fn pins(&self, p: &Pack) -> bool {
        PinKey::of(p) == self.key()
    }

    /// True when `c` is the pack that this pin names, as the template of a profile in the
    /// document of a cluster holds it. The template names the pack and its tag.
    fn is_in_template(&self, c: &Value) -> bool {
        str_at(c, "/name") == self.name && str_at(c, "/tag") == self.tag
    }

    /// What identifies this pin in the records.
    fn key(&self) -> PinKey {
        PinKey {
            name: self.name.clone(),
            registry_uid: self.registry_uid.clone(),
            version: self.version.clone(),
            tag: self.tag.clone(),
        }
    }
}

/// Give `pin` its bump: what a move to the newest version does, and the answer of Palette for
/// the records `holding` that pin it. A comparison that fails is the error of `pin`. A pin with no
/// newer version has no bump.
pub(super) fn read_bump<A: PaletteApi>(
    pin: &mut Pin,
    holding: &[&Snapshot],
    listings: &mut Registries<'_, A>,
    questions: &mut BumpQuestions<'_, A>,
    at: &str,
    progress: &Progress,
) {
    let Some(newest) = pin.newer.first() else {
        return;
    };
    let to = newest.version.clone();
    progress.note(&format!("comparing {} {} with {to}", pin.name, pin.version));
    match bump_of(listings, pin, &to, at, progress) {
        Ok((mut bump, target)) => {
            progress.note(&format!(
                "asking Palette about {} {} to {to}",
                pin.name, pin.version
            ));
            let asked = questions.ask(holding, pin, &target);
            bump.palette_validation = asked.validation;
            bump.palette_validation_error = asked.error;
            pin.bump = Some(bump);
        }
        Err(e) => pin.error = Some(e.to_json()),
    }
}

/// What a move from the pinned version of `pin` to the version `to` does: `compare` of the two
/// pack versions, which `listings` reads. Also the pack at the version `to`, with the default
/// values of that version, for the question to Palette. The bump holds no answer of Palette yet.
fn bump_of<A: PaletteApi>(
    listings: &mut Registries<'_, A>,
    pin: &Pin,
    to: &str,
    at: &str,
    progress: &Progress,
) -> Result<(Bump, Pack)> {
    let mut take = |version: &str| {
        pack_snapshot_in(
            listings,
            Some(&pin.registry_uid),
            &pin.name,
            version,
            &Ref::Pack {
                registry: None,
                name: pin.name.clone(),
                version: version.to_string(),
            }
            .to_string(),
            at,
            progress,
        )
    };
    let (a, b) = (take(&pin.version)?, take(to)?);
    let d = crate::ops::compare(&a, &b, at);
    bump_and_target(pin, to, &d, b)
}

/// The bump of a move of `pin` to the version `to`, from `d`, the comparison of the two pack
/// versions. Also the pack at `to`, from `b`, the record of that version.
///
/// # Errors
///
/// [`Error::PackNotFound`] when `d` or `b` holds no pack with the name of the pin. The reads of
/// the registry gave an answer with no such pack, so the fault is in the answer, and not in the
/// command.
fn bump_and_target(pin: &Pin, to: &str, d: &Diff, b: Snapshot) -> Result<(Bump, Pack)> {
    let no_pack = || Error::PackNotFound {
        name: pin.name.clone(),
        version: to.to_string(),
        tag: None,
        registry: first_nonempty(&[&pin.registry, &pin.registry_uid]),
        unsynced: Vec::new(),
    };
    let bump = Bump::of(to, d, &pin.name).ok_or_else(no_pack)?;
    let target = b
        .packs
        .into_iter()
        .find(|p| p.name == pin.name)
        .ok_or_else(no_pack)?;
    Ok((bump, target))
}
