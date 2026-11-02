# The endpoint of the report

The endpoint is a small service that takes the report of the values that strata does not know. A
user of strata sends the report only by choice: see
[Send the report by itself](../reference/report.md#send-the-report-by-itself). This page is for the
maintainers. It says what the service checks, what it writes, and how the lead deploys it.

The service is the package `strata-report-endpoint` in `report-endpoint/`. It runs on Cloud Run, in
the project of the lab, and it writes issues of the repository of strata. No release of strata
carries it.

## What a request goes through

The service checks each request in this order. A request that fails a check changes nothing.

| Order | Check | Answer when it fails |
| --- | --- | --- |
| 1 | The path is `/` and the method is `POST`. | 404 or 405 |
| 2 | The sender made 10 requests or fewer in the last hour. | 429, with `Retry-After` |
| 3 | The content type is JSON, and the body has 64 KiB or fewer. | 415, 413, or 400 |
| 4 | The body is `strata-report/1`, with no field outside the list. Each value is a short word. | 422 |
| 5 | The merge opens no more than 20 new issues in the last hour, for all senders. | 429, with `Retry-After` |
| 6 | The tracker takes the issues. | 502 |

Check 4 is the type `Report` of the library. The send of strata and the endpoint use the same type,
so the list of fields has one definition. A report has 1 to 100 entries. The check refuses a
route that is the path of a real request, and a value of more than 64 characters. Check 5 comes
after the reads and before the writes, so a report that the limit refuses writes nothing.

The answer is JSON: `{"ok":true,"opened":1,"updated":0}`, or `{"error":"..."}`. The message of an
error never repeats a value of the body. A failure of the tracker goes to standard error with no
address, and the sender gets a short sentence.

## The issue of a value

A set and a value have one issue. Its first line is a marker:

```text
<!-- strata-report: values ProfileType later-type -->
```

The service writes the set and the value in the marker with the escape of an address, so a value
cannot end the comment. The second line holds the state of the issue, in hex. The table in the
body shows the same state: the lines, the reports, the releases of strata and of Palette, the routes,
and the refusals. Each list holds 20 items at most.

- A later report of the value changes the body of the issue. It writes no comment, so it sends no
  notification.
- A closed issue gets the new count too, and stays closed. Users of an older release send the
  value until they update.
- The issue has the label `palette-drift` and the milestone `Backlog`. The weekly contract run uses
  both. The marker differs from the markers of that run, so the two never touch each other's issues.
- A person writes notes in a comment. The next report writes the body again.

A maintainer proves the value in the lab, then follows
[What to do with a new value](./facts.md#what-to-do-with-a-new-value).

## What the service keeps

The service keeps no address and no report. Its memory holds a hash of the address of each sender
for one hour, with a salt that the process makes when it starts. It writes the hash nowhere. The
tracker keeps the issues.

The request log of Cloud Run holds the address of the sender, so the deploy excludes that log from
Cloud Logging. The log of the container holds the start of the service and the failures of the
tracker, and no address.

## The settings of the service

Cloud Run sets the variables of the service. Only the token is required.

| Variable | Default | What it sets |
| --- | --- | --- |
| `GITHUB_TOKEN` | none, required | The token that writes issues: see [The token](#the-token). |
| `GITHUB_REPOSITORY` | `como-technologies/strata` | The repository that gets the issues. |
| `GITHUB_API` | `https://api.github.com` | The address of the API. A test points it at a stand-in. |
| `REPORT_LABEL` | `palette-drift` | The label of each issue. |
| `REPORT_MILESTONE` | `Backlog` | The milestone of a new issue, when the repository has it open. An empty value is no milestone. |
| `PORT` | `8080` | The port. Cloud Run sets it. |

The service reads the sender from the last entry of `X-Forwarded-For`, which the front end of
Cloud Run adds. A request without the header gets the address of its connection.

## The settings of the service

Cloud Run sets the variables of the service. Only the token is required.

| Variable | Default | What it sets |
| --- | --- | --- |
| `GITHUB_TOKEN` | none, required | The token that writes issues: see [The token](#the-token). |
| `GITHUB_REPOSITORY` | `como-technologies/strata` | The repository that gets the issues. |
| `GITHUB_API` | `https://api.github.com` | The address of the API. A test points it at a stand-in. |
| `REPORT_LABEL` | `palette-drift` | The label of each issue. |
| `REPORT_MILESTONE` | `Backlog` | The milestone of a new issue, when the repository has it open. An empty value is no milestone. |
| `PORT` | `8080` | The port. Cloud Run sets it. |

The service reads the sender from the last entry of `X-Forwarded-For`, which the front end of
Cloud Run adds. A request without the header gets the address of its connection.

## The token

The service writes issues with a token of GitHub. The token has the permission Issues: read and
write on `como-technologies/strata`, and no other. It lives in Secret Manager, in the secret
`strata-report-token` of the project `como-strata-test`. Cloud Run gives it to the service as
`GITHUB_TOKEN` when the service starts. The token is never in the repository, in GitHub, in an image,
or in a log. The `Debug` text of the settings and of the tracker prints `<redacted>` for it.

## Run the tests

```bash
cargo test -p strata-report-endpoint     # the rules, the merge, and the service against a stand-in of GitHub
cargo test --test integration report_deploy   # the deploy script, against a stand-in gcloud
```

The tests of the service start the real service and a stand-in of GitHub on this machine. They post
with the same function that strata uses, so the send and the endpoint are tested together. No test
calls GitHub or Google.

## Deploy the endpoint

The lead deploys the endpoint after the merge, on kadomony, with `gcloud` logged in. A session does
not deploy it, because a session has no account of Google.

1. Make sure that the project has the APIs `run`, `cloudbuild`, `artifactregistry`, and
   `secretmanager`, and the secret `strata-report-token`. The service account of Cloud Run needs
   the role `roles/secretmanager.secretAccessor` on the secret.
2. Run `just report-deploy --dry-run`, and read the commands.
3. Run `just report-deploy`. The script builds the image on Cloud Build, deploys the service
   `strata-report` with one instance, and prints the address.
4. Run `just live-report-send ADDRESS` with the address that the script printed. The script sends
   two reports and checks that one issue holds both. Close the issue with a comment after you read it.
5. Name the address in the release: set the constant `ENDPOINT` in
   `src/closed_set/unknown/send.rs`. Until then, a user must set `STRATA_REPORT_URL`.

The deploy script reads no secret. `just report-deploy --project ID --region NAME` replaces the
project and the region. To remove the service, run
`gcloud run services delete strata-report --project ID --region NAME`.
