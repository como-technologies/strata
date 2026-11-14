//! The consumers of an app profile: its app deployments.

use std::collections::HashMap;

use serde_json::Value;

use super::reach::{search_listed, tenant_listing, tenant_search};
use super::{
    APP_NO_EDGE_HOSTS, APP_NO_SECOND_READING, ClustersRead, Consumer, ConsumerKind,
    ConsumersOptions, ConsumersReport, EdgeHostsRead, ProjectsRead, Readings, Scopes,
    name_consumer_projects, name_profile_project, name_wide_then_home, report,
};
use crate::api::{Palette, PaletteApi};
use crate::error::Result;
use crate::json::{arr, text};
use crate::model::profile_type::ProfileType;
use crate::model::scope::ProfileScope;
use crate::ops::client_for;
use crate::ops::list::{ListedProject, ProjectListings, TENANT_SCOPE};
use crate::term::progress::Progress;

/// The app deployments that use an app profile version.
///
/// The uses are `status.inUseApps` of `GET /v1/appProfiles/{uid}`, each read with
/// `GET /v1/appDeployments/{uid}` in its own project for its state and the version its copy names.
/// Palette refuses the delete of an app profile while a deployment uses it, as it does for a
/// cluster profile. Strata also lists the app deployments of each project that can use the
/// version: see [`app_readings`]. An app profile binds to no edge host, so the report has no edge
/// hosts.
///
/// `id` is a uid, or a name when `version` is given. `scope` is the project uid of the app
/// profile, and `None` is the configured scope. `connect` gives a client for a project uid, with
/// `None` for the configured scope. `generated_at` is the time of the report, in RFC 3339.
///
/// # Errors
///
/// The error of `connect`. [`AppProfileNotFound`](crate::Error::AppProfileNotFound) or
/// [`AppProfileNameAmbiguous`](crate::Error::AppProfileNameAmbiguous) when `id` and `version` name
/// no app profile, or several. The error of the read of the app profile. A deployment that cannot
/// be read stays in the report with its error.
pub(super) fn app_consumers<A, F>(
    scope: Option<&str>,
    id: &str,
    version: Option<&str>,
    options: &ConsumersOptions,
    mut connect: F,
    generated_at: &str,
    progress: &Progress,
) -> Result<ConsumersReport>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut clients: HashMap<Option<String>, Palette<A>> = HashMap::new();
    let home = connect(scope)?;
    let uid = home.app_profile_uid(id, version, progress)?;
    progress.note(&format!("listing the app deployments of app profile {uid}"));
    let doc = home.app_profile_document(&uid)?;
    let mut list = listed_apps(&doc, &home, &mut clients, &mut connect);
    // the project listing of the tenant gives the projects of an app profile of the tenant, and
    // names a deployment in another project
    let tenant = connect(Some(TENANT_SCOPE));
    let wide = tenant.as_ref().unwrap_or(&home);
    let scopes = Scopes {
        wide,
        home: &home,
        no_tenant: tenant.as_ref().err(),
    };
    let mut listings = ProjectListings::default();
    let mut readings = app_readings(
        &mut list,
        &doc,
        &uid,
        options,
        &scopes,
        &mut clients,
        &mut connect,
        &mut listings,
        progress,
    );
    name_consumer_projects(&mut list, wide, &home, &mut listings);
    if let Some(projects) = readings.projects.as_mut() {
        name_wide_then_home(
            &mut projects.unread,
            |p| &p.uid,
            |p| &mut p.name,
            [wide, &home],
            &mut listings,
        );
    }
    let mut r = report(&doc, Some(ProfileType::App), list, readings, generated_at);
    name_profile_project(&mut r.profile, wide, &home, &mut listings);
    Ok(r)
}

/// The app deployments that `status.inUseApps` of the app profile document `doc` names. Strata
/// reads each one in its own project for its state and the version that its copy names.
fn listed_apps<A, F>(
    doc: &Value,
    home: &Palette<A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
) -> Vec<Consumer>
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let mut list: Vec<Consumer> = Vec::new();
    for app in arr(doc, "/status/inUseApps") {
        let app_uid = text(app, "/uid");
        if app_uid.is_empty() || list.iter().any(|c| c.uid == app_uid) {
            continue;
        }
        let mut c = Consumer::unread(
            ConsumerKind::App,
            app_uid,
            text(app, "/name"),
            text(app, "/projectUid"),
        );
        c.read(
            clients,
            home,
            connect,
            |p, deployment| p.app_deployment_document(deployment),
            |d| text(d, "/spec/profile/metadata/version"),
        );
        list.push(c);
    }
    list
}

