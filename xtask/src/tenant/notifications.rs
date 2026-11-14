//! The notifications of a cluster that an edit of a profile opens, and the updates that wait for
//! it.

use serde_json::{Map, Value, json};

use super::Tenant;
use super::profiles::template_of;
use super::routes::{cluster_route, notifications_route, profile_route};
use super::times::minutes_after;

/// The time of the first notification of a cluster. Each later one takes the next minute.
const FIRST_NOTIFICATION: &str = "2026-09-18T14:02:12.107Z";

impl Tenant {
    /// Tell each cluster that runs profile `uid` whether an update waits for it, as Palette does
    /// after an edit in place. A live run gives the rules. A change of a pack opens one
    /// notification. A change of the version number alone opens none. An edit that gives the
    /// profile the content of the cluster's copy again closes the open ones. `cluster` closes
    /// them too, when the cluster takes the update.
    pub(super) fn notify_clusters(&self, project: &str, uid: &str, doc: &Value) {
        let clusters: Vec<String> = doc["status"]["inUseClusterUids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        let packs = doc["spec"]["published"]["packs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for cluster in clusters {
            let copy: Vec<Value> =
                self.read(project, &cluster_route(&cluster))["spec"]["clusterProfileTemplates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|t| t["uid"] == json!(uid))
                    .and_then(|t| t["packs"].as_array().cloned())
                    .unwrap_or_default();
            self.write_profile_updates(project, &cluster);
            if copy == packs {
                self.close_notifications(project, &cluster);
                continue;
            }
            let path = notifications_route(&cluster);
            let mut items = self.notifications(project, &cluster);
            let events = update_events(uid, &packs, &copy);
            items.insert(
                0,
                json!({
                    "metadata": {
                        "uid": format!("64fc{:020x}", items.len() + 1),
                        "creationTimestamp": minutes_after(FIRST_NOTIFICATION, items.len()),
                        "name": format!("spectrocluster '{cluster}' notification"),
                    },
                    "type": "NotificationPackUpdate",
                    "action": {
                        "actionType": "NotifyActionClusterProfileUpdate",
                        "actionMessage": "Apply cluster profile pack updates",
                        "isDone": false,
                        "events": events,
                    },
                    "relatedObject": {"kind": "spectrocluster", "uid": cluster},
                }),
            );
            self.write_notifications(project, &path, items);
        }
    }

    /// Close the open notifications of the cluster `cluster`: no update waits for it.
    pub(super) fn close_notifications(&self, project: &str, cluster: &str) {
        let mut items = self.notifications(project, cluster);
        for n in &mut items {
            n["action"]["isDone"] = json!(true);
        }
        self.write_notifications(project, &notifications_route(cluster), items);
    }

    /// `GET /v1/spectroclusters/{uid}/profileUpdates`: each profile of the cluster as the profile
    /// is now, in the shape of the cluster's templates. A live run gives the rule: Palette lists
    /// each profile of the cluster also when no update waits.
    pub(super) fn write_profile_updates(&self, project: &str, cluster: &str) {
        let profiles: Vec<Value> =
            self.read(project, &cluster_route(cluster))["spec"]["clusterProfileTemplates"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| {
                    let uid = t["uid"].as_str().unwrap_or_default();
                    template_of(&self.read(project, &profile_route(uid)))
                })
                .collect();
        self.write(
            project,
            &format!("{}/profileUpdates", cluster_route(cluster)),
            &json!({ "profiles": profiles }),
        );
    }

    /// The notifications of a cluster, newest first; none for a cluster that has no file yet.
    fn notifications(&self, project: &str, cluster: &str) -> Vec<Value> {
        let path = notifications_route(cluster);
        if !self.doc(project, &path).is_file() {
            return Vec::new();
        }
        self.read(project, &path)["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    /// Palette gives `items: null`, not an empty list, for a cluster with no notification.
    fn write_notifications(&self, project: &str, path: &str, items: Vec<Value>) {
        let items = if items.is_empty() {
            Value::Null
        } else {
            Value::Array(items)
        };
        self.write(
            project,
            path,
            &json!({"items": items, "listmeta": {"continue": "", "limit": 50}}),
        );
    }
}

/// One event for each pack of the profile `profile` that the edit changed, as Palette names it.
/// `packs` are the packs of the profile now, and `copy` are the packs that the cluster holds.
fn update_events(profile: &str, packs: &[Value], copy: &[Value]) -> Map<String, Value> {
    packs
        .iter()
        .filter(|p| !copy.contains(p))
        .enumerate()
        .map(|(i, p)| {
            let name = p["name"].as_str().unwrap_or_default();
            let version = p["version"]
                .as_str()
                .or(p["tag"].as_str())
                .unwrap_or_default();
            let values_only = copy.iter().any(|c| {
                c["name"] == p["name"] && c["tag"] == p["tag"] && c["values"] != p["values"]
            });
            let (kind, message) = if values_only {
                (
                    "PackValuesUpdate",
                    format!("{name} {version} values are updated"),
                )
            } else {
                ("PackUpdate", format!("Pack '{name}' is updated"))
            };
            (
                format!("e{i}"),
                json!({"profileUid": profile, "type": kind, "message": message}),
            )
        })
        .collect()
}
