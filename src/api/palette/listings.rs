//! The listings and the searches of Palette: profiles, clusters, projects, edge hosts, cluster
//! groups, tag filters, pack versions, registries, and the audit log.

use std::collections::BTreeSet;

use serde_json::Value;

use super::{
    AUDIT_PAGE, AUDITS, CLUSTER_GROUP_LISTING, CLUSTER_LISTING, CLUSTER_SEARCH,
    CLUSTER_SEARCH_SCHEMA, EDGE_HOST_SEARCH, PACK_LISTING_PAGE, PACKS, PROFILE_LISTING,
    PROJECT_LISTING, Palette, REGISTRIES, REGISTRY_METADATA, RegistryKind, SEARCH_ENVIRONMENT,
    SEARCH_STRING, SearchedClusters, TAG_FILTER_LISTING, TAG_FILTERS, named_entries, query_value,
    route, search_all, sorted_names, uids_named, with_limit, with_nested,
};
use crate::api::{AuditKind, PaletteApi};
use crate::error::{Error, Result, served};
use crate::json::{arr, items, opt_text, str_at, text};
use crate::reference::one_named;

impl<A: PaletteApi> Palette<A> {
    /// The profile listing, `GET /v1/dashboard/clusterprofiles/metadata`: `items[]` with each
    /// version's name, uid, version, and cloud type. In a project it holds that project's
    /// profiles; at tenant scope, the tenant's own.
    pub fn profile_listing(&self) -> Result<Value> {
        self.api.get_json(PROFILE_LISTING)
    }

