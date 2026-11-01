//! The edits that the chapters give to a profile. Each one is a change that a person makes through
//! the API.

use crate::tenant::Edit;

pub(super) const RETAIN: &[Edit] = &[Edit::Reclaim("Retain")];
pub(super) const TAG_2404: &[Edit] = &[Edit::Tag {
    pack: "ubuntu-aws",
    tag: "24.04",
}];
pub(super) const TAG_LTS2404: &[Edit] = &[Edit::Tag {
    pack: "ubuntu-aws",
    tag: "LTS__24.04",
}];
pub(super) const OCI_CSI: &[Edit] = &[Edit::OciCsi];
pub(super) const IMAGE_1: &[Edit] = &[Edit::ImageId {
    pack: "ubuntu-aws",
    id: "ami-0a1b2c3d4e5f60001",
}];
pub(super) const IMAGE_2: &[Edit] = &[Edit::ImageId {
    pack: "ubuntu-aws",
    id: "ami-0a1b2c3d4e5f60002",
}];
pub(super) const CERT_MANAGER_TAG_DELETED: &[Edit] = &[Edit::Invalid {
    pack: "cert-manager",
    reason: "Cert Manager 1.21.1 tag is deleted",
}];
pub(super) const SECRET_VALUE: &[Edit] = &[Edit::Masked {
    pack: "cert-manager",
    key: "registryPassword",
}];
pub(super) const MACRO_REGION: &[Edit] = &[Edit::Macro {
    pack: "csi-aws-ebs",
    name: "region",
}];
