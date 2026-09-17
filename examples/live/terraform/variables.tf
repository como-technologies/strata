variable "palette_project" {
  description = "Palette project the profiles and the cluster land in"
  type        = string
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

# The pack versions of examples/profiles/aws-pxk@1.1.0.json, so the live run and the offline
# examples describe the same stack.
variable "os_version" {
  type    = string
  default = "24.04"
}

variable "k8s_version" {
  type    = string
  default = "1.36.2"
}

variable "cni_version" {
  type    = string
  default = "1.20.1"
}

variable "csi_version" {
  type    = string
  default = "1.65.0"
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

variable "keep_profile_versions" {
  description = "skip_destroy on every profile with the flag off, for an apply that must not undo it (see examples/live/lib.sh)"
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
  description = "An OCI pack registry that holds csi-aws-ebs at csi_version"
  type        = string
  default     = "Palette Registry"
}
