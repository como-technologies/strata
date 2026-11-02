#!/usr/bin/env bash
# Deploy the endpoint of the report to Cloud Run, in the project of the lab.
#
#   deploy/report/deploy.sh [--project ID] [--region NAME] [--dry-run]
#
# The lead runs this on kadomony after the merge, with gcloud logged in. The script builds the image
# on Cloud Build, and deploys it as the service strata-report. Then it prints the address of the
# service. The script reads no secret: the token of GitHub stays in Secret Manager, and Cloud Run
# gives it to the service as GITHUB_TOKEN when the service starts.
#
# The project has these before the first run:
#   - The APIs run, cloudbuild, artifactregistry, and secretmanager are on.
#   - The secret strata-report-token holds a token of GitHub that can write the issues of
#     como-technologies/strata. It has the permission Issues: read and write, and no other.
#   - The service account of Cloud Run can read the secret (roles/secretmanager.secretAccessor).
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
repository=como-technologies/strata
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
if ! ask gcloud artifacts repositories describe "$registry" --project "$project" --location "$region" > /dev/null; then
  run gcloud artifacts repositories create "$registry" --project "$project" --location "$region" \
    --repository-format docker --description "images of strata"
fi

cd "$root"
run gcloud builds submit --project "$project" --config deploy/report/cloudbuild.yaml \
  --substitutions "_IMAGE=$image" .
# The service answers a report and then writes it to GitHub, so Cloud Run must keep the CPU on
# after the answer (--no-cpu-throttling).
run gcloud run deploy "$service" --project "$project" --region "$region" --image "$image" \
  --allow-unauthenticated --max-instances 1 --concurrency 8 --cpu 1 --memory 256Mi --timeout 60 \
  --no-cpu-throttling \
  --set-secrets "GITHUB_TOKEN=$secret:latest" --set-env-vars "GITHUB_REPOSITORY=$repository,TRUST_FORWARDED_FOR=1"

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
