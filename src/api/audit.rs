//! The kinds of object in the audit log of Palette.

/// The kind of an object in the audit log of Palette: the value of `resourceKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    /// A cluster profile.
    ClusterProfile,
    /// An app profile.
    AppProfile,
    /// A cluster.
    SpectroCluster,
    /// The create and the delete of a virtual cluster.
    VirtualCluster,
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
            AuditKind::SpectroCluster => "spectrocluster",
            AuditKind::VirtualCluster => "virtualCluster",
            AuditKind::EdgeHost => "edgehost",
            AuditKind::ClusterGroup => "clustergroup",
        }
    }
}
