#!/usr/bin/env bash
# Deploy the endpoint of the report to Cloud Run, in the project of the lab.
#
#   deploy/report/deploy.sh [--project ID] [--region NAME] [--dry-run]
#
# The lead runs this on kadomony after the merge, with a gcloud login. The script builds the image
# on Cloud Build, and deploys it as the service strata-report. Then it prints the address of the
# service. The script reads no secret: the token of GitHub stays in Secret Manager, and Cloud Run
# gives it to the service as GITHUB_TOKEN when the service starts.
#
# The service and the build each run as an account of their own. The default compute account of the
# project runs neither of them. It holds the role roles/compute.admin, and a fault in a public
# service can give that power to a sender. It gets no role and no access to the secret.
#   - The account strata-report runs the service. It has one role: it reads the secret
#     strata-report-token (roles/secretmanager.secretAccessor).
#   - The account strata-report-build runs the build. It has three roles: it reads the source in the
#     bucket PROJECT_cloudbuild (roles/storage.objectViewer), it writes the log of the build
#     (roles/logging.logWriter on the project), and it writes the image to the repository strata
#     (roles/artifactregistry.writer).
#
# The project has these before the first run:
#   - The APIs run, cloudbuild, artifactregistry, secretmanager, and iam are on.
#   - The secret strata-report-token holds a token of GitHub that can write the issues of
#     como-technologies/strata. It has the permission Issues: read and write, and no other.
#   - The two accounts exist, with the roles above. A person who owns the project runs these
#     commands one time, and the script runs none of them. A create command that says the
#     repository or the bucket exists is harmless. "Make the two accounts" in
#     docs/src/development/report-endpoint.md gives the same commands.
#
#       project=como-strata-test
#       region=us-central1
#       service_account=strata-report@$project.iam.gserviceaccount.com
#       build_account=strata-report-build@$project.iam.gserviceaccount.com
#
#       gcloud iam service-accounts create strata-report --project "$project" \
#         --display-name "The service of the report endpoint"
#       gcloud iam service-accounts create strata-report-build --project "$project" \
#         --display-name "The build of the report endpoint"
#       gcloud artifacts repositories create strata --project "$project" --location "$region" \
#         --repository-format docker --description "images of strata"
#       gcloud storage buckets create "gs://${project}_cloudbuild" --project "$project" \
#         --location "$region"
#       gcloud secrets add-iam-policy-binding strata-report-token --project "$project" \
#         --member "serviceAccount:$service_account" --role roles/secretmanager.secretAccessor
#       gcloud storage buckets add-iam-policy-binding "gs://${project}_cloudbuild" \
#         --member "serviceAccount:$build_account" --role roles/storage.objectViewer
#       gcloud projects add-iam-policy-binding "$project" \
#         --member "serviceAccount:$build_account" --role roles/logging.logWriter --condition=None
#       gcloud artifacts repositories add-iam-policy-binding strata \
#         --project "$project" --location "$region" \
#         --member "serviceAccount:$build_account" --role roles/artifactregistry.writer
#
# The person who runs the script must be able to act as both accounts
# (roles/iam.serviceAccountUser). An owner of the project can.
#
# The service takes one instance and 8 requests at a time. One instance keeps the limits of the
# rate in one place. A second instance splits the count of an issue.
#
# --project and --region replace the defaults, which are the project of the lab and us-central1.
# --dry-run prints each gcloud command and runs none.
set -Eeuo pipefail

script=deploy
die() { echo "$script: $*" >&2; exit 2; }

project="${STRATA_REPORT_PROJECT:-como-strata-test}"
region="${STRATA_REPORT_REGION:-us-central1}"
service=strata-report
secret=strata-report-token
registry=strata
dry=""
while [ $# -gt 0 ]; do
  case "$1" in
    --project) [ $# -ge 2 ] || die "--project needs a value"; project=$2; shift 2 ;;
    --region) [ $# -ge 2 ] || die "--region needs a value"; region=$2; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,/^set -Eeuo/{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)
tag=$(git -C "$root" rev-parse --short=12 HEAD 2> /dev/null) || tag=latest
image="$region-docker.pkg.dev/$project/$registry/report-endpoint:$tag"
service_account="strata-report@$project.iam.gserviceaccount.com"
build_account="strata-report-build@$project.iam.gserviceaccount.com"

# run COMMAND...: print the command, and run it unless this is a dry run
run() {
  echo "+ $*" >&2
  [ -n "$dry" ] || "$@"
}

# ask COMMAND...: print a command that reads, and run it unless this is a dry run. The error text of
# the command is hidden, and a dry run gives no output and the status 0.
ask() {
  echo "+ $*" >&2
  [ -n "$dry" ] || "$@" 2> /dev/null
}

[ -n "$dry" ] || command -v gcloud > /dev/null || die "gcloud is not installed"
ask gcloud secrets describe "$secret" --project "$project" > /dev/null \
  || die "the secret $secret does not exist in the project $project: create it, and put the token in it"
# The script makes no account and grants no role: a person who owns the project does it one time.
for account in "$service_account" "$build_account"; do
  ask gcloud iam service-accounts describe "$account" --project "$project" > /dev/null \
    || die "the service account $account does not exist in the project $project: make it with the commands of \"Make the two accounts\" in docs/src/development/report-endpoint.md"
done
if ! ask gcloud artifacts repositories describe "$registry" --project "$project" --location "$region" > /dev/null; then
  run gcloud artifacts repositories create "$registry" --project "$project" --location "$region" \
    --repository-format docker --description "images of strata"
fi

cd "$root"
# The build and the service each run as their own account, never as the default compute account.
run gcloud builds submit --project "$project" --config deploy/report/cloudbuild.yaml \
  --service-account "projects/$project/serviceAccounts/$build_account" \
  --substitutions "_IMAGE=$image" .
# The service answers a report and then writes it to GitHub, so Cloud Run must keep the CPU on
# after the answer (--no-cpu-throttling). Cloud Run refuses a service with less than 512 MiB of
# memory when the CPU stays on, so the memory is 512Mi.
run gcloud run deploy "$service" --project "$project" --region "$region" --image "$image" \
  --service-account "$service_account" --allow-unauthenticated --max-instances 1 --concurrency 8 --cpu 1 --memory 512Mi --timeout 60 \
  --no-cpu-throttling \
  --set-secrets "GITHUB_TOKEN=$secret:latest" --set-env-vars "TRUST_FORWARDED_FOR=1"

# The request log of Cloud Run holds the address of each sender. The endpoint keeps no address, so
# the log is not kept either: an exclusion of the default sink drops it. The exclusion names the
# service, and no other log.
exclusion=strata-report-requests
if ! ask gcloud logging sinks describe _Default --project "$project" | grep -q "name: $exclusion"; then
  run gcloud logging sinks update _Default --project "$project" --add-exclusion \
    "name=$exclusion,description=The requests of $service hold the address of a sender,filter=resource.type=\"cloud_run_revision\" AND resource.labels.service_name=\"$service\" AND log_id(\"run.googleapis.com/requests\")"
fi

url=$(run gcloud run services describe "$service" --project "$project" --region "$region" --format 'value(status.url)')
[ -z "$dry" ] || exit 0
echo "$service is at $url"
echo "Set STRATA_REPORT_URL=$url to send one report from the lab."