/// What strata reads beside the consumer list of an app profile: the app deployments of each
/// project that can use the version.
///
/// For an app profile of a project, Palette names the app deployments in the consumer list only
/// when a role of the key gives `cluster.get` in that project. Else the list is empty, with no
/// error. So strata lists the app deployments of each project that can use the version: the
/// project of an app profile of a project, or each project of the tenant listing for an app
/// profile of the tenant or of the system. Each one that runs the version `uid` and is not in
/// `list` joins it, marked `unlisted`. A project that refuses the listing is in
/// [`ProjectsRead::unread`]. `--list-only` asks for none of it.
///
/// For an app profile of the tenant or of the system, strata also sends one page of the cluster
/// search at tenant scope, as [`tenant_search`] reads it for a profile of the tenant. A denial
/// shows a key that cannot list each project. Each other error of the search stops the listings,
/// and it is the error of [`ClustersRead`].
#[allow(clippy::too_many_arguments)]
fn app_readings<A, F>(
    list: &mut Vec<Consumer>,
    doc: &Value,
    uid: &str,
    options: &ConsumersOptions,
    scopes: &Scopes<'_, A>,
    clients: &mut HashMap<Option<String>, Palette<A>>,
    connect: &mut F,
    listings: &mut ProjectListings,
    progress: &Progress,
) -> Readings
where
    A: PaletteApi,
    F: FnMut(Option<&str>) -> Result<Palette<A>>,
{
    let readings = |reason: &str, projects: Option<ProjectsRead>| Readings {
        clusters: ClustersRead {
            skipped: true,
            reason: reason.to_string(),
            ..ClustersRead::default()
        },
        edge_hosts: EdgeHostsRead {
            skipped: true,
            reason: APP_NO_EDGE_HOSTS.to_string(),
            ..EdgeHostsRead::default()
        },
        projects,
    };
    if options.list_only {
        return readings("", None);
    }
    let mut projects = ProjectsRead::default();
    let project = text(doc, "/metadata/annotations/projectUid");
    let (listed, stopped) = match ProfileScope::of_document(doc) {
        Some(ProfileScope::Project) if !project.is_empty() => {
            let own = ListedProject {
                name: String::new(),
                uid: project,
            };
            (vec![own], None)
        }
        Some(scope) if scope.is_shared() => {
            match tenant_listing(scopes, listings, &mut projects).map(<[ListedProject]>::to_vec) {
                // strata lists the app deployments in each project, so only the search at tenant
                // scope shows a key that cannot list each project
                Some(listed) => {
                    let searched = scopes.wide.cluster_search_page();
                    (listed, tenant_search(searched, &mut projects).err())
                }
                None => (Vec::new(), None),
            }
        }
        _ => return readings(APP_NO_SECOND_READING, None),
    };
    progress.note("listing the app deployments of each project that can use the version");
    search_listed(&listed, stopped.as_ref(), &mut projects, |project| {
        let deployments = client_for(clients, scopes.home, &project.uid, connect)
            .and_then(Palette::app_deployments)?;
        join_running(list, uid, &project.uid, &deployments);
        Ok(())
    });
    let mut read = readings("", Some(projects));
    read.clusters.error = stopped.map(|e| e.to_json());
    read
}

/// Add to `list` each app deployment of `deployments`, the listing of the project `project`, that
/// runs the app profile version `uid` and that `list` does not name. It is marked `unlisted`, with
/// the name, the state, and the version that the listing gives.
fn join_running(list: &mut Vec<Consumer>, uid: &str, project: &str, deployments: &[Value]) {
    for d in deployments {
        let app_uid = text(d, "/metadata/uid");
        if text(d, "/spec/profile/metadata/uid") != uid
            || app_uid.is_empty()
            || list.iter().any(|c| c.uid == app_uid)
        {
            continue;
        }
        let mut c = Consumer {
            unlisted: true,
            ..Consumer::unread(
                ConsumerKind::App,
                app_uid,
                String::new(),
                project.to_string(),
            )
        };
        c.fill_from_document(d);
        c.version = text(d, "/spec/profile/metadata/version");
        list.push(c);
    }
}
