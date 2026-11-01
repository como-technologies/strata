//! The uids of the profiles, the clusters, and the other objects that the chapters build. Each uid
//! names one object of the stand-in.

pub(super) const DEV: &str = "64f20000000000000000a0d1";
pub(super) const PROD: &str = "64f20000000000000000a0f1";

// the profile uids of the examples, in dev
pub(super) const A100: &str = "64e0000000000000000000a1";
pub(super) const A110: &str = "64e0000000000000000000b2";
pub(super) const A111: &str = "64e0000000000000000000c3";
// other profile and cluster uids
pub(super) const A111_PROD: &str = "64e1000000000000000000c3";
pub(super) const A110_PROD: &str = "64e1000000000000000000b2";
pub(super) const A100_PROD: &str = "64e1000000000000000000a1";
pub(super) const A112: &str = "64e0000000000000000000d4";
pub(super) const A113: &str = "64e0000000000000000000d5";
pub(super) const A120: &str = "64e0000000000000000000e5";
// aws-stack@1.1.4: the content of 1.1.1, whose CSI values read a project macro, in dev and prod
pub(super) const A114: &str = "64e0000000000000000000f7";
pub(super) const A114_PROD: &str = "64e1000000000000000000f7";
// aws-stack@1.1.5: the content of 1.1.1, with no digest for the CSI pack
pub(super) const A115: &str = "64e0000000000000000000f8";
// aws-stack@1.1.6: the content of 1.1.1, with one more kubeadm command in the Kubernetes pack
pub(super) const A116: &str = "64e0000000000000000000f9";
// aws-stack@1.1.7: the content of 1.1.1, with Kubernetes 1.36.3 in place of 1.36.2
pub(super) const A117: &str = "64e0000000000000000000fa";
// aws-stack@1.1.8: the content of 1.1.1, with an OS pack that names its machine image
pub(super) const A118: &str = "64e0000000000000000000fb";
// aws-stack@1.1.9: the content of 1.1.1, whose cert-manager tag the registry deletes
pub(super) const A119: &str = "64e0000000000000000000fc";
// aws-stack@1.1.10: the content of 1.1.1, with a secret in the cert-manager values, in dev and
// prod
pub(super) const A1110: &str = "64e0000000000000000000fd";
pub(super) const A1110_PROD: &str = "64e1000000000000000000fd";
// aws-stack@1.1.11: the content of 1.1.1, with the CSI pack at 1.63.1, the one pin that is behind
pub(super) const A1111: &str = "64e0000000000000000000ff";
// aws-stack@1.3.0: the content of 1.1.1, made with no publish
pub(super) const A130: &str = "64e0000000000000000000fe";
// aws-stack@1.1.1 of the tenant: the content of 1.1.1, published at tenant scope
pub(super) const A111_TENANT: &str = "64e4000000000000000000c3";
pub(super) const ADDONS_DEV: &str = "64e2000000000000000000a1";
pub(super) const ADDONS_PROD: &str = "64e3000000000000000000a1";
pub(super) const ADDONS_PROD2: &str = "64e3000000000000000000a2";
// charts-probe@1.0.0: one chart of a private Helm registry, and the pack that Palette made for it
pub(super) const CHARTS_PROBE: &str = "64ed000000000000000000c1";
pub(super) const CHART_PACK: &str = "64ed000000000000000000b1";
pub(super) const A112_PROD: &str = "64e1000000000000000000d4";
pub(super) const PAYMENTS: &str = "64f1000000000000000000e1";
// the cloud account platform-aws of the preflight chapter and the loop chapter
pub(super) const PLATFORM_AWS: &str = "64f3000000000000000000a1";
// gcp-stack@1.0.0: a profile of the cloud type gcp, which the preflight chapter puts in the body of
// an
// AWS create
pub(super) const GCP_STACK: &str = "64f3000000000000000000b1";
pub(super) const SEARCH: &str = "64f1000000000000000000e2";
pub(super) const LEDGER: &str = "64f1000000000000000000e3";
pub(super) const LEGACY: &str = "64f1000000000000000000e7";
pub(super) const PAYMENTS_PROD: &str = "64f1000000000000000000f1";
// a cluster on vSphere, for the shape tier of another cloud
pub(super) const LAB: &str = "64f1000000000000000000e6";
// the virtual clusters of a cluster group
pub(super) const TENANT_A: &str = "64f1000000000000000000e4";
pub(super) const TENANT_B: &str = "64f1000000000000000000e5";
pub(super) const TEAM_A_APPS: &str = "64e2000000000000000000b1";
pub(super) const AWS_STANDARD: &str = "64f8000000000000000000a1";
pub(super) const DEV_GROUP: &str = "64f5000000000000000000a1";
// the saved tag filter of the loop
pub(super) const PRODUCTION: &str = "64f7000000000000000000a2";
// edge hosts, and an app profile
pub(super) const STORE_1: &str = "64f4000000000000000000e1";
pub(super) const STORE_2: &str = "64f4000000000000000000e2";
pub(super) const SHOP: &str = "64f6000000000000000000a1";
pub(super) const SHOP_DEV: &str = "64f7000000000000000000a1";
// the manifest pack of platform-addons
pub(super) const ADDONS_PACK: &str = "64e2000000000000000000d1";
