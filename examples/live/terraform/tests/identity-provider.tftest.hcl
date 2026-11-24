# The variable k8s_identity_provider of the cluster of stage 10 of tiers.sh (#915). The providers
# are mocks, so the test sends no request. `just live-values` runs it.
#
# The values of the Kubernetes pack have the form of the registry. They hold a line of presets, the
# line "pack:", and the kubeadm command of next_variable_in_values. With the default, 1.0.0 and
# 1.0.1 keep the values of each pack. With the variable, the Kubernetes pack gets the four lines of
# the identity provider under "pack:", and no other line changes.

# The run "values_with_no_line_pack_stop_the_plan" gives other values to the Kubernetes pack.
mock_provider "spectrocloud" {
  override_data {
    target = data.spectrocloud_pack.k8s
    values = {
      values = <<-EOT
        # spectrocloud.com/enabled-presets: Kube Controller Manager:loopback-ctrlmgr
        pack:
          content:
            images:
              - image: us-docker.pkg.dev/palette-images/k8s/kube-apiserver:v1.36.2
          podCIDR: "192.168.0.0/16"

        kubeadmconfig:
          postKubeadmCommands:
            - 'chmod 600 /var/lib/kubelet/config.yaml'
      EOT
    }
  }
}
# The policy of the registry user is a JSON object, which the provider of AWS reads at the plan.
mock_provider "aws" {
  mock_data "aws_iam_policy_document" {
    defaults = { json = "{}" }
  }
}
mock_provider "time" {}
mock_provider "random" {}
mock_provider "external" {}

variables {
  palette_project    = "project"
  cloud_account_name = "account"
  region             = "us-east-2"
  az                 = "us-east-2a"
  ssh_key_name       = "key"
  oci_registry_name  = "oci"
  oci_pack           = "pack"
  oci_pack_version   = "1.0.0"
  aws_profile        = "profile"
  # A terraform.tfvars of the host can set these, and the runs need their defaults.
  cloud                   = "aws"
  k8s_identity_provider   = ""
  next_variable           = false
  next_variable_in_values = false
}

# With the default, the plan gives each pack of 1.0.0, and the Kubernetes pack of 1.0.1, the values
# of the registry.
run "the_default_keeps_the_values_of_each_pack" {
  command = plan

  assert {
    condition = alltrue([for v in [spectrocloud_cluster_profile.release, spectrocloud_cluster_profile.next] :
    [for p in v.pack : p.values if p.name == data.spectrocloud_pack.k8s.name] == [data.spectrocloud_pack.k8s.values]])
    error_message = "a version of the release does not keep the values of the Kubernetes pack"
  }
  assert {
    condition = jsonencode({ for p in spectrocloud_cluster_profile.release.pack : p.name => p.values }) == jsonencode({
      for d in [data.spectrocloud_pack.os, data.spectrocloud_pack.k8s, data.spectrocloud_pack.cni, data.spectrocloud_pack.csi] : d.name => d.values
    })
    error_message = "version 1.0.0 does not keep the values of each pack"
  }
}

# With the variable, the Kubernetes pack of 1.0.0 and 1.0.1 names the identity provider palette.
# The other packs, and each other line of the Kubernetes pack, stay.
run "the_variable_adds_only_the_identity_provider_to_the_kubernetes_pack" {
  command = plan

  variables {
    k8s_identity_provider = "palette"
  }

  assert {
    condition = [for p in spectrocloud_cluster_profile.release.pack : p.values if p.name == data.spectrocloud_pack.k8s.name] == [<<-EOT
      # spectrocloud.com/enabled-presets: Kube Controller Manager:loopback-ctrlmgr
      pack:
        palette:
          config:
            dashboard:
              identityProvider: palette
        content:
          images:
            - image: us-docker.pkg.dev/palette-images/k8s/kube-apiserver:v1.36.2
        podCIDR: "192.168.0.0/16"

      kubeadmconfig:
        postKubeadmCommands:
          - 'chmod 600 /var/lib/kubelet/config.yaml'
    EOT
    ]
    error_message = "the Kubernetes pack of 1.0.0 gets a change beside the four lines of the identity provider"
  }
  assert {
    condition = [for p in spectrocloud_cluster_profile.release.pack : yamldecode(p.values).pack.palette.config.dashboard.identityProvider
    if p.name == data.spectrocloud_pack.k8s.name] == ["palette"]
    error_message = "the Kubernetes pack of 1.0.0 does not name the identity provider palette"
  }
  assert {
    condition = jsonencode({ for p in spectrocloud_cluster_profile.release.pack : p.name => p.values if p.name != data.spectrocloud_pack.k8s.name }) == jsonencode({
      for d in [data.spectrocloud_pack.os, data.spectrocloud_pack.cni, data.spectrocloud_pack.csi] : d.name => d.values
    })
    error_message = "the variable changes the values of another pack of 1.0.0"
  }
  assert {
    condition = [for p in spectrocloud_cluster_profile.next.pack : p.values if p.name == data.spectrocloud_pack.k8s.name] == [
    for p in spectrocloud_cluster_profile.release.pack : p.values if p.name == data.spectrocloud_pack.k8s.name]
    error_message = "the Kubernetes pack of 1.0.1 differs from the Kubernetes pack of 1.0.0"
  }
}

# With the variable and the variable in the values of repave.sh, the Kubernetes pack of 1.0.1 has
# both edits.
run "version_1_0_1_keeps_the_identity_provider_with_the_variable_in_the_values" {
  command = plan

  variables {
    k8s_identity_provider   = "palette"
    next_variable           = true
    next_variable_in_values = true
  }

  assert {
    condition = [for p in spectrocloud_cluster_profile.next.pack : yamldecode(p.values).pack.palette.config.dashboard.identityProvider
    if p.name == data.spectrocloud_pack.k8s.name] == ["palette"]
    error_message = "the Kubernetes pack of 1.0.1 does not name the identity provider palette"
  }
  assert {
    condition = [for p in spectrocloud_cluster_profile.next.pack : yamldecode(p.values).kubeadmconfig.postKubeadmCommands
    if p.name == data.spectrocloud_pack.k8s.name] == [["chmod {{.spectro.var.strata_mode}} /var/lib/kubelet/config.yaml"]]
    error_message = "the Kubernetes pack of 1.0.1 loses the variable in its values"
  }
}

# Values with no line "pack:" get no identity provider, so the plan stops.
run "values_with_no_line_pack_stop_the_plan" {
  command = plan

  variables {
    k8s_identity_provider = "palette"
  }

  override_data {
    target = data.spectrocloud_pack.k8s
    values = { values = "kubeadmconfig:\n  postKubeadmCommands: []\n" }
  }

  expect_failures = [spectrocloud_cluster_profile.release]
}

# A name with a capital letter is not a name of an identity provider of the pack.
run "a_name_with_a_capital_letter_is_refused" {
  command = plan

  variables {
    k8s_identity_provider = "Palette"
  }

  expect_failures = [var.k8s_identity_provider]
}
