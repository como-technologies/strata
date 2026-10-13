//! The type of a pack: `spectro`, `helm`, `oci`, or `manifest`.

/// The `type` of a pack, as a pack reference of Palette names it and as a record keeps it.
///
/// A pack from a registry has the type of its registry: `spectro` from a pack registry, `helm`
/// from a Helm registry, and `oci` from an OCI registry. A `manifest` pack comes from no
/// registry: Palette makes it from the manifests of the layer. A tier of an app profile has a
/// type of its own, such as `container`. It is not a pack type: a record keeps the text, and
/// [`PackType::parse`] gives none for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackType {
    /// `spectro`: a pack of a pack registry.
    Spectro,
    /// `helm`: a chart of a Helm registry.
    Helm,
    /// `oci`: a pack or a chart of an OCI registry.
    Oci,
    /// `manifest`: the manifests of a layer, from no registry.
    Manifest,
}

impl PackType {
    /// The type as the documents name it: `spectro`, `helm`, `oci`, or `manifest`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Spectro => "spectro",
            Self::Helm => "helm",
            Self::Oci => "oci",
            Self::Manifest => "manifest",
        }
    }

    /// The type that the documents name `text`, or none for another text.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "spectro" => Some(Self::Spectro),
            "helm" => Some(Self::Helm),
            "oci" => Some(Self::Oci),
            "manifest" => Some(Self::Manifest),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_type_parses_and_prints_as_the_documents_name_it() {
        for kind in [
            PackType::Spectro,
            PackType::Helm,
            PackType::Oci,
            PackType::Manifest,
        ] {
            assert_eq!(PackType::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(PackType::parse("container"), None);
        assert_eq!(PackType::parse(""), None);
    }
}
