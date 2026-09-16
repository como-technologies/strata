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
  description = "Prefix for the cluster and profile names"
  type        = string
  default     = "strata-live"
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

variable "edit_next_in_place" {
  description = "Change a value of profile version 1.0.1 in place after it is published, to see what Palette does and what strata verify reports. Nothing runs 1.0.1."
  type        = bool
  default     = false
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
