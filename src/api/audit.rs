//! The kinds of object in the audit log of Palette.

/// The kind of an object in the audit log of Palette: the value of `resourceKind`.
///
/// A cluster has no kind here. Strata reads the log of a cluster by its uid alone, with no
/// `resourceKind`. Palette logs the create and the delete of a virtual cluster under
/// `virtualCluster`, and its other writes under `spectrocluster`. The uid finds both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A cluster profile.
    ClusterProfile,
    /// An app profile.
    AppProfile,
    /// An edge host.
    EdgeHost,
    /// A cluster group.
    ClusterGroup,
}

impl AuditKind {
    /// The name that Palette gives the kind in the log and in the query.
    pub fn as_str(self) -> &'static str {
        match self {
            AuditKind::ClusterProfile => "clusterprofile",
            AuditKind::AppProfile => "appprofile",
            AuditKind::EdgeHost => "edgehost",
            AuditKind::ClusterGroup => "clustergroup",
        }
    }
}
