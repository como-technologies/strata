variable "palette_project" {
  description = "Palette project the profiles and the cluster land in"
  type        = string
}

variable "cloud" {
  description = "The cloud of the clusters: aws, gcp, eks, or gke. The cloud-<cloud>.tf files hold what changes with it"
  type        = string
  default     = "aws"
  validation {
    condition     = contains(["aws", "gcp", "eks", "gke"], var.cloud)
    error_message = "cloud is aws, gcp, eks, or gke."
  }
}

variable "cloud_account_name" {
  description = "Name of the AWS cloud account registered in Palette"
  type        = string
}

variable "region" {
  description = "AWS region"
  type        = string
}

variable "az" {
  description = "The single availability zone both machine pools use"
  type        = string
}

variable "ssh_key_name" {
  description = "Name of an EC2 key pair that exists in the region; the provider requires one"
  type        = string
}

# The GCP clusters, when cloud is gcp.
variable "gcp_cloud_account_name" {
  description = "Name of the GCP cloud account registered in Palette"
  type        = string
  default     = ""
}

variable "gcp_project" {
  description = "The GCP project the clusters land in; the cloud account needs access to it"
  type        = string
  default     = ""
}

variable "gcp_region" {
  type    = string
  default = "us-east1"
}

variable "gcp_zone" {
  description = "The single zone both machine pools use"
  type        = string
  default     = "us-east1-b"
}

variable "gcp_instance_type" {
  description = "Machine type for both pools: 4 cores, as t3.xlarge has"
  type        = string
  default     = "e2-standard-4"
}

variable "name_prefix" {
  description = "Prefix for the profile names"
  type        = string
  default     = "strata-live"
}

variable "cluster_name" {
  description = "Cluster A's name: shares no part with second_cluster_name, so no prefix or substring match finds both"
  type        = string
  default     = "fred"
}

variable "instance_type" {
  description = "Instance type for both pools"
  type        = string
  default     = "t3.xlarge"
}

variable "worker_count" {
  # Palette refuses the cluster unless workers total more than 4 CPU cores: one 4-core
  # t3.xlarge fails with "A minimum allocation of '4.1' CPU cores is required across all
  # worker nodes". Two clear it.
  type    = number
  default = 2
}

variable "registry_name" {
  type    = string
  default = "Public Repo"
}

# The pack versions of examples/profiles/aws-stack@1.1.0.json, so the live run and the offline
# examples describe the same stack. The OS pack of each cloud has 24.04.
variable "os_version" {
  description = "The version of the OS pack; empty takes the version of the cloud's file"
  type        = string
  default     = ""
}

variable "k8s_version" {
  description = "The version of the Kubernetes pack; empty takes the version of the cloud's file"
  type        = string
  default     = ""
}

variable "cni_version" {
  description = "The version of the CNI pack; empty takes the version of the cloud's file"
  type        = string
  default     = ""
}

variable "csi_version" {
  description = "The version of the CSI pack; empty takes the version of the cloud's file"
  type        = string
  default     = ""
}

# The registries profile. verify.sh reads each pack's digest from its registry as well as from
# Palette, so the machine running it needs read access to both registries.
variable "helm_registry_name" {
  description = "A Helm registry in the tenant that serves charts without credentials"
  type        = string
  default     = "Bitnami"
}

variable "helm_chart" {
  type    = string
  default = "nginx"
}

variable "helm_chart_version" {
  type    = string
  default = "25.1.5"
}

variable "oci_registry_name" {
  description = "An OCI pack registry in the tenant; log in to it before verify.sh"
  type        = string
}

variable "oci_pack" {
  description = "An add-on pack in that registry"
  type        = string
}

variable "oci_pack_version" {
  type = string
}

# The add-on profile the clusters run, and examples/live/in-place.sh's variables.
variable "addon_version" {
  description = "Version of the add-on profile; in-place.sh bumps it"
  type        = string
  default     = "1.0.0"
}

variable "edit_addon_in_place" {
  description = "Change one data value of the add-on profile's ConfigMap without bumping the version"
  type        = bool
  default     = false
}

variable "cluster_addon_profile_uid" {
  description = "Pin the clusters to this add-on profile uid instead of the resource's current version"
  type        = string
  default     = ""
}

variable "cluster" {
  description = "Create cluster A. With false the example makes its profiles and no cluster, which costs nothing and takes seconds"
  type        = bool
  default     = true
}

variable "cluster_group" {
  description = "Create a cluster group on cluster A, with a virtual cluster in it. It runs in cluster A."
  type        = bool
  default     = false
}

variable "group_cpu_millicore" {
  description = "The CPU limit, in millicores, that the cluster group gives each virtual cluster. cluster-group.sh changes it."
  type        = number
  default     = 6000
}

variable "second_cluster" {
  description = "Create cluster B next to the cluster. Billable while it exists."
  type        = bool
  default     = false
}

variable "second_cluster_name" {
  description = "Cluster B's name: shares no part with cluster_name, so no prefix or substring match finds both"
  type        = string
  default     = "barney"
}

variable "immutable_profiles" {
  description = "The provider's immutable-clusterprofiles feature_preview flag, with skip_destroy on every profile"
  type        = bool
  default     = false
}

variable "next_variable" {
  description = "Give version 1.0.1 the profile variable strata_mode, default 600. repave.sh sets it."
  type        = bool
  default     = false
}

