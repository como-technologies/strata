# The endpoint of the report

The endpoint is a small service that takes the report of the values that strata does not know. A
user of strata sends the report only by choice: see
[Send the report by itself](../reference/report.md#send-the-report-by-itself). This page is for the
maintainers. It says what the service checks, what it writes, and how the lead deploys it.

The service is the package `strata-report-endpoint` in `crates/strata-report-endpoint/`. It runs on Cloud Run, in
the project of the lab, and it writes issues of the repository of strata. No release of strata
carries it.

## The checks of a request

The service checks each request in this order. A request that fails a check changes nothing.

| Order | Check | Answer when it fails |
| --- | --- | --- |
| 1 | The path is `/` and the method is `POST`. | 404 or 405 |
| 2 | The content type is JSON, the body has 64 KiB or fewer, and a key has the right form. | 415, 413, or 400 |
| 3 | The body is `strata-report/1`, with no field outside the list. Each value is a short word. | 422 |
| 4 | The sender made 10 requests or fewer in the last hour. | 429, with `Retry-After` |
| 5 | The service did not take this report in the last day. | 200, with `duplicate` |
| 6 | The tracker lists the issues. | 502 |
| 7 | The hour has room for at least one step of the report. | 429, with `Retry-After` |

Check 3 is the type `Report` of the library. The send of strata and the endpoint use the same type,
so the list of fields has one definition. A report has 1 to 100 entries. The check refuses a
count of more than 1,000,000.

The check also refuses a route that is not a template of a route. It refuses a value of more than
64 characters, and a value with a space or with a character other than printable ASCII. Check 4
comes after
checks 2 and 3, so a request that fails them spends no request of the sender.

A request that passes check 7 gets the answer 202, and the service writes the report after the
answer. The answer is JSON: `{"ok":true,"opens":1,"updates":0}`, or `{"error":"..."}`. The message of
an error never repeats a value of the body. A field of the wrong type gets a fixed text with the
line and the column. A failure of the tracker goes to standard error with no address, and the
sender gets a short sentence.

## The key of a send

The send of strata puts a key in the header `Idempotency-Key`. The key is 64 characters of hex: the
digest of the report and of the number of lines of the log before it. A second try of the same
lines has the same key. A request without a key is never a repeat.

- Check 5 hashes the key and the content of the report together, and it keeps the hash for one
  day. A request that has the hash gets 200 and `{"ok":true,"duplicate":true}`, and it changes
  nothing. So a send that timed out and came again counts one time.
- A key alone stops no other report, because the hash holds the content too.
- Two reports of equal content from two senders on the same day, at the same place of their logs,
  count one time. The counts show how often a value comes, and they are not exact.
- When the tracker fails after the answer, the service forgets the hash. A second try of the
  report then counts.
- The service keeps the hash only when it took the whole report. When the answer has `waiting`, the
  service keeps no hash. The next send of that report has the same key, and it gets a new plan and
  a new list `waiting`. Strata reads the answer `duplicate` as "the service took all". So a report
  with a value that waits must not get it.

## The limits

Three limits protect the tracker. They hold for all senders together, except the first.

| Limit | Value | What the service does over it |
| --- | --- | --- |
| Requests of one sender | 10 in an hour | Check 4 answers 429. |
| New issues | 20 in an hour | The step waits. |
| Writes: new issues and changes | 200 in an hour | The step waits. |

A sender is an address. An IPv6 address counts by its /64 network, because one machine can send from
all the addresses of its network. The table holds 10,000 senders. When more senders come, the oldest
sender leaves the table. So a flood of addresses cannot lock out a new sender.

The service also drops each sender whose hour is over, and each hash of a report whose day is over.
It does this when a request comes, one time in a minute at most. A request that a check refuses
also does it.

A step is one set and one value of the report. The service keeps the steps that the new-issue limit
and the write limit take, in the order of the set and the value. For each step it does not keep, the
answer lists the numbers of the entries of the report, from 1, in `waiting`:
`{"ok":true,"opens":20,"updates":0,"waiting":[21]}`. The service writes the other steps. Strata
makes a report of 20 values or fewer, so an hour that no other sender used takes a whole report. When
the limits keep no step, the answer is 429 with `Retry-After`, and the service writes nothing.

Strata reads `waiting`. It marks the lines before the first line of a value that waits, and it
stops. So a value that waits comes back with the next send. The lines after that line come back
too, and the service counts a line that it took one more time. See
[What a run sends](../reference/report.md#what-a-run-sends).

The merge is not atomic. When the tracker fails in the middle of a report, the issues written so
far stay. The sender has its answer by then, so the rest of the report is lost.

The service reads at most 1000 issues of its label, open and closed. A closed issue counts, because
the service must find the issue of a value to give it the new count. When the label has more, the
service answers 502 and logs the cause. It does not open a second issue for a value that it cannot
find.

To make room, remove the label from the issue of a value only when no user runs a release that does
not know the value. The service lists its label alone, so it does not find an issue with no label.
The next report of that value opens a second issue, and the maintainers read a value that they
closed. Users of an older release send the value until they update. So a set and a value have one
issue only while the issue has the label.

The label belongs to the service. The weekly contract run has its own label, `palette-drift`, so its
issues do not count toward the 1000.

## The locks and the threads

The service has two locks. The first guards the limits and the hashes of the reports. It guards
memory only, and no call to the tracker runs under it, so a request that a check refuses never waits
for a merge. The second guards the tracker. One merge holds it from the read of the issues to the
last write. A request that passed checks 1 to 4 waits for it, except a repeat: check 5 answers a
repeat before the wait.

The service answers before it writes, so a slow tracker cannot make a sender time out. Strata waits
3 seconds for an answer. The read of the issues is the only call that comes before the answer.

Each request has a thread that lives until its report is written, which is after its answer. So the
limit of Cloud Run does not bound the threads. The service handles 32 requests at a time and answers
503 with `Retry-After` to a request over that number.

Cloud Run must keep the CPU after the answer. The deploy sets `--no-cpu-throttling`. Without it,
Cloud Run can stop the write that follows the answer.

## The issue of a value

A set and a value have one issue. Its first line is a marker:

```text
<!-- strata-report: values ProfileType later-type -->
```

The service writes the set and the value in the marker with the escape of an address, so a value
cannot end the comment. The second line holds the state of the issue, in hex. The table in the
body shows the same state: the lines, the reports, the releases of strata, and the routes. Each
list holds 20 items at most. The state of an older service can hold the lists `palette` and
`refusals`. The service reads such a state and ignores those lists.

- A later report of the value changes the body of the issue. It writes no comment, so it sends no
  notification.
- A closed issue gets the new count too, and stays closed. Users of an older release send the
  value until they update.
- The issue has the label `unknown-value` and the milestone `Backlog`. The weekly contract run has
  the label `palette-drift` and the same milestone. A test makes sure that the milestone is the
  same text in the service and in the script of the run.
- A person writes notes in a comment. The next report writes the body again.

The lab and the users can both see a new value. Then the weekly contract run opens its issue, and
the service opens another. This is correct, because the two issues answer two questions: what
the lab sees, and what users see.

- The run writes one issue for a fact and a set, with all the new values of the set. It closes the
  issue when the values are gone.
- The service writes one issue for a value, with the counts of the reports of users.

The two writers share no label and no marker. So the issues of the one never count toward the limit
of the other.

A maintainer proves the value in the lab, then follows
[What to do with a new value](./facts.md#what-to-do-with-a-new-value).

## What the service keeps

The service keeps no address and no report. Its memory holds these things, each with a salt that
the process makes when it starts:

- A hash of the address of each sender, for one hour. An IPv6 address counts by its /64 network.
- A hash of the key and the content of each report that it took, for one day.

The service drops an old hash of either kind when a request comes, one time in a minute at most. A
request that a check refuses also drops it. So a hash can stay a minute after its hour or its day,
and longer when no request comes.

It writes the hashes nowhere, and a restart of the service forgets them. The tracker keeps the
issues.

The request log of Cloud Run holds the address of the sender, so the deploy excludes that log from
Cloud Logging. The log of the container holds the start of the service and the failures of the
tracker, and no address.

## The settings of the service

Cloud Run sets the variables of the service. Only the token is required.

| Variable | Default | What it sets |
| --- | --- | --- |
| `GITHUB_TOKEN` | none, required | The token that writes issues: see [The token](#the-token). |
| `GITHUB_REPOSITORY` | the repository of the root `Cargo.toml` | The repository that gets the issues. The deploy sets no value. |
| `GITHUB_API` | `https://api.github.com` | The address of the API. A test points it at a stand-in. |
| `REPORT_LABEL` | `unknown-value` | The label of each issue, and the only label that the service lists. |
| `REPORT_MILESTONE` | `Backlog` | The milestone of a new issue, when the repository has it open. An empty value is no milestone. |
| `TRUST_FORWARDED_FOR` | not set | `1` reads the sender from the last entry of `X-Forwarded-For`. The deploy sets it, because the front end of Cloud Run adds that entry. |
| `PORT` | `8080` | The port. Cloud Run sets it. |

Without `TRUST_FORWARDED_FOR=1`, the service uses the address of the connection. A client can send
`X-Forwarded-For` to a service that no proxy fronts. Then each request has a sender of its own, and
the limit of the rate does not hold.

## The token

The service writes issues with a token of GitHub. The token has the permission Issues: read and
write on `como-technologies/strata`, and no other. It lives in Secret Manager, in the secret
`strata-report-token` of the project of the service. Cloud Run gives it to the service as
`GITHUB_TOKEN` when the service starts. The token is never in the repository, in GitHub, in an image,
or in a log. The `Debug` text of the settings and of the tracker prints `<redacted>` for it.

The account `strata-report` reads the secret, and no other account does. See
[Make the two accounts](#make-the-two-accounts).

## Run the tests

```bash
cargo test -p strata-report-endpoint     # the rules, the merge, and the service against a stand-in of GitHub
cargo test --test integration report_deploy   # the deploy script, against a stand-in gcloud
```

The tests of the service start the real service and a stand-in of GitHub on this machine. They post
with the same function that strata uses, so one test covers the send and the endpoint. No test
calls GitHub or Google. No test posts to the endpoint that the release names: see
[The tests send no report](./build-test.md#the-tests-send-no-report).

## Deploy the endpoint

The lead deploys the endpoint after the merge, on its host, after `gcloud auth login`. A session does
not deploy it, because a session has no account of Google.

1. Make sure that the project has the APIs `run`, `cloudbuild`, `artifactregistry`,
   `secretmanager`, and `iam`, and the secret `strata-report-token`.
2. Make the two accounts, with the commands of [Make the two accounts](#make-the-two-accounts). A
   person who owns the project does this one time.
3. Set `STRATA_REPORT_PROJECT` to the id of the project. The script has no default project. With
   no project, it stops with exit 2. Run `just report-deploy --dry-run`, and read the commands.
4. Move the issues of an older service to the label of this one, one time.
   See [Move the issues to the label of the service](#move-the-issues-to-the-label-of-the-service).
   Run `just report-deploy`. The script builds the image on Cloud Build as `strata-report-build`.
   Then it deploys the service `strata-report`, with one instance, 512 MiB of memory, and the CPU
   on after the answer. Then it prints the address, or stops with a message that names a missing
   account.
5. Run `just live-report-send ADDRESS` with the address that the script printed. The script sends
   two reports, waits for the issue, and checks that one issue holds both. It also posts one report
   twice with one key, and checks that the issue counts it one time. Close the issue with a comment
   after you read it.
6. Know three points of Cloud Run that a test cannot show. A live run showed two of them:
   - **The last entry of `X-Forwarded-For` is the address of the client.** Valid reports with
     another forged first entry each count as one sender. The service refuses them at the limit
     with 429. A control with no forged header also gets 429. A bad body spends no limit, so the
     check needs valid reports.
   - **IPv6.** The service publishes IPv6 addresses, so a sender can reach it on IPv6. The rule of
     the /64 network has an effect. The lab machine has no IPv6 route, and `curl -6` exits 7. So no
     live request shows the rule, and the tests of `network_of` show it. A failed `curl -6` shows
     a fault of the route of the machine, not a fault of the service.
   - **A slow body.** 12 connections, above the 8 requests that the service takes at a time, each
     sent a body one byte in a second. A quick request beside them took 0.20 s. So a slow body
     holds up nothing that a sender sees. No run shows where the front end buffers.
7. Name the address in the release: set the constant `ENDPOINT` in
   `src/closed_set/unknown/send.rs` to the address that the script printed. The release names the
   endpoint now, so a user sets no address. A new address needs a new release.

The deploy sets `--no-cpu-throttling` and `--memory 512Mi` together. Cloud Run refuses a service
that keeps the CPU on after the answer and has less than 512 MiB of memory. The error is `Total
memory < 512 Mi is not supported with cpu always allocated (unthrottled)`. A test of the deploy
fails when the memory is less than 512 MiB with `--no-cpu-throttling`.

The deploy script reads no secret and grants no role. `--project ID` replaces
`STRATA_REPORT_PROJECT`, and `--region NAME` replaces the region `us-central1`. To remove the
service, run
`gcloud run services delete strata-report --project ID --region NAME`.

### Move the issues to the label of the service

An older service wrote its issues with the label `palette-drift`, which the weekly contract run
uses too. The service lists the label `unknown-value` now. Move the issues of the older service
before the first deploy. If you do not, the new service does not find them, and it opens a second
issue for each value.

1. Make the label. When the label is missing, GitHub makes it with the first issue and gives it no
   description:

   ```bash
   gh label create unknown-value --repo como-technologies/strata \
     --description "strata: a user reports a value that strata does not know"
   ```

2. Move each issue whose first line is the marker of the service. The issues of the contract run
   have another marker, so they stay:

   ```bash
   gh issue list --repo como-technologies/strata --label palette-drift --state all --limit 1000 \
     --json number,body \
     | jq -r '.[] | select((.body // "") | startswith("<!-- strata-report: values ")) | .number' \
     | xargs -n 1 gh issue edit --repo como-technologies/strata \
         --add-label unknown-value --remove-label palette-drift
   ```

The state of an issue keeps its count, so the next report of the value adds to it.

An issue of a value that this release writes in another form stays as it is. Release 0.27.0 sent a
value with a space, a value with a character that is not printable ASCII, and a long value that
ends with `…`. This release sends a `?` for each such character, and it ends a long value with
`...`. So its marker is another marker. The next report of such a value opens a new issue, and the
first issue gets no more counts. Close it after you read it.

### Make the two accounts

The service and the build each run as an account of their own. The script does not use the default
compute account of the project. That account holds `roles/compute.admin` on the project. A fault in
the public service can give that power to a sender. Each other program that runs as that account
can read the token too, when the account has access to the secret.

| Account | Runs | Role | On |
| --- | --- | --- | --- |
| `strata-report` | The service | `roles/secretmanager.secretAccessor` | The secret `strata-report-token` |
| `strata-report-build` | The build | `roles/storage.objectViewer` | The bucket `PROJECT_cloudbuild` |
| `strata-report-build` | The build | `roles/logging.logWriter` | The project |
| `strata-report-build` | The build | `roles/artifactregistry.writer` | The repository `strata` |

The default compute account gets no role and no access to the secret.

The bucket `PROJECT_cloudbuild` holds the source that `gcloud builds submit` uploads. The first build
of a project makes it. A role needs the bucket and the repository, so the commands make them first.
A create command that says the repository or the bucket exists is harmless. The build writes its log
to Cloud Logging only, because a build with its own account must say where its log goes.

The person who runs the commands must own the project. An owner can act as both accounts
(`roles/iam.serviceAccountUser`), so `just report-deploy` can start the build and the service with
them.

```bash
project=${STRATA_REPORT_PROJECT:?the id of the project}
region=us-central1
service_account=strata-report@$project.iam.gserviceaccount.com
build_account=strata-report-build@$project.iam.gserviceaccount.com

gcloud iam service-accounts create strata-report --project "$project" \
  --display-name "The service of the report endpoint"
gcloud iam service-accounts create strata-report-build --project "$project" \
  --display-name "The build of the report endpoint"
gcloud artifacts repositories create strata --project "$project" --location "$region" \
  --repository-format docker --description "images of strata"
gcloud storage buckets create "gs://${project}_cloudbuild" --project "$project" \
  --location "$region"
gcloud secrets add-iam-policy-binding strata-report-token --project "$project" \
  --member "serviceAccount:$service_account" --role roles/secretmanager.secretAccessor
gcloud storage buckets add-iam-policy-binding "gs://${project}_cloudbuild" \
  --member "serviceAccount:$build_account" --role roles/storage.objectViewer
gcloud projects add-iam-policy-binding "$project" \
  --member "serviceAccount:$build_account" --role roles/logging.logWriter --condition=None
gcloud artifacts repositories add-iam-policy-binding strata \
  --project "$project" --location "$region" \
  --member "serviceAccount:$build_account" --role roles/artifactregistry.writer
```

Then make sure that the secret lists `strata-report` and no other account:

```bash
gcloud secrets get-iam-policy strata-report-token --project "$project"
```

If the list shows the default compute account, remove its role from the secret. Replace
`PROJECT_NUMBER` with the number of the project:

```bash
gcloud secrets remove-iam-policy-binding strata-report-token --project "$project" \
  --member "serviceAccount:PROJECT_NUMBER-compute@developer.gserviceaccount.com" \
  --role roles/secretmanager.secretAccessor
```
