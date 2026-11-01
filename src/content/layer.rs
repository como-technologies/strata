//! Palette profile layers, in the order Palette applies them.

use std::fmt;

use crate::closed_set::impl_texts;

/// A profile layer. Palette names them; strata keeps unknown names as [`Layer::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// Kernel, on some edge profiles.
    Kernel,
    /// Operating system image.
    Os,
    /// Kubernetes distribution.
    K8s,
    /// Container network interface.
    Cni,
    /// Container storage interface.
    Csi,
    /// Add-on: anything above the infrastructure layers.
    Addon,
    /// The layer that strata gives each tier of an app profile.
    App,
    /// A layer name strata does not know.
    Other,
}

// The layer as Palette names it: `kernel`, `os`, `k8s`, `cni`, `csi`, or `addon`. `app` is a tier
// of an app profile, and `other` is a name that strata does not know.
impl_texts!(pub(crate) Layer {
    Kernel => "kernel",
    Os => "os",
    K8s => "k8s",
    Cni => "cni",
    Csi => "csi",
    Addon => "addon",
    App => "app",
    Other => "other",
});

impl Layer {
    /// Parse Palette's layer name; unknown names become [`Layer::Other`].
    pub fn parse(name: &str) -> Layer {
        Layer::from_text(name).unwrap_or(Layer::Other)
    }

    /// Sort key: infrastructure layers first, add-ons after, unknown last.
    pub fn rank(self) -> u8 {
        match self {
            Layer::Kernel => 0,
            Layer::Os => 1,
            Layer::K8s => 2,
            Layer::Cni => 3,
            Layer::Csi => 4,
            Layer::Addon => 5,
            Layer::App | Layer::Other => 9,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for Layer {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_follows_palette() {
        let mut layers = vec![
            Layer::App,
            Layer::Addon,
            Layer::Csi,
            Layer::Os,
            Layer::Other,
            Layer::K8s,
            Layer::Cni,
        ];
        layers.sort_by_key(|l| l.rank());
        assert_eq!(
            layers,
            vec![
                Layer::Os,
                Layer::K8s,
                Layer::Cni,
                Layer::Csi,
                Layer::Addon,
                Layer::App,
                Layer::Other
            ]
        );
        assert_eq!(Layer::parse("k8s"), Layer::K8s);
        assert_eq!(Layer::parse("weird"), Layer::Other);
    }

    #[test]
    fn a_layer_parses_and_prints_as_palette_names_it() {
        for layer in [
            Layer::Kernel,
            Layer::Os,
            Layer::K8s,
            Layer::Cni,
            Layer::Csi,
            Layer::Addon,
            Layer::App,
        ] {
            assert_eq!(Layer::parse(layer.as_str()), layer);
            assert_eq!(layer.to_string(), layer.as_str());
        }
        assert_eq!(Layer::Other.as_str(), "other");
        assert_eq!(Layer::parse("other"), Layer::Other);
    }
}
