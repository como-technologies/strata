//! The reads of a cluster, of a cluster group, and of an edge host.
//!
//! They give the summary, the events, the raw responses of a record, and the cloud config of a
//! cluster. They also give the cloud account, the resolved values, and the repave and validate
//! routes.

use serde_json::Value;

use super::{
    CLOUD_ACCOUNTS, CLOUD_CONFIGS, CLUSTER_EVENTS, CLUSTER_EVENTS_LIMIT, CLUSTER_GROUP_DASHBOARD,
    CLUSTER_NOTIFICATIONS, CLUSTER_SUMMARIES, CLUSTERS, ConfigPart, PROFILES, Palette,
    VariablesRead, collection, named_entries, no_variables, or_unread, own_manifests, route,
    search_all, sorted_names, uids_named, unavailable_note, with_limit,
};
use crate::api::{PaletteApi, RawCluster, ReadErrors};
use crate::error::{Error, Result, found};
use crate::json::{arr, first_nonempty, str_at, take_items, text};
use crate::reference::one_named;
use crate::snapshot::SubjectKind;
use crate::term::progress::Progress;

/// The route of the document of the object `uid` of `kind`, such as `/v1/spectroclusters/{uid}` for
/// a cluster.
pub fn subject_path(kind: SubjectKind, uid: &str) -> String {
    route(collection(kind), &[uid])
}

/// The route of the cluster summary: `/v1/dashboard/spectroclusters/{uid}`.
pub fn cluster_summary_path(uid: &str) -> String {
    route(CLUSTER_SUMMARIES, &[uid])
}

/// The route of the cloud account `uid` of the cloud type `kind`:
/// `/v1/cloudaccounts/{kind}/{uid}`.
pub fn cloud_account_path(kind: &str, uid: &str) -> String {
    route(CLOUD_ACCOUNTS, &[kind, uid])
}

/// The route of the cloud config `uid` of the cloud type `kind`: `/v1/cloudconfigs/{kind}/{uid}`.
pub fn cloud_config_path(kind: &str, uid: &str) -> String {
    route(CLOUD_CONFIGS, &[kind, uid])
}

/// The route of the events of a cluster, with the page size that strata reads:
/// `/v1/events/components/spectrocluster/{uid}?limit=50`.
pub fn cluster_events_path(uid: &str) -> String {
    format!(
        "{}?limit={CLUSTER_EVENTS_LIMIT}",
        route(CLUSTER_EVENTS, &[uid])
    )
}

