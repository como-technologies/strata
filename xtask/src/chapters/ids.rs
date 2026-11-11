//! The uids of the profiles, the clusters, and the other objects that the chapters build. Each uid
//! names one object of the stand-in.
//!
//! A test reads each const of this file. It fails when two consts have the same value, and names
//! the two consts.

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
// aws-stack@1.1.12: the content of 1.1.1, with a profile type that this release does not know
pub(super) const A1112: &str = "64e000000000000000000100";
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
// observability@1.0.0 and payments-apps@1.0.0: two more add-on profiles of payments in dev, each
// with a manifest pack of its own
pub(super) const OBSERVABILITY: &str = "64e2000000000000000000c1";
pub(super) const OBSERVABILITY_PACK: &str = "64e2000000000000000000d5";
pub(super) const PAYMENTS_APPS: &str = "64e2000000000000000000c2";
pub(super) const PAYMENTS_APPS_PACK: &str = "64e2000000000000000000d6";
// aws-stack@1.1.13 to 1.1.17: the content of 1.1.1, whose Kubernetes pack names one more image. The
// image moves from a short tag (1.1.13) to a build tag (1.1.14) and to another registry (1.1.15).
// Then it gets a digest that changes (1.1.16 and 1.1.17)
pub(super) const A1113: &str = "64e000000000000000000101";
pub(super) const A1114: &str = "64e000000000000000000102";
pub(super) const A1115: &str = "64e000000000000000000103";
pub(super) const A1116: &str = "64e000000000000000000104";
pub(super) const A1117: &str = "64e000000000000000000105";
// aws-stack@1.1.8, its develop build 1.1.8.develop, and 1.1.10: the content of 1.1.1, in the
// versions that a field test found (#616)
pub(super) const DEVELOP_118: &str = "64e000000000000000000106";
pub(super) const DEVELOP_BUILD: &str = "64e000000000000000000107";
pub(super) const DEVELOP_1110: &str = "64e000000000000000000108";
// aws-stack@1.1.2 of the sample pipeline: the content of 1.1.1 with the reclaim policy Retain, as
// the candidate of a merge request, the pre-release of the merge, and the final version
pub(super) const SAMPLE_CANDIDATE: &str = "64e000000000000000000109";
pub(super) const SAMPLE_PRE: &str = "64e00000000000000000010a";
pub(super) const SAMPLE_FINAL: &str = "64e00000000000000000010b";
// aws-stack@1.1.3 of the sample pipeline: the content of 1.1.1 with a new build of the CSI pack, as
// the pre-release that pins the pre-release of the pack, and the final version that pins its final
pub(super) const SAMPLE_PIN_PRE: &str = "64e00000000000000000010c";
pub(super) const SAMPLE_PIN_FINAL: &str = "64e00000000000000000010d";
// aws-stack@1.2.0 of the use case of a renumber: the pre-release pins a pre-release of the
// Kubernetes pack and of the CSI pack, and the final version pins the final build of each. 1.2.1 is
// the final version with one more value
pub(super) const RENUMBER_PRE: &str = "64e00000000000000000010e";
pub(super) const RENUMBER_FINAL: &str = "64e00000000000000000010f";
pub(super) const RENUMBER_EDITED: &str = "64e000000000000000000110";
// aws-stack@1.2.develop: a develop build with no three numbers, beside the versions of #616 (#714)
pub(super) const DEVELOP_SHORT: &str = "64e000000000000000000111";
// aws-stack in the chapter behind-profiles (#707): the pre-release 1.2.0-rc.1 in dev, 1.3.0 in
// prod, and 2.0.0 at tenant scope, each with the content of 1.1.1. Only 1.1.0 of dev is a newer
// version of 1.0.0 of dev
pub(super) const A120_RC: &str = "64e000000000000000000112";
pub(super) const A130_PROD: &str = "64e100000000000000000110";
pub(super) const A200_TENANT: &str = "64e400000000000000000111";
// aws-stack@1.2.2 of the use case of a renumber: the final build of the CSI pack has another default
// value, and the profile gives the pin the same values
pub(super) const RENUMBER_REBUILT: &str = "64e000000000000000000113";

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use syn::visit::Visit;
    use syn::{Expr, ExprLit, ItemConst, Lit, parse_file};

    use crate::files::code_without_tests;

    /// The consts of a Rust file. `by_uid` holds the names of the consts for each value, in the
    /// order of the file. `unread` holds each const whose value is no string literal.
    #[derive(Default)]
    struct Consts {
        by_uid: BTreeMap<String, Vec<String>>,
        unread: Vec<String>,
    }

    impl<'ast> Visit<'ast> for Consts {
        fn visit_item_const(&mut self, item: &'ast ItemConst) {
            let name = item.ident.to_string();
            match &*item.expr {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(uid), ..
                }) => self.by_uid.entry(uid.value()).or_default().push(name),
                _ => self.unread.push(name),
            }
        }
    }

    /// The consts of the Rust file `text`, at each depth, outside its test code.
    fn consts(text: &str) -> Consts {
        let code = code_without_tests(text).unwrap();
        let mut consts = Consts::default();
        consts.visit_file(&parse_file(&code).unwrap());
        consts
    }

    /// One message for each value that two or more consts of `text` have, with the names of these
    /// consts. Also one message for each const whose value the test cannot read.
    fn faults(text: &str) -> Vec<String> {
        let consts = consts(text);
        let shared = consts
            .by_uid
            .iter()
            .filter(|(_, names)| names.len() > 1)
            .map(|(uid, names)| format!("{} have the same uid {uid}", names.join(" and ")));
        let unread = consts
            .unread
            .iter()
            .map(|name| format!("{name} has no string literal as its uid"));
        shared.chain(unread).collect()
    }

    #[test]
    fn two_consts_with_the_same_value_are_a_fault_that_names_the_two_consts() {
        let text =
            "const A: &str = \"64e0\";\nconst B: &str = \"64e1\";\nconst C: &str = \"64e0\";\n";
        assert_eq!(faults(text), ["A and C have the same uid 64e0"]);
    }

    #[test]
    fn a_const_whose_value_is_no_string_literal_is_a_fault() {
        let text = "const A: &str = concat!(\"64e\", \"0\");\n";
        assert_eq!(faults(text), ["A has no string literal as its uid"]);
    }

    #[test]
    fn the_test_reads_the_consts_of_a_module_and_not_the_consts_of_the_tests() {
        let text = "mod m {\n    const A: &str = \"64e0\";\n}\n#[cfg(test)]\nmod tests {\n    const B: &str = \"64e0\";\n}\n";
        let consts = consts(text);
        assert_eq!(
            consts.by_uid,
            BTreeMap::from([("64e0".to_string(), vec!["A".to_string()])])
        );
    }

    #[test]
    fn each_const_of_the_chapters_has_a_uid_of_its_own() {
        let text = include_str!("ids.rs");
        let faults = faults(text);
        assert!(faults.is_empty(), "{}", faults.join("\n"));
        // the scan reads this file: it finds the first const and the last const
        let by_uid = consts(text).by_uid;
        for (uid, name) in [
            (super::DEV, "DEV"),
            (super::RENUMBER_REBUILT, "RENUMBER_REBUILT"),
        ] {
            assert_eq!(by_uid.get(uid), Some(&vec![name.to_string()]));
        }
    }
}
