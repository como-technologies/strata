//! The impact of a change to a pack: a table of rules over the layer of the pack and what changed
//! in it. The impact is a hint, not an answer from Palette.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::closed_set::impl_closed_set;
use crate::closed_set::legend::Described;
use crate::content::layer::Layer;

/// Ordered by severity, so `max` picks the worst.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImpactLevel {
    /// No change, a renamed tag of the same build, or an OS, Kubernetes, or kernel pack that moved
    /// to another pack reference at the same version.
    #[default]
    None,
    /// Only the upstream digest changed, or an OS, Kubernetes, or kernel pack moved to another pack
    /// reference at the same version and its digest changed. Applied on the next profile sync.
    Sync,
    /// Add-on upgraded or reconfigured in place.
    InPlace,
    /// CNI or CSI changed; every node affected, may repave.
    ClusterWide,
    /// OS or Kubernetes configuration changed; Palette may repave.
    MayRepave,
    /// Nodes are replaced.
    Repave,
}

impl_closed_set!(ImpactLevel {
    None => "none",
    Sync => "sync",
    InPlace => "in-place",
    ClusterWide => "cluster-wide",
    MayRepave => "may-repave",
    Repave => "repave",
});

impl Described for ImpactLevel {
    fn describe(self) -> &'static str {
        match self {
            Self::None => {
                "No change, only the tag name of a pack changed for the same build, or an OS, Kubernetes, or kernel pack moved to another pack reference at the same version."
            }
            Self::Sync => {
                "Only the upstream digest changed, or an OS, Kubernetes, or kernel pack moved to another pack reference at the same version and its digest changed. Applied on the next profile sync."
            }
            Self::InPlace => "Add-on pack upgraded or reconfigured in place.",
            Self::ClusterWide => "CNI or CSI layer changed; affects every node and may repave.",
            Self::MayRepave => {
                "Node or control plane configuration changed on the OS or Kubernetes layer; Palette may repave."
            }
            Self::Repave => {
                "Nodes are replaced on a cluster of the kubeadm engine: OS image or Kubernetes version change. compare --repave asks Palette."
            }
        }
    }
}

/// What a change does to running clusters; a hint from the layer and the flags.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Impact {
    /// Severity level.
    pub level: ImpactLevel,
    /// Human sentence, the same text the terminal report prints
    pub text: String,
}

/// Which of the three hashes moved for one pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// The identity hash moved.
    pub identity: bool,
    /// The version moved: a part of the identity.
    pub version: bool,
    /// The upstream hash moved.
    pub upstream: bool,
    /// The authored hash moved.
    pub authored: bool,
}

/// Which layers a rule applies to.
#[derive(Debug, Clone, Copy)]
enum Layers {
    Only(Layer),
    NodeImage,
    InfraAddon,
    Any,
}

impl Layers {
    fn matches(self, layer: Layer) -> bool {
        match self {
            Layers::Only(l) => layer == l,
            Layers::NodeImage => matches!(layer, Layer::Kernel | Layer::Os | Layer::K8s),
            Layers::InfraAddon => matches!(layer, Layer::Cni | Layer::Csi),
            Layers::Any => true,
        }
    }
}

/// Which flags a rule needs.
#[derive(Debug, Clone, Copy)]
enum Needs {
    Identity,
    Authored,
    IdentityOrAuthored,
    UpstreamOnly,
}

impl Needs {
    fn matches(self, f: Flags) -> bool {
        match self {
            Needs::Identity => f.identity,
            Needs::Authored => f.authored,
            Needs::IdentityOrAuthored => f.identity || f.authored,
            Needs::UpstreamOnly => f.upstream && !f.identity && !f.authored,
        }
    }
}

struct Rule {
    layers: Layers,
    needs: Needs,
    level: ImpactLevel,
    text: &'static str,
}

/// The impact rules, first match wins. The order encodes severity: a Kubernetes version bump is a
/// repave even when its values also changed.
const RULES: &[Rule] = &[
    Rule {
        layers: Layers::Only(Layer::Os),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (OS image change)",
    },
    Rule {
        layers: Layers::Only(Layer::K8s),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (Kubernetes version change)",
    },
    Rule {
        layers: Layers::Only(Layer::Kernel),
        needs: Needs::Identity,
        level: ImpactLevel::Repave,
        text: "node repave (kernel change)",
    },
    // Palette checks every value of the OS pack on the engine of this table: any change repaves
    Rule {
        layers: Layers::Only(Layer::Os),
        needs: Needs::Authored,
        level: ImpactLevel::Repave,
        text: "node repave (OS values change)",
    },
    Rule {
        layers: Layers::NodeImage,
        needs: Needs::Authored,
        level: ImpactLevel::MayRepave,
        text: "node/control plane config change, may repave",
    },
    Rule {
        layers: Layers::InfraAddon,
        needs: Needs::IdentityOrAuthored,
        level: ImpactLevel::ClusterWide,
        text: "cluster-wide infra addon update",
    },
    Rule {
        layers: Layers::Any,
        needs: Needs::IdentityOrAuthored,
        level: ImpactLevel::InPlace,
        text: "in-place addon upgrade",
    },
    Rule {
        layers: Layers::Any,
        needs: Needs::UpstreamOnly,
        level: ImpactLevel::Sync,
        text: "pack content changed under the same tag, applied on next profile sync",
    },
];