impl<A: PaletteApi> Palette<A> {
    /// The cluster summary, `GET /v1/dashboard/spectroclusters/{uid}`: what the dashboard shows
    /// for one cluster. Its `status.health` is the health of the cluster, which the cluster
    /// document does not carry.
    ///
    /// # Errors
    ///
    /// The error of the request, also [`Error::Api`] with HTTP 404 when Palette has no cluster with
    /// the uid `uid`.
    pub fn fetch_cluster_summary(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&cluster_summary_path(uid))
    }

    /// The events of the cluster `uid`, `GET /v1/events/components/spectrocluster/{uid}?limit=50`:
    /// the first 50 events that Palette keeps for it, as the `items` of the answer. Palette gives
    /// `items: null` when the cluster has none, and the list is then empty. The read does not page.
    ///
    /// # Errors
    ///
    /// The error of the request, also [`Error::Api`] with HTTP 404 when Palette has no cluster with
    /// the uid `uid`.
    pub fn cluster_events(&self, uid: &str) -> Result<Vec<Value>> {
        Ok(take_items(self.api.get_json(&cluster_events_path(uid))?))
    }

    /// The values that the cluster `uid` gives the variables of its profiles,
    /// `GET /v1/spectroclusters/{uid}/variables`, and whether the route is missing.
    ///
    /// The answer is an array with one entry for each applied profile that declares a variable:
    /// [`profile_variables`](crate::api::profile_variables) reads it. Palette masks the value of
    /// a sensitive variable, and the answer holds the mask. An older Palette has no route: the
    /// answer is then `[]` and `route_missing` is true.
    ///
    /// # Errors
    ///
    /// The error of the request, except a missing route. [`Error::Api`] with HTTP 404 when Palette
    /// has no cluster with the uid `uid`.
    pub fn cluster_variables(&self, uid: &str, progress: &Progress) -> Result<VariablesRead> {
        self.variables_or(&route(CLUSTERS, &[uid]), serde_json::json!([]), progress)
    }

    /// The raw documents of the cluster, edge host, or cluster group `id`, in the shape of a
    /// cluster.
    ///
    /// `id` is a uid, or, when no object of `kind` has that uid, the name of one object of the kind
    /// in the scope. Strata reads the documents by uid in each case.
    ///
    /// A cluster: its document with its applied profile templates, the pack manifests of each
    /// applied profile, and the variables of each. An edge host: `GET /v1/edgehosts/{uid}` gives
    /// the bound profiles in `spec.clusterProfileTemplates`. The host holds its own copy of each
    /// manifest, which `GET /v1/edgehosts/{uid}/pack/manifests/{manifestUid}` reads. A cluster
    /// group: `GET /v1/clustergroups/{uid}` gives the profiles that the group pins for its
    /// virtual clusters. The group holds its own copy of each manifest, which
    /// `GET /v1/clusterprofiles/{uid}/packs/{pack}/manifests/{manifestUid}` reads by its uid.
    ///
    /// The variables and the rest of each pack of a host or a group come from its profile, and
    /// neither has notifications. A missing variables route is "no variables". Each other error
    /// returns.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails. [`Error::ClusterNotFound`],
    /// [`Error::EdgeHostNotFound`], or [`Error::ClusterGroupNotFound`] when `id` is no uid and no
    /// name in the scope. The `NameAmbiguous` kind of the same object when two have the name.
    pub fn fetch_subject(
        &self,
        kind: SubjectKind,
        id: &str,
        progress: &Progress,
    ) -> Result<RawCluster> {
        let doc = self.subject_by_id(kind, id, progress)?;
        self.subject_raw(kind, doc, progress)
    }

    /// The document of the cluster, edge host, or cluster group with the uid `uid`, and no
    /// other.
    ///
    /// The routes are `GET /v1/spectroclusters/{uid}`, `GET /v1/edgehosts/{uid}`, and `GET
    /// /v1/clustergroups/{uid}`. Strata tries no name after a 404. Palette keeps the document of
    /// a deleted cluster, with the state `Deleted`.
    ///
    /// # Errors
    ///
    /// Each error of the request, also [`Error::Api`] with HTTP 404 when Palette has no object
    /// with the uid `uid`.
    pub fn subject_document(&self, kind: SubjectKind, uid: &str) -> Result<Value> {
        self.api.get_json(&subject_path(kind, uid))
    }

    /// The document of the object `id` of `kind`: [`Palette::subject_document`] of the uid `id`.
    ///
    /// When no object has that uid, it is the document of the one object named `id` in the scope.
    /// `progress` gets a note for the read, and a note that names the uid of a name.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails. [`Error::ClusterNotFound`],
    /// [`Error::EdgeHostNotFound`], or [`Error::ClusterGroupNotFound`] when `id` is no uid and no
    /// name in the scope. The `NameAmbiguous` kind of the same object when two have the name.
    pub fn subject_by_id(&self, kind: SubjectKind, id: &str, progress: &Progress) -> Result<Value> {
        progress.note(&format!("fetching {} {id}", kind.noun()));
        match found(self.subject_document(kind, id))? {
            Some(doc) => Ok(doc),
            None => {
                let uid = self.subject_uid_named(kind, id)?;
                progress.note(&format!("{} '{id}' is {uid}", kind.noun()));
                self.subject_document(kind, &uid)
            }
        }
    }

    /// The uid of the one object of `kind` in the scope that has the name `name`.
    fn subject_uid_named(&self, kind: SubjectKind, name: &str) -> Result<String> {
        match kind {
            SubjectKind::Cluster => self.cluster_uid_named(name),
            SubjectKind::EdgeHost => self.edge_host_uid_named(name),
            SubjectKind::ClusterGroup => self.cluster_group_uid_named(name),
        }
    }

    /// The raw documents of a cluster, an edge host, or a cluster group, from the document that
    /// [`Palette::subject_document`] gives: the rest of what [`Palette::fetch_subject`] reads.
    pub fn subject_raw(
        &self,
        kind: SubjectKind,
        doc: Value,
        progress: &Progress,
    ) -> Result<RawCluster> {
        let uid = text(&doc, "/metadata/uid");
        match kind {
            SubjectKind::Cluster => self.cluster_raw(&uid, doc, progress),
            // an edge host keeps each copy of a manifest under its own uid, and a cluster group
            // keeps it with the profile
            SubjectKind::EdgeHost => self.holder_raw(doc, progress, |_, _, muid| {
                self.edge_host_manifest(&uid, muid)
            }),
            SubjectKind::ClusterGroup => self.holder_raw(doc, progress, |puid, name, muid| {
                self.profile_manifest(puid, name, muid)
            }),
        }
    }

    /// The rest of the raw documents of the cluster `uid`, whose document is `cluster`.
    fn cluster_raw(&self, uid: &str, cluster: Value, progress: &Progress) -> Result<RawCluster> {
        let manifests = self
            .api
            .get_json(&route(CLUSTERS, &[uid, "profiles", "packs", "manifests"]))?;
        let variables = self.cluster_variables(uid, progress)?;
        // the updates and the repave that wait, the bindings, and the namespaces are beside the
        // copy, not part of it. So strata records the error of a read, and still takes the record
        let (notifications, notifications_error) = or_unread(
            self.cluster_notifications(uid).map(Value::Array),
            &format!("notifications of cluster {uid}"),
            progress,
        )?;
        // the cluster document gives only the state of a repave; its own route gives the reasons
        let (repave, repave_error) = if crate::repave::is_pending(&cluster) {
            or_unread(
                self.cluster_repave(uid),
                &format!("reasons of the repave of cluster {uid}"),
                progress,
            )?
        } else {
            (None, None)
        };
        let (rbacs, rbacs_error) = self.cluster_config(uid, ConfigPart::Rbacs, progress)?;
        let (namespaces, namespaces_error) =
            self.cluster_config(uid, ConfigPart::Namespaces, progress)?;
        Ok(RawCluster {
            cluster,
            manifests,
            variables: variables.answer,
            variables_unavailable: variables.route_missing,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            notifications,
            repave,
            resolved_values: None,
            cloud_config: None,
            rbacs,
            namespaces,
            errors: ReadErrors {
                notifications: notifications_error,
                repave: repave_error,
                rbacs: rbacs_error,
                namespaces: namespaces_error,
            },
        })
    }

    /// The raw documents of a cluster as they would be after the cluster accepts the update that
    /// waits: `GET /v1/spectroclusters/{uid}/profileUpdates` gives `profiles[]`, each a profile
    /// template in the shape of `spec.clusterProfileTemplates`, and they take the place of the
    /// templates of the cluster. The manifests of each come from its profile, `GET
    /// /v1/clusterprofiles/{uid}/packs/manifests`, since the cluster holds no copy of them yet.
    pub fn with_pending(&self, mut raw: RawCluster) -> Result<RawCluster> {
        let uid = text(&raw.cluster, "/metadata/uid");
        let updates = self
            .api
            .get_json(&route(CLUSTERS, &[&uid, "profileUpdates"]))?;
        let profiles = arr(&updates, "/profiles").to_vec();
        raw.manifests = self.profile_manifests(&profiles)?;
        raw.cluster["spec"]["clusterProfileTemplates"] = Value::Array(profiles);
        Ok(raw)
    }

    /// The manifests of the profile of each template, `GET
    /// /v1/clusterprofiles/{uid}/packs/manifests`, in the shape of the manifests of a cluster:
    /// `{"profiles": [{metadata, spec}]}`.
    fn profile_manifests(&self, templates: &[Value]) -> Result<Value> {
        let mut manifests = Vec::with_capacity(templates.len());
        for t in templates {
            let d = self.profile_pack_manifests(str_at(t, "/uid"))?;
            manifests.push(serde_json::json!({"metadata": d["metadata"], "spec": d["spec"]}));
        }
        Ok(serde_json::json!({ "profiles": manifests }))
    }

    /// The virtual clusters of a cluster group, `POST
    /// /v1/dashboard/clustergroups/{uid}/virtualClusters`, every page: the clusters that run the
    /// profiles that the group pins.
    pub fn cluster_group_members(&self, uid: &str) -> Result<Vec<Value>> {
        self.searched(
            &with_limit(&route(CLUSTER_GROUP_DASHBOARD, &[uid, "virtualClusters"])),
            &search_all(),
        )
    }

    /// The uid of the one cluster group in the scope that has the name `name`, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    fn cluster_group_uid_named(&self, name: &str) -> Result<String> {
        let groups = self.cluster_groups()?;
        one_named(
            uids_named(&groups, name),
            String::as_str,
            || Error::ClusterGroupNotFound {
                cluster_group: name.to_string(),
                available: sorted_names(&groups),
            },
            |uids| Error::ClusterGroupNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The raw documents of an object that holds profiles in `spec.clusterProfileTemplates` and is
    /// not a cluster.
    ///
    /// The manifests and the variables of each profile come from the profile. Such an object
    /// gives no values to the variables, so a record holds the definitions of each profile, as a
    /// profile record does. It has no notifications. `read_copy` reads the copy that the holder
    /// keeps of one manifest. Its arguments are the uid of the profile, the name of the pack, and
    /// the uid of the copy.
    fn holder_raw(
        &self,
        doc: Value,
        progress: &Progress,
        read_copy: impl Fn(&str, &str, &str) -> Result<Value>,
    ) -> Result<RawCluster> {
        let templates = arr(&doc, "/spec/clusterProfileTemplates").to_vec();
        let mut variables = Vec::with_capacity(templates.len());
        let mut variables_unavailable = false;
        for t in &templates {
            let puid = str_at(t, "/uid");
            let read = self.variables_or(&route(PROFILES, &[puid]), no_variables(), progress)?;
            variables_unavailable |= read.route_missing;
            variables.push(
                serde_json::json!({"profileUid": puid, "variables": read.answer["variables"]}),
            );
        }
        let mut manifests = self.profile_manifests(&templates)?;
        own_manifests(&templates, read_copy, &mut manifests)?;
        Ok(RawCluster {
            manifests,
            cluster: doc,
            variables: Value::Array(variables),
            variables_unavailable,
            project_uid: self.project_uid().map(str::to_string),
            fetched_at: None,
            notifications: None,
            repave: None,
            resolved_values: None,
            cloud_config: None,
            rbacs: None,
            namespaces: None,
            errors: ReadErrors::default(),
        })
    }

    /// The uid of the one edge host in the scope that has the name `name`.
    fn edge_host_uid_named(&self, name: &str) -> Result<String> {
        let hosts = named_entries(&Value::Array(self.edge_hosts()?));
        one_named(
            uids_named(&hosts, name),
            String::as_str,
            || Error::EdgeHostNotFound {
                edge_host: name.to_string(),
                available: sorted_names(&hosts),
            },
            |uids| Error::EdgeHostNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The cloud config of a cluster, `GET /v1/cloudconfigs/{cloudType}/{configUid}`, from the
    /// `spec.cloudConfigRef` of the cluster document: its machine pools and its cloud part.
    ///
    /// # Errors
    ///
    /// [`Error::NoCloudConfig`] when the cluster document names no cloud config. The error of the
    /// first request that fails.
    pub fn cloud_config(&self, cluster: &Value) -> Result<Value> {
        let kind = RawCluster::cloud_type(cluster);
        let uid = str_at(cluster, "/spec/cloudConfigRef/uid");
        if kind.is_empty() || uid.is_empty() {
            return Err(Error::NoCloudConfig {
                cluster: first_nonempty(&[
                    str_at(cluster, "/metadata/uid"),
                    str_at(cluster, "/metadata/name"),
                ]),
            });
        }
        self.api.get_json(&cloud_config_path(kind, uid))
    }

    /// The cloud account `uid` of the cloud type `kind`, `GET /v1/cloudaccounts/{kind}/{uid}`.
    /// `kind` is the cloud type that the route names, such as `aws`. A managed cluster uses the
    /// account of its cloud: an `eks` cluster uses an `aws` account. The document can hold
    /// secrets, and the caller must not write them to a file or to an output.
    ///
    /// # Errors
    ///
    /// The error of the request, also [`Error::Api`] with HTTP 404 when Palette has no such
    /// account.
    pub fn cloud_account(&self, kind: &str, uid: &str) -> Result<Value> {
        self.api.get_json(&cloud_account_path(kind, uid))
    }

    /// The value Palette gives each macro for the cluster or the cluster group `uid`: `GET
    /// /v1/spectroclusters/{uid}/packs/resolvedValues` or `GET
    /// /v1/clustergroups/{uid}/packs/resolvedValues`. The answer has `profiles[]`, each with its
    /// `uid` and a `resolved` map from each placeholder, such as `{{.spectro.macro.region}}`, to
    /// its value. The map holds every macro Palette can resolve for the profile, the system ones
    /// too, and not only the ones it uses.
    ///
    /// # Errors
    ///
    /// [`Error::Usage`] for an edge host, before any request: strata reads no macro values for
    /// an edge host. An error of the request otherwise.
    pub fn resolved_values(&self, kind: SubjectKind, uid: &str) -> Result<Value> {
        if kind == SubjectKind::EdgeHost {
            return Err(Error::Usage {
                message: "strata reads no macro values for an edge host".to_string(),
            });
        }
        self.api
            .get_json(&route(collection(kind), &[uid, "packs", "resolvedValues"]))
    }

    /// Palette's answer to whether a change of the profiles of a cluster replaces its nodes:
    /// `POST /v1/spectroclusters/{uid}/validate/repave`. The request changes nothing.
    pub fn validate_repave(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(CLUSTERS, &[uid, "validate", "repave"]), body)
    }

    /// Palette's answer to whether a change of the profiles of the cluster template `uid` replaces
    /// the nodes of its clusters: `POST /v1/spectroclusters/clusterTemplates/{uid}/validate/repave`.
    /// The body names profiles by uid, with no pack values. The request changes nothing.
    ///
    /// # Errors
    ///
    /// An error of the request, such as a 404 for a template that Palette does not have.
    pub fn validate_template_repave(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api.post_json(
            &route(CLUSTERS, &["clusterTemplates", uid, "validate", "repave"]),
            body,
        )
    }

    /// Palette's answer to whether it accepts the packs of a change of the profiles of a cluster:
    /// `POST /v1/spectroclusters/{uid}/validate/packs`, with the body of `validate/repave`. The
    /// request changes nothing.
    pub fn validate_packs(&self, uid: &str, body: &Value) -> Result<Value> {
        self.api
            .post_json(&route(CLUSTERS, &[uid, "validate", "packs"]), body)
    }

    /// One part of the config of the cluster `uid`, `GET
    /// /v1/spectroclusters/{uid}/config/{part}`: its role bindings or its namespaces, and why
    /// Palette did not give them.
    ///
    /// Palette gives `items`, one entry for each. When Palette has no such route (see
    /// [`Error::is_missing_route`]), the record holds null for the part and no error, and not an
    /// empty list. Another answer of Palette that is an error gives no part, and its error: see
    /// [`or_unread`].
    ///
    /// # Errors
    ///
    /// Each error of the request that is not an answer of Palette.
    fn cluster_config(
        &self,
        uid: &str,
        part: ConfigPart,
        progress: &Progress,
    ) -> Result<(Option<Value>, Option<String>)> {
        let path = route(CLUSTERS, &[uid, "config", part.route()]);
        let what = format!("{} of cluster {uid}", part.noun());
        match self.api.get_json(&path) {
            Err(e @ Error::Api { status, .. }) if e.is_missing_route() => {
                unavailable_note(progress, &what, status);
                Ok((None, None))
            }
            answer => or_unread(answer, &what, progress),
        }
    }

    /// The last repave of one cluster, `GET /v1/spectroclusters/{uid}/repave/status`: its source,
    /// its reasons, and its state. The pack ref on each side of a reason holds the values of the
    /// pack, with the value of each macro in its place.
    pub(crate) fn cluster_repave(&self, uid: &str) -> Result<Value> {
        self.api
            .get_json(&route(CLUSTERS, &[uid, "repave", "status"]))
    }

    /// The notifications of one cluster, `GET /v1/notifications/spectrocluster/{uid}`, every page,
    /// newest first. Palette gives `items: null` when there are none, and at most 50 on a page;
    /// `listmeta.continue` names the next page, and is empty on the last one.
    pub fn cluster_notifications(&self, uid: &str) -> Result<Vec<Value>> {
        self.listed(&route(CLUSTER_NOTIFICATIONS, &[uid]))
    }
}
