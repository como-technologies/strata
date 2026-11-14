//! The consumers of an app profile: its app deployments.

use std::collections::HashMap;

use super::{
    APP_NO_EDGE_HOSTS, APP_NO_SECOND_READING, ClustersRead, Consumer, ConsumerKind,
    ConsumersReport, EdgeHostsRead, Readings, name_consumer_projects, report,
};
use crate::api::{Palette, PaletteApi};
use crate::error::Result;
use crate::json::{arr, text};
use crate::model::profile_type::ProfileType;
use crate::ops::list::{ProjectListings, TENANT_SCOPE};
use crate::term::progress::Progress;

/// The app deployments that use an app profile version.
///
/// The uses are `status.inUseApps` of `GET /v1/appProfiles/{uid}`, each read with
/// `GET /v1/appDeployments/{uid}` in its own project for its state and the version its copy names.
/// Palette refuses the delete of an app profile while a deployment uses it, as it does for a
/// cluster profile. The dashboard route of the app deployments filters no profile, and an app
/// profile binds to no edge host. So the report has no second reading and no edge hosts.
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

    let mut list: Vec<Consumer> = Vec::new();
    for app in arr(&doc, "/status/inUseApps") {
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
            &mut clients,
            &home,
            &mut connect,
            |p, deployment| p.app_deployment_document(deployment),
            |d| text(d, "/spec/profile/metadata/version"),
        );
        list.push(c);
    }
    // the project listing of the tenant names a deployment in another project
    let tenant = connect(Some(TENANT_SCOPE)).ok();
    let wide = tenant.as_ref().unwrap_or(&home);
    name_consumer_projects(&mut list, wide, &home, &mut ProjectListings::default());
    let readings = Readings {
        clusters: ClustersRead {
            skipped: true,
            reason: APP_NO_SECOND_READING.to_string(),
            ..ClustersRead::default()
        },
        edge_hosts: EdgeHostsRead {
            skipped: true,
            reason: APP_NO_EDGE_HOSTS.to_string(),
            ..EdgeHostsRead::default()
        },
        projects: None,
    };
    Ok(report(
        &doc,
        Some(ProfileType::App),
        list,
        readings,
        generated_at,
    ))
}
