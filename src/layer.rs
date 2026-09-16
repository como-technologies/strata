//! Palette profile layers, in the order Palette applies them.

use std::fmt;

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
    /// A layer name strata does not know.
    Other,
}

impl Layer {
    /// Parse Palette's layer name; unknown names become [`Layer::Other`].
    pub fn parse(name: &str) -> Layer {
        match name {
            "kernel" => Layer::Kernel,
            "os" => Layer::Os,
            "k8s" => Layer::K8s,
            "cni" => Layer::Cni,
            "csi" => Layer::Csi,
            "addon" => Layer::Addon,
            _ => Layer::Other,
        }
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
            Layer::Other => 9,
        }
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Layer::Kernel => "kernel",
            Layer::Os => "os",
            Layer::K8s => "k8s",
            Layer::Cni => "cni",
            Layer::Csi => "csi",
            Layer::Addon => "addon",
            Layer::Other => "other",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_follows_palette() {
        let mut layers = vec![
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
                Layer::Other
            ]
        );
        assert_eq!(Layer::parse("k8s"), Layer::K8s);
        assert_eq!(Layer::parse("weird"), Layer::Other);
    }
}
