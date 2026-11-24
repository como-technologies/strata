# The names of the tenant come from examples/live/terraform/terraform.tfvars: platform.sh gives that
# file to each plan, apply, and destroy. The pins of the packs come from terraform.tfvars in this
# folder, which `examples/live/platform.sh pins` writes.

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

variable "aws_profile" {
  description = "AWS profile that makes the private zone and the policy of external-dns"
  type        = string
}

variable "immutable_profiles" {
  description = "The immutable-clusterprofiles flag of the provider; lib.sh gives it to each apply"
  type        = bool
  default     = false
}

variable "release_profile_name" {
  description = "The cluster profile of the lab that wilma runs; just live-profiles or just live-up makes it"
  type        = string
  default     = "strata-live-profile"
}

variable "release_profile_version" {
  type    = string
  default = "1.0.0"
}

variable "cluster_name" {
  description = "The platform cluster: it shares no part with fred and barney"
  type        = string
  default     = "wilma"
}

variable "instance_type" {
  description = "Instance type for both pools: four t3.xlarge nodes cost less than $1 an hour"
  type        = string
  default     = "t3.xlarge"
}

variable "worker_count" {
  type    = number
  default = 3
}

variable "domain" {
  description = "The default of the profile variable domain, and the name of the private zone"
  type        = string
  default     = "wilma.strata-live.internal"
}

variable "node_role_name" {
  description = "The IAM role of the worker nodes, which external-dns uses to write the private zone"
  type        = string
  default     = "nodes.cluster-api-provider-aws.sigs.k8s.io"
}

variable "node_policy_name" {
  description = "The inline policy of the role of the nodes that lets external-dns write the private zone"
  type        = string
  default     = "strata-live-wilma-external-dns"
}

variable "packs" {
  description = "The pin of each pack: its registry, its version, and its type (spectro, helm, or oci)"
  type = map(object({
    registry = string
    version  = string
    type     = string
  }))
  validation {
    condition     = alltrue([for p in values(var.packs) : contains(["spectro", "helm", "oci"], p.type) && p.version != ""])
    error_message = "Each pin has a version and the type spectro, helm, or oci. examples/live/platform.sh pins writes them."
  }
}

variable "sso" {
  description = "The SSO of Argo CD and Grafana through Keycloak, in the values of the three packs"
  type        = bool
  default     = true
}

# The workload cluster pebbles (#701) is in a second project of the tenant. platform-lib.sh gives the
# name of that project from STRATA_LIVE_SECOND_PROJECT, so no file of the repository names it.

variable "second_project" {
  description = "The second project of the tenant, where pebbles runs; empty for no pebbles and no profile of the tenant"
  type        = string
  default     = ""
}

variable "second_cloud_account_name" {
  description = "The AWS cloud account of pebbles in the second project; empty for cloud_account_name"
  type        = string
  default     = ""
}

variable "workload_name" {
  description = "The workload cluster in the second project"
  type        = string
  default     = "pebbles"
}

variable "workload_worker_count" {
  type    = number
  default = 2
}

# The variables below are the cases of platform.sh. Their defaults are the baseline of wilma.

variable "addons" {
  description = "false leaves only the release profile on wilma: the first step of platform.sh down"
  type        = bool
  default     = true
}

variable "data_profile" {
  description = "false removes wilma-data from wilma, and the Cluster of Keycloak has no CRD (case 6)"
  type        = bool
  default     = true
}

variable "cluster_domain" {
  description = "The value that wilma gives the variable domain of the five profiles; empty for the default (case 4)"
  type        = string
  default     = ""
}

variable "network_cert_manager" {
  description = "A version of cert-manager for wilma-network 1.1.0, which no cluster runs; empty for none (case 2)"
  type        = string
  default     = ""
}

variable "network_cert_manager_in_place" {
  description = "true adds that cert-manager to wilma-network 1.0.0 in place, which wilma runs (case 2)"
  type        = bool
  default     = false
}

variable "move_fluentbit" {
  description = "true runs wilma-observability 1.1.0 with no fluentbit, and fluentbit in wilma-logging (case 3)"
  type        = bool
  default     = false
}

variable "vault_version" {
  description = "Another version of vault: wilma runs it in wilma-security 1.1.0; empty for the pin (case 2 of platform-boundary.sh)"
  type        = string
  default     = ""
}