    /// The cluster listing, `GET /v1/dashboard/spectroclusters/meta`: an array with each cluster's
    /// name, uid, cloud type, and state. At tenant scope it holds the clusters of every project.
    pub fn cluster_listing(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_LISTING)
    }

    /// The uid of the project named `name`, from [`Palette::project_listing`]. Call it at tenant
    /// scope: in a project, Palette lists only that project.
    ///
    /// The first project with the name is the one project with the name. Palette refuses a
    /// second project with the name of a project of the tenant, with the code
    /// `ResourceNameAlreadyExists`. It compares the names with no regard to case.
    ///
    /// # Errors
    ///
    /// [`Error::ProjectNotFound`] when no project has the name. Each error of the request.
    pub fn project_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.project_listing()?;
        let projects = named_entries(&listing);
        match projects.iter().find(|(n, _)| n == name) {
            Some((_, uid)) => Ok(uid.clone()),
            None => Err(Error::ProjectNotFound {
                name: name.to_string(),
                available: sorted_names(&projects),
            }),
        }
    }

    /// The uid of the one cluster named `name` in the scope.
    ///
    /// It reads [`Palette::cluster_listing`] and [`Palette::virtual_clusters`]. The listing leaves
    /// out each virtual cluster, so the search gives them, when it takes `environment`.
    ///
    /// # Errors
    ///
    /// [`Error::ClusterNotFound`] when no cluster has the name, and
    /// [`Error::ClusterNameAmbiguous`] when two or more have it.
    pub fn cluster_uid_named(&self, name: &str) -> Result<String> {
        let listing = self.cluster_listing()?;
        let mut clusters = named_entries(&listing);
        let properties = self.cluster_search_properties()?;
        if let Some(found) = self.virtual_clusters(&properties, &Value::Null, || {})? {
            clusters.extend(named_entries(&Value::Array(found)));
        }
        one_named(
            uids_named(&clusters, name),
            String::as_str,
            || Error::ClusterNotFound {
                cluster: name.to_string(),
                available: sorted_names(&clusters),
            },
            |uids| Error::ClusterNameAmbiguous {
                name: name.to_string(),
                uids,
            },
        )
    }

    /// The project listing, `GET /v1/dashboard/projects/metadata`: `items[]` with each project's
    /// name and uid. At tenant scope it holds every project the key can see; in a project, only
    /// that project.
    pub fn project_listing(&self) -> Result<Value> {
        self.api.get_json(PROJECT_LISTING)
    }

    /// The entries of the audit log of Palette for one resource, `GET /v1/audits`, that it logged
    /// at `since` or after, every page. With no `kind`, Palette gives the entries of each kind
    /// for the uid. Palette lists them newest first, and at most
    /// 100 on a page; `listmeta.continue` names the next page, and is empty on the last one. An
    /// empty `since` reads the full log of the resource.
    ///
    /// # Errors
    ///
    /// The error of the first request that fails.
    pub fn audits(&self, kind: Option<AuditKind>, uid: &str, since: &str) -> Result<Vec<Value>> {
        let mut query = Vec::new();
        if let Some(kind) = kind {
            query.push(format!("resourceKind={}", kind.as_str()));
        }
        query.push(format!("resourceUid={}", query_value(uid)));
        if !since.is_empty() {
            query.push(format!("startTime={}", query_value(since)));
        }
        query.push(format!("limit={AUDIT_PAGE}"));
        self.listed(&format!("{AUDITS}?{}", query.join("&")))
    }

    /// Every edge host the scope can list, `POST /v1/dashboard/edgehosts/search`, every page.
    /// Each item names the profiles bound to the host in `spec.clusterProfileTemplates`, as a
    /// summary: uid, name, and version.
    pub fn edge_hosts(&self) -> Result<Vec<Value>> {
        self.searched(&with_limit(EDGE_HOST_SEARCH), &search_all())
    }

    /// The clusters that a search filter selects, `POST /v1/dashboard/spectroclusters/search`,
    /// every page. `filter` is the `filter` of the body: groups of conditions on the properties
    /// that `cluster_search_schema` names. At tenant scope it holds the clusters of each project
    /// the key can read, each with its labels and its project. It holds no virtual cluster,
    /// unless a condition asks for the environment `nested`: see
    /// [`Palette::virtual_cluster_search`].
    pub fn cluster_search(&self, filter: &Value) -> Result<Vec<Value>> {
        self.searched(
            &with_limit(CLUSTER_SEARCH),
            &serde_json::json!({"filter": filter, "sort": [{"field": "clusterName", "order": "asc"}]}),
        )
    }

    /// The virtual clusters that a search filter selects: [`Palette::cluster_search`] with the
    /// condition `environment eq nested` added to `filter` as one more group. The search leaves
    /// out each virtual cluster unless a condition asks for that environment, the cloud type of
    /// a virtual cluster. `filter` joins its groups with `and`, and `Null` has no condition, so
    /// the answer then holds each virtual cluster in the scope. Each item names the host cluster
    /// and the cluster group of the virtual cluster in `status.virtual`.
    pub fn virtual_cluster_search(&self, filter: &Value) -> Result<Vec<Value>> {
        self.cluster_search(&with_nested(filter))
    }

    /// The virtual clusters that `filter` selects, when the search takes `environment`.
    ///
    /// It is [`Palette::virtual_cluster_search`] behind the one check of that property.
    /// `properties` is the answer of [`Palette::cluster_search_properties`]. `None` when the
    /// search does not take `environment`: no condition can then ask for a virtual cluster, and
    /// the search leaves each one out. `on_search` runs before the search, when there is one, so
    /// that a caller can note it.
    ///
    /// # Errors
    ///
    /// Each error of the search.
    pub fn virtual_clusters(
        &self,
        properties: &BTreeSet<String>,
        filter: &Value,
        on_search: impl FnOnce(),
    ) -> Result<Option<Vec<Value>>> {
        if !properties.contains(SEARCH_ENVIRONMENT) {
            return Ok(None);
        }
        on_search();
        self.virtual_cluster_search(filter).map(Some)
    }

    /// The clusters that `filter` selects, with the virtual clusters among them.
    ///
    /// The clusters come from [`Palette::cluster_search`], and the virtual clusters from
    /// [`Palette::virtual_clusters`], when the search takes `environment`. `properties` is the
    /// answer of [`Palette::cluster_search_properties`], and `on_virtual` runs before the search
    /// of the virtual clusters, when there is one.
    ///
    /// # Errors
    ///
    /// Each error of the search of the clusters, or of the search of the virtual clusters.
    pub fn cluster_search_with_virtual(
        &self,
        properties: &BTreeSet<String>,
        filter: &Value,
        on_virtual: impl FnOnce(),
    ) -> Result<SearchedClusters> {
        let mut items = self.cluster_search(filter)?;
        let nested = self.virtual_clusters(properties, filter, on_virtual)?;
        let includes_virtual = nested.is_some();
        items.extend(nested.into_iter().flatten());
        Ok(SearchedClusters {
            items,
            includes_virtual,
        })
    }

    /// The properties that the cluster search takes, `GET
    /// /v1/dashboard/spectroclusters/search/schema`: `schema.properties[]` with `name` and `type`.
    pub fn cluster_search_schema(&self) -> Result<Value> {
        self.api.get_json(CLUSTER_SEARCH_SCHEMA)
    }

    /// The names of the string properties that the cluster search takes, from
    /// [`Palette::cluster_search_schema`]. Empty when Palette has no search: it answers 404 or
    /// 405 for the schema.
    ///
    /// # Errors
    ///
    /// Each other error of the request.
    pub fn cluster_search_properties(&self) -> Result<BTreeSet<String>> {
        let Some(schema) = served(self.cluster_search_schema())? else {
            return Ok(BTreeSet::new());
        };
        Ok(arr(&schema, "/schema/properties")
            .iter()
            .filter(|p| str_at(p, "/type") == SEARCH_STRING)
            .filter_map(|p| opt_text(p, "/name"))
            .collect())
    }

    /// The saved tag filters, `GET /v1/filters/metadata?filterType=tag`: the uid and the name of
    /// each. A tag filter belongs to the tenant.
    pub fn tag_filters(&self) -> Result<Value> {
        self.api.get_json(TAG_FILTER_LISTING)
    }

    /// One saved tag filter, `GET /v1/filters/tag/{uid}`: `spec.filterGroup` holds its
    /// conjunction and its conditions.
    pub fn tag_filter(&self, uid: &str) -> Result<Value> {
        self.api.get_json(&route(TAG_FILTERS, &[uid]))
    }

    /// The name and the uid of each cluster group in the scope, from `GET
    /// /v1/clustergroups/hostCluster/metadata`.
    pub fn cluster_groups(&self) -> Result<Vec<(String, String)>> {
        let listing = self.api.get_json(CLUSTER_GROUP_LISTING)?;
        Ok(items(&listing)
            .map(|g| (text(g, "/name"), text(g, "/uid")))
            .collect())
    }

    /// One page of the pack listing of `name`,
    /// `GET /v1/packs?filters=spec.name={name}&limit=100&offset={offset}`. It gives each version
    /// of the pack in each registry, [`PACK_LISTING_PAGE`] on a page. `listmeta.count` counts the
    /// whole listing.
    pub(crate) fn pack_listing_page(&self, name: &str, offset: usize) -> Result<Value> {
        self.api.get_json(&format!(
            "{PACKS}?filters=spec.name={}&limit={PACK_LISTING_PAGE}&offset={offset}",
            query_value(name)
        ))
    }

    /// The tags of the pack `name` in the registry `registry_uid`,
    /// `GET /v1/packs/{name}/registries/{uid}`. Each tag names its version. The answer also
    /// holds the values of the pack, and its layer. Palette answers 404 when the registry serves
    /// no version of the pack.
    ///
    /// With `states`, a list such as `deprecated,disabled,deleted`, the request adds the query
    /// `states=<states>`, and Palette gives the versions in those states only. The crate gives
    /// the list, as the route reads it, so it goes in with no escape.
    pub(crate) fn pack_tags(
        &self,
        name: &str,
        registry_uid: &str,
        states: Option<&str>,
    ) -> Result<Value> {
        let path = route(PACKS, &[name, "registries", registry_uid]);
        match states {
            Some(states) => self.api.get_json(&format!("{path}?states={states}")),
            None => self.api.get_json(&path),
        }
    }

    /// The registries that the key can see, `GET /v1/registries/metadata`: the `uid`, the
    /// `name`, the `kind`, and `isPrivate` of each, as `items`.
    pub(crate) fn registry_metadata(&self) -> Result<Value> {
        self.api.get_json(REGISTRY_METADATA)
    }

    /// The summary listing of the registries of one `kind`, `GET /v1/registries/{kind}/summary`.
    /// Each registry is an entry of `items`, with its last sync in `status.sync`. An OCI registry
    /// also has its provider type in `spec.providerType`.
    pub(crate) fn registry_summary(&self, kind: RegistryKind) -> Result<Value> {
        self.api
            .get_json(&route(REGISTRIES, &[kind.route(), "summary"]))
    }
}