/// The impact of a change on a layer, from the rules table.
///
/// On the OS, Kubernetes, and kernel layers, an identity change that keeps the version takes the
/// level of the other flags. Palette replaces the nodes for a new version of such a pack. A live
/// run moved the OS pack of a running cluster to another registry at one version, and Palette
/// replaced no node.
pub fn impact(layer: Layer, flags: Flags) -> Impact {
    if !(flags.identity && !flags.version && Layers::NodeImage.matches(layer)) {
        return rule_impact(layer, flags);
    }
    let other = rule_impact(
        layer,
        Flags {
            identity: false,
            ..flags
        },
    );
    let text = match other.level {
        ImpactLevel::None => "none (the same version from another pack reference)",
        ImpactLevel::Sync => {
            "the same version from another pack reference, applied on next profile sync"
        }
        _ => return other,
    };
    Impact {
        level: other.level,
        text: text.to_string(),
    }
}

/// The impact of the first rule that matches the layer and the flags.
fn rule_impact(layer: Layer, flags: Flags) -> Impact {
    RULES
        .iter()
        .find(|r| r.layers.matches(layer) && r.needs.matches(flags))
        .map(|r| Impact {
            level: r.level,
            text: r.text.to_string(),
        })
        .unwrap_or(Impact {
            level: ImpactLevel::None,
            text: String::new(),
        })
}

/// The impact of a pack that only one side has.
///
/// A pack key is `<layer>/<name>`, so a profile that exchanges one pack for a pack of another name
/// gives one removed pack and one added pack, and no identity change. The level therefore follows
/// the layer, as an identity change does. `added` is true for a pack that only side B has.
pub fn presence_impact(layer: &str, added: bool) -> Impact {
    let level = impact(
        Layer::parse(layer),
        Flags {
            identity: true,
            version: true,
            upstream: false,
            authored: false,
        },
    )
    .level;
    let what = if added { "added" } else { "removed" };
    let text = match level {
        ImpactLevel::Repave => format!("node repave ({layer} pack {what})"),
        ImpactLevel::ClusterWide => format!("cluster-wide infra addon {what}"),
        _ => format!("addon {what} in place"),
    };
    Impact { level, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impact_levels_order_by_severity() {
        assert!(ImpactLevel::Repave > ImpactLevel::MayRepave);
        assert!(ImpactLevel::MayRepave > ImpactLevel::ClusterWide);
        assert!(ImpactLevel::ClusterWide > ImpactLevel::InPlace);
        assert!(ImpactLevel::InPlace > ImpactLevel::Sync);
        assert!(ImpactLevel::Sync > ImpactLevel::None);
        let f = |identity, upstream, authored| Flags {
            identity,
            version: identity,
            upstream,
            authored,
        };
        assert_eq!(
            impact(Layer::K8s, f(true, true, true)).level,
            ImpactLevel::Repave
        );
        assert_eq!(
            impact(Layer::Cni, f(false, true, false)).level,
            ImpactLevel::Sync
        );
        assert_eq!(
            impact(Layer::Addon, f(false, false, true)).level,
            ImpactLevel::InPlace
        );
        assert_eq!(
            impact(Layer::Os, f(false, false, true)).level,
            ImpactLevel::Repave
        );
        assert_eq!(
            impact(Layer::K8s, f(false, false, true)).level,
            ImpactLevel::MayRepave
        );
        assert_eq!(
            impact(Layer::Other, f(true, false, false)).level,
            ImpactLevel::InPlace
        );
        assert_eq!(
            impact(Layer::Addon, f(false, false, false)).level,
            ImpactLevel::None
        );
    }

    #[test]
    fn a_node_image_pack_from_another_registry_at_the_same_version_takes_the_level_of_its_other_flags()
     {
        // the registry uid alone moved: no node repave, as the live run of the OS pack showed
        let moved = Flags {
            identity: true,
            ..Flags::default()
        };
        for layer in [Layer::Os, Layer::K8s, Layer::Kernel] {
            let imp = impact(layer, moved);
            assert_eq!(imp.level, ImpactLevel::None, "{layer:?}");
            assert_eq!(
                imp.text,
                "none (the same version from another pack reference)"
            );
        }
        // the digest moved with the registry, as between two kinds of registry
        let imp = impact(
            Layer::Os,
            Flags {
                upstream: true,
                ..moved
            },
        );
        assert_eq!(imp.level, ImpactLevel::Sync);
        assert_eq!(
            imp.text,
            "the same version from another pack reference, applied on next profile sync"
        );
        // a value of the OS pack still repaves, and a new version still repaves
        let imp = impact(
            Layer::Os,
            Flags {
                authored: true,
                ..moved
            },
        );
        assert_eq!(imp.level, ImpactLevel::Repave);
        assert_eq!(imp.text, "node repave (OS values change)");
        let imp = impact(
            Layer::K8s,
            Flags {
                version: true,
                ..moved
            },
        );
        assert_eq!(imp.level, ImpactLevel::Repave);
        // another layer keeps the identity rule
        assert_eq!(impact(Layer::Csi, moved).level, ImpactLevel::ClusterWide);
        assert_eq!(impact(Layer::Addon, moved).level, ImpactLevel::InPlace);
    }
}
