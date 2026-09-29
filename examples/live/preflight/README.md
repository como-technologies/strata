# The entities of the test plan

One body of a create for each engine, with the values of the tenant as `__PLACEHOLDERS__`:
`preflight-plan.sh` fills them from `terraform.tfvars` and the cloud accounts of the project, and
edits the result to make each fault of the plan. Each file is the shape `POST /v1/spectroclusters/{cloudType}`
takes; the GKE file is the GCP entity with `managedClusterConfig`, as Palette takes it.