variable "next_variable_in_values" {
  description = "A kubeadm command of the Kubernetes pack of 1.0.1 reads strata_mode. It needs next_variable."
  type        = bool
  default     = false
}

variable "registry_move" {
  description = "Create the registry-move profile"
  type        = bool
  default     = false
}

variable "move_version" {
  type    = string
  default = "1.0.0"
}

variable "move_csi_to_oci" {
  description = "Take the move profile's CSI pack from move_registry_name instead of registry_name"
  type        = bool
  default     = false
}

variable "move_registry_name" {
  description = "An OCI pack registry that holds the CSI pack and the OS pack of the cloud at their versions"
  type        = string
  default     = "Palette Registry"
}

variable "move_os_registry" {
  description = "Take the OS pack of version 1.0.0 from move_registry_name, at the same version and values. registry-move-nodes.sh sets it."
  type        = bool
  default     = false
}

# examples/live/registry.sh's variables: the example's own OCI pack registry, and the profile that
# pins the pack the script pushes into it.
variable "aws_profile" {
  description = "The AWS profile that creates the ECR repository of the example's own pack registry"
  type        = string
}

variable "owned_pack_profile" {
  description = "Create the profile that pins the pack of the example's own registry; registry.sh sets it after the push"
  type        = bool
  default     = false
}

variable "owned_pack_version" {
  description = "The version of that pack the profile pins"
  type        = string
  default     = "1.0.0"
}

variable "owned_pack_tag" {
  description = "The tag the profile pins the pack under; a floating tag such as 1.x when the registry lists one"
  type        = string
  default     = "1.0.0"
}

variable "owned_chart_profile" {
  description = "Create the profile that pins the chart of the example's own OCI Helm registry; registry.sh sets it after the push"
  type        = bool
  default     = false
}

variable "owned_chart_version" {
  description = "The version of that chart the profile pins"
  type        = string
  default     = "1.0.0"
}

variable "owned_helm_registry" {
  description = "The Palette registry over the example's OCI Helm repository; registry.sh removes it and makes it again, so Palette reads the chart for the first time"
  type        = bool
  default     = true
}

variable "dep_profile" {
  description = "The profile of the dependency check: none; alone, with strata-live-pack 1.1.0 and not the pack it requires; both, with the two packs. registry.sh sets it after the pushes"
  type        = string
  default     = "none"
  validation {
    condition     = contains(["none", "alone", "both"], var.dep_profile)
    error_message = "dep_profile is none, alone, or both."
  }
}

# examples/live/cluster.sh's variable: the value cluster A gives the variable of the add-on
# profile. Empty gives none, and the variable has its default.
variable "addon_purpose" {
  description = "The value the cluster gives the add-on profile's variable purpose; empty gives none"
  type        = string
  default     = ""
}

# examples/live/template.sh's variables (#66): a cluster template over the release and add-on
# versions, and a second add-on profile the cluster runs beside the first.
variable "template" {
  description = "Create the maintenance policy and the cluster template that link the release and add-on versions"
  type        = bool
  default     = false
}

variable "second_addon" {
  description = "Create a second add-on profile, and run it on the clusters beside the first"
  type        = bool
  default     = false
}

# examples/live/tag-only.sh's variables (#19).
variable "tag_only_versions" {
  description = "Release versions that differ from 1.0.0 only in the OS pack's tag name, as version => tag"
  type        = map(string)
  default     = {}
}

variable "cluster_release_profile_uid" {
  description = "Pin the clusters to this release profile uid instead of 1.0.0"
  type        = string
  default     = ""
}

# examples/live/registry-kinds.sh's variable (#75).
variable "registry_kinds" {
  description = "Create the registry host of registry-kinds.tf and a Palette registry over each kind it serves; registry-kinds.sh sets it"
  type        = bool
  default     = false
}

variable "extra_addon_profile_uids" {
  description = "More add-on profile versions for the clusters to run, by uid: profiles that a script made through the API"
  type        = list(string)
  default     = []
}

variable "eks_azs" {
  description = "The availability zones of an EKS cluster; empty takes the zones a and b of the region"
  type        = list(string)
  default     = []
}

variable "eks_instance_type" {
  description = "The instance type of the managed node group of an EKS cluster"
  type        = string
  default     = "t3.xlarge"
}

variable "eks_worker_count" {
  description = "The nodes of the managed node group of an EKS cluster"
  type        = number
  default     = 2
}

variable "eks_k8s_bump" {
  description = "With cloud eks: a Kubernetes version for the profile version 1.0.9, which is the release with that version alone"
  type        = string
  default     = ""
}

variable "palette_core_policies" {
  description = "With cloud eks: the customer managed core policies of Palette in the AWS account, by name, for the role of the example's own cloud account"
  type        = list(string)
  default = [
    "PaletteControllerPolicy",
    "PaletteControlPlanePolicy",
    "PaletteNodesPolicy",
    "PaletteDeploymentPolicy",
    "PaletteCAPATaggingPolicy",
    "PaletteCloudFormationPolicy",
  ]
}

variable "gke_instance_type" {
  description = "The machine type of the node pool of a GKE cluster"
  type        = string
  default     = "e2-standard-4"
}

variable "gke_worker_count" {
  description = "The nodes of the node pool of a GKE cluster: a multiple of 3, which Palette requires of each GKE worker pool"
  type        = number
  default     = 3
}

variable "gke_k8s_bump" {
  description = "With cloud gke: a Kubernetes version for the profile version 1.0.9, which is the release with that version alone"
  type        = string
  default     = ""
}
