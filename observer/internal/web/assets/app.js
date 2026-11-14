"use strict";
/* The dashboard of the observer. It reads the routes of the API, draws each screen with h``, and
   writes the page in one place: put(). Each value goes through e(), so a name from a record or from
   Palette shows as text and never as markup.

   The page has five scripts, and they share one scope. index.html loads them in this order: app.js,
   frame.js, screens.js, pack.js, and routes.js. app.js holds the template, the words of the API, the
   state, the helpers, the splash, and the question to Palette. frame.js holds the frame and the cards,
   screens.js holds the screens, and pack.js holds the comparison of a pack. routes.js holds the routes
   and the events, and it starts the page. So it loads last. */

/* ---- Escape and template ---- */
const ESCAPES = {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"};
const e = s => String(s).replace(/[&<>"']/g, c => ESCAPES[c]);
// Safe is markup that the code wrote itself. h`` makes it, and raw() marks a fixed entity.
class Safe { constructor(s) { this.s = s; } }
const raw = s => new Safe(s);
const part = v => v instanceof Safe ? v.s : Array.isArray(v) ? v.map(part).join("") : v == null || v === false ? "" : e(v);
const h = (strings, ...vals) => new Safe(strings.reduce((out, s, i) => out + s + (i < vals.length ? part(vals[i]) : ""), ""));
const ARROW = raw(" &rarr; ");
const DASH = raw("&ndash;");

/* ---- The words of the API ---- */
const REFRESH_MS = 30000;
// The period of the refresh, as the page says it to a person.
const REFRESH_WORDS = `each ${REFRESH_MS / 1000} seconds`;
const PAGE = {OK: "ok", FIRST_LOAD: "first-load", NO_RECORDS: "no-records", STALE: "stale", LOST: "lost"};
const BASELINE = {APPROVED: "approved", FIRST_SEEN: "first-seen", UNKNOWN: "unknown"};
const COPY = {SAME: "same", SAME_CONTENT: "same-content", DIFFERS: "differs", NOT_ON_CLUSTER: "not-on-this-cluster", NOT_READ: "not-read"};
// A record of a profile version that the cluster does not run takes the word of its cluster copy as its status.
const STATUS = {DRIFT: "drift", NOT_ON_CLUSTER: COPY.NOT_ON_CLUSTER, NEWER: "newer", NOT_CHECKED: "not-checked", OK: "ok", NO_RECORD: "no-record"};
const VERIFY = {UNCHANGED: "unchanged", DRIFTED: "drifted", UNVERIFIED: "unverified", ERROR: "error"};
// A copy that strata read matches the record, also by content, or it differs.
const COPY_KEPT = [COPY.SAME, COPY.SAME_CONTENT];
const COPY_READ = [...COPY_KEPT, COPY.DIFFERS];
const KIND = {PUBLISHED: "published-changed", OVERWRITTEN: "overwritten", RENAMED: "renamed", GONE: "gone", CLUSTER: "cluster-differs", IMAGE_ID: "image-id-changed", IMAGE_MOVED: "image-moved", IMAGE_SPLIT: "image-split"};
// The findings of the pods. They set the status of no profile and no pack.
const IMAGE_KINDS = [KIND.IMAGE_MOVED, KIND.IMAGE_SPLIT];
const SIDE = {PUBLISHED: "published", CLUSTER: "cluster", NEWER: "newer", NEWEST: "newest"};
// The sides that come from the versions cycle: the move of a pack, and the move of its profile to the newest version.
const VERSION_SIDES = [SIDE.NEWER, SIDE.NEWEST];
const CAUSE = {NO_DIGEST: "no-upstream-digest", OLD_COPY: "older-registry-copy", NO_RECORD: "no-record", READ_ERROR: "read-error", PIN_ERROR: "pin-error", PODS_NOT_READ: "pods-not-read"};
const EVENT = {DRIFT: "drift", NEWER: "newer", NOT_CHECKED: "not-checked", INFO: "info"};
// The words of /v1/images: whether the observer reads pods, the status of an image, how it matched a pack, and the status of one digest.
const PODS = {OFF: "off", NOT_READ: "not-read", READ: "read"};
const IMAGE = {STEADY: "steady", MOVED: "moved", SPLIT: "split", NOT_CHECKED: "not-checked"};
const MATCH = {EXACT: "exact", NAME_AND_TAG: "name-and-tag", NONE: "none"};
const DIGEST = {FIRST: "first-digest", ANOTHER: "another-digest", NO_REPO: "no-repo-digest", NOT_STARTED: "not-started"};
// The words of /v1/asks. A move that nobody asked about has no answer. A no-answer is a request that
// failed: it is no finding, and it raises no severity.
const VERDICT = {NO_ERROR: "no-error", REFUSES: "refuses", NO_ANSWER: "no-answer"};
// The words of /v1/alerts. A rule has a level and a kind of news. Its state follows its subjects: a mute
// shows Muted while it holds, and a mute that ended shows Inactive.
const ALERT_STATE = {FIRING: "firing", PENDING: "pending", INACTIVE: "inactive", MUTED: "muted"};
const ALERT_BADGE = {
  [ALERT_STATE.FIRING]: ["r", "Firing"], [ALERT_STATE.PENDING]: ["a", "Pending"], [ALERT_STATE.INACTIVE]: ["g", "Inactive"], [ALERT_STATE.MUTED]: ["y", "Muted"],
};
const ALERT_LEVEL = {cluster: "Cluster", profile: "Profile", pack: "Pack"};
const ALERT_NEWS = {drift: "Drift", image: "Image split or moved", newer: "Newer version", lost: "Lost contact"};
const ALERT_SEND = {NOW: "now", DIGEST: "digest"};
// The period of a digest that the page calls weekly. The API writes it in hours.
const WEEK = "168h";
// What the page shows before the first read of /v1/alerts, and when the observer sends no alert.
const NO_ALERTS = {set: false, channels: [], digestEvery: "", waiting: 0, rules: [], sample: ""};
// What a row of the table of newer versions says for an answer.
const ASK_ROW = {[VERDICT.NO_ERROR]: "No error", [VERDICT.REFUSES]: "Refuses", [VERDICT.NO_ANSWER]: "No answer"};
const ASK_FAILED = "The observer could not ask now. Try again later.";
// What a click asks, and what an answer of no error covers. Strata sends Palette the packs of the profile, with the newest version of
// this pack and its default values in the place of the pin. Palette checks them against each cluster that runs the profile. The
// request changes nothing.
const ASK_WHAT = "Asks Palette whether it would take this move on the clusters that run this profile. It changes nothing.";
const ASK_COVERS = "Palette checked the profile with the new version and its default values against each cluster that runs the profile. It did not check your own values of the pack.";
// The two layouts of a diff, and the note of a last line that has no end of line.
const VIEWS = [["unified", "Unified"], ["split", "Split"]];
const NO_EOL = "\\ No newline at end of file";
// The tone of a splash sets the look of its mark and its animation. The size is the place: the page, a card, or a line.
// SAME is the mark of one comparison that strata read and found the same: it looks as CALM does. CALM speaks for the whole page, so a
// finding anywhere takes it away; SAME speaks for one place, and stays.
const TONE = {CALM: "calm", SAME: "same", BUILD: "build", OFF: "off", WARN: "warn", LOST: "lost"};
const SIZE = {PAGE: "lg", CARD: "md", LINE: "sm"};
// The geometry of the mark in assets/logo/strata-mark-dark.svg, one path for each slab, from the top one down.
// app.css gives each slab its color, because the policy of the page refuses an inline style.
const SLABS = [
  "M3.65 30.51Q3.1 29.8 3.97 29.56L74.23 10.24Q75.1 10 75.58 10.76L83.02 22.44Q83.5 23.2 82.61 23.36L9.39 36.54Q8.5 36.7 7.95 35.99Z",
  "M3.65 47.91Q3.1 47.2 3.99 47.04L75.11 34.46Q76 34.3 76.86 34.57L82.64 36.43Q83.5 36.7 84.02 37.43L90.48 46.47Q91 47.2 90.1 47.28L9.4 54.02Q8.5 54.1 7.95 53.39Z",
  "M3.56 65.07Q3.1 64.3 4 64.22L75.7 57.78Q76.6 57.7 77.49 57.86L84.11 59.04Q85 59.2 85.63 59.84L95.47 69.96Q96.1 70.6 95.2 70.61L8.5 71.79Q7.6 71.8 7.14 71.03Z",
  "M2.94 81.58Q2.5 80.8 3.4 80.79L76.6 79.61Q77.5 79.6 78.39 79.73L87.11 80.97Q88 81.1 88.65 81.73L96.35 89.17Q97 89.8 96.1 89.8L8.5 89.8Q7.6 89.8 7.16 89.02Z",
];
// The commands that the page gives for a next step. The namespace is the one of deploy/observer.
const LOG_COMMAND = "kubectl -n strata-observer logs deploy/strata-observer --tail=20";
const SEED_LOG_COMMAND = "kubectl -n strata-observer logs -l app.kubernetes.io/name=strata-observer-seed --tail=20";
// The page was reached on a local port of a port-forward, often not 8080 because that port is in use. The command keeps that port.
const portForwardCommand = () => `kubectl -n strata-observer port-forward deploy/strata-observer ${location.port || "80"}:8080`;
const RECORDS_COMMAND = "scripts/ci/observer-records.sh --revision \"$GIT_SHA\" releases/*.json | kubectl apply --server-side -f -";
// The next step of a choice that the install makes. The page names the section of the book, and no file of the repository: a
// reader of the page needs not know how strata keeps its manifests.
const PODS_NEXT = "Turn on the read of the pods when you install the observer: with the chart, set readPods to true and upgrade. The book of strata tells how, in The observer: The pods of the cluster.";
const ALERTS_NEXT = "Turn on the alerts when you install the observer. The book of strata tells how, in The observer: Alerts.";
const CAN_LIST_PODS_COMMAND = "kubectl auth can-i list pods --all-namespaces --as=system:serviceaccount:strata-observer:strata-observer";
// The records are in four Secrets, and the page does not know which one holds a record. A key is the name and the version
// of the profile, as scripts/ci/observer-records.sh writes it.
const RECORD_SECRETS = [0, 1, 2, 3];
const RECORD_FILE = "record.json";

// The color class and the word of a status, a kind of news, and an impact level.
const BADGE = {
  [STATUS.DRIFT]: ["r", "Drift"], [STATUS.NEWER]: ["b", "Newer"], [STATUS.NOT_CHECKED]: ["y", "Not checked"],
  [STATUS.OK]: ["g", "OK"], [STATUS.NO_RECORD]: ["y", "No record"], [STATUS.NOT_ON_CLUSTER]: ["y", "Not on this cluster"],
};
const EVENT_BADGE = {
  [EVENT.DRIFT]: ["r", "Drift"], [EVENT.NEWER]: ["b", "Newer"], [EVENT.NOT_CHECKED]: ["y", "Not checked"], [EVENT.INFO]: ["y", "Info"],
};
// The badge of the copy of this cluster. The row of the card and the head of the version card both read this table.
const COPY_BADGE = {
  [COPY.SAME]: BADGE[STATUS.OK], [COPY.SAME_CONTENT]: ["g", "Same content"], [COPY.DIFFERS]: BADGE[STATUS.DRIFT],
  [COPY.NOT_ON_CLUSTER]: BADGE[STATUS.NOT_ON_CLUSTER], [COPY.NOT_READ]: ["y", "Not read"],
};
const IMPACT = {
  "repave": ["a", "Repave"], "may-repave": ["a", "May repave"], "cluster-wide": ["b", "Cluster-wide"],
  "in-place": ["y", "In place"], "sync": ["y", "Sync"], "none": ["y", "None"],
};
const SEVERITY = [STATUS.DRIFT, STATUS.NEWER, STATUS.NOT_CHECKED, STATUS.NO_RECORD, STATUS.OK, STATUS.NOT_ON_CLUSTER];
// The words of the five change flags: when each one lights, and what that means. They come from the table in
// docs/src/reference/impact.md, and this table is their one copy on the page.
const FLAG_WORDS = {
  IDENTITY: {when: "the type, tag, version, pack uid, or registry uid of the pack changes", means: "The release points at another pack build."},
  UPSTREAM: {when: "the digest on the pack reference changes", means: "The content of the pack changed under the same version."},
  AUTHORED: {when: "the values, the attached manifests, or the params change", means: "The configuration of the release changed."},
  IMAGES: {when: "the image list that the values declare, pack.content.images, changes", means: "An image was added, removed, or moved to another tag."},
  CHARTS: {when: "the chart list that the values declare, pack.content.charts, changes", means: "A chart was added, removed, or moved to another version."},
};
const FLAGS = Object.keys(FLAG_WORDS);
const FILTERS = [["all", "All"], [STATUS.DRIFT, "Drift"], [STATUS.NEWER, "Newer"], [STATUS.NOT_CHECKED, "Not checked"], [STATUS.OK, "OK"]];
const OPS = {added: "+", changed: "~", removed: "-"};
const PINNED = {deprecated: "deprecated", disabled: "disabled", deleted: "deleted", absent: "no longer listed"};
// The badge of an image, and of one digest of an image. A digest that is not a repo digest, and a pod
// that never started, read Not checked and never OK.
const IMAGE_BADGE = {
  [IMAGE.STEADY]: ["g", "First digest"], [IMAGE.MOVED]: ["a", "Image moved"], [IMAGE.SPLIT]: ["r", "Two digests"], [IMAGE.NOT_CHECKED]: ["y", "Not checked"],
};
const DIGEST_BADGE = {
  [DIGEST.FIRST]: ["g", "First digest"], [DIGEST.ANOTHER]: ["r", "Another digest"], [DIGEST.NO_REPO]: ["y", "Not checked"], [DIGEST.NOT_STARTED]: ["y", "Not checked"],
};
const DIGEST_WHY = {[DIGEST.NO_REPO]: "No repo digest", [DIGEST.NOT_STARTED]: "Not started"};
// A finding of the pods has the badge of its image, and not the badge of a drift.
const IMAGE_FINDING_BADGE = {[KIND.IMAGE_MOVED]: IMAGE_BADGE[IMAGE.MOVED], [KIND.IMAGE_SPLIT]: IMAGE_BADGE[IMAGE.SPLIT]};
const MATCH_WORDS = {[MATCH.EXACT]: "Exact", [MATCH.NAME_AND_TAG]: "Matched by name and tag", [MATCH.NONE]: "No pack"};

// What a finding says, by kind. Title is the heading, and line is the sentence for a profile.
const FINDING = {
  [KIND.PUBLISHED]: {title: "Published version changed after its record", line: "changed in the published version."},
  [KIND.OVERWRITTEN]: {title: "Another version replaced the recorded version", line: "Another version replaced the recorded version in place."},
  [KIND.RENAMED]: {title: "The recorded version has another name", line: "The recorded version has another name now."},
  [KIND.GONE]: {title: "Palette no longer holds the recorded version", line: "Palette no longer holds the recorded version."},
  [KIND.CLUSTER]: {title: "This cluster runs other content than its record", line: "changed on this cluster."},
  [KIND.IMAGE_ID]: {title: "A pack names another machine image", line: "changed the machine image on this cluster."},
  [KIND.IMAGE_MOVED]: {title: "A tag runs another digest than the first one seen", line: "ran another digest in pods than the first one seen."},
  [KIND.IMAGE_SPLIT]: {title: "One tag runs with two digests", line: "ran with two digests in pods at one time."},
};
// The kinds whose line starts with the names of the packs.
const NAMES_PACKS = [KIND.PUBLISHED, KIND.CLUSTER, KIND.IMAGE_ID, ...IMAGE_KINDS];
const CAUSE_TEXT = {
  [CAUSE.NO_DIGEST]: "Palette gives no upstream digest for this registry kind, so a change of the archive does not show.",
  [CAUSE.OLD_COPY]: "Current against an older copy of the registry. A newer version can be missing here.",
  [CAUSE.NO_RECORD]: "No record holds this profile version, so strata cannot say that anything changed. Your pipeline must publish its record: a check cannot add one.",
  [CAUSE.READ_ERROR]: "Strata could not read the record or its answer.",
  [CAUSE.PIN_ERROR]: "Strata could not look for a newer version of this pack, or could not compare it with the newest version.",
  [CAUSE.PODS_NOT_READ]: "The observer could not read the pods, so the list of images is old or empty.",
};
// What a profile says in the list for a cause, with the names of its packs.
// NO_RECORD_FIRST_SEEN is the cause of a profile version with no record while the records are first-seen: the observer records it
// itself at its next content check.
const NO_RECORD_FIRST_SEEN = "No record holds this profile version yet. The records are first-seen, so the observer records it at its next content check.";
const CAUSE_LINE = {
  [CAUSE.NO_DIGEST]: (names) => `${names} has no upstream digest.`,
  [CAUSE.OLD_COPY]: (names, registry) => `${names} is current against an older copy of ${registry}.`,
  [CAUSE.NO_RECORD]: () => "No record.",
  [CAUSE.READ_ERROR]: () => "Strata could not read the record.",
  [CAUSE.PIN_ERROR]: (names) => `Strata could not check ${names} for newer versions.`,
};
// wasRefused says that a server refused the key or the role: HTTP 401 or 403.
const wasRefused = status => status === 401 || status === 403;
const ERROR_TEXT = {
  api: status => wasRefused(status) ? `HTTP ${status}, the API key was refused` : `HTTP ${status}, Palette gave an error`,
  transport: () => "the link to Palette failed",
  "not-json": () => "Palette sent an answer that is not JSON",
  timeout: () => "Palette did not answer in time",
  "kube-api": status => wasRefused(status) ? `HTTP ${status}, the role of the observer was refused` : `HTTP ${status}, the API server gave an error`,
  "kube-transport": () => "the link to the API server failed",
  "kube-config": () => "the observer could not read its token or its CA file",
};
// The read of the pods says "the API server" where a call to Palette says "Palette".
const POD_ERROR_TEXT = {
  "not-json": () => "the API server sent an answer that is not JSON",
  timeout: () => "the API server did not answer in time",
};

/* ---- State ---- */
// D holds the last answers of the routes. S holds what the person did on the page: view is the layout of
// each diff, asking holds the moves with a question to Palette on its way, and askFailed holds the moves
// for which the observer could not ask.
let D = null;
const S = {filter: "all", q: "", qs: {}, sort: {}, tabs: {}, side: {}, open: {}, diffs: {}, view: "unified", asking: {}, askFailed: {}, now: {}, nowWait: {}, nowFailed: {}, offline: false, loadedAt: null};

async function getJSON(url) {
  const r = await fetch(url, {cache: "no-store"});
  if (!r.ok) throw new Error(url + " " + r.status);
  return r.json();
}

// load reads the seven summary routes, the list of answers of Palette, and the rules of the alerts. The
// detail route of a pack is read when a screen needs it. A list of answers or of rules that cannot be read
// keeps the last one, and it never puts the page offline.
async function load() {
  const [cluster, findings, profiles, newer, notChecked, events, images, asks, alerts] = await Promise.all([
    ...["/v1/cluster", "/v1/findings", "/v1/profiles", "/v1/newer", "/v1/notchecked", "/v1/events", "/v1/images"].map(getJSON),
    getJSON("/v1/asks").catch(() => null), getJSON("/v1/alerts").catch(() => null)]);
  if (D && (D.cluster.content.checkedAt !== cluster.content.checkedAt || D.cluster.versions.readAt !== cluster.versions.readAt)) {
    S.diffs = {};
  }
  const answers = asks && Array.isArray(asks.asks) ? asks.asks : D ? D.asks : [];
  const rules = alerts && Array.isArray(alerts.rules) ? alerts : D ? D.alerts : NO_ALERTS;
  D = {cluster, findings: findings.findings, profiles: profiles.profiles, newer: newer.newer, notChecked: notChecked.notChecked, limits: notChecked.limits, events: events.events, images, asks: answers, alerts: rules};
  S.loadedAt = new Date().toISOString();
  // A click "Check now" waits until a read shows its cycle: one that runs, one that ended, or no sign of it for a minute.
  for (const name of Object.keys(CYCLES)) {
    const p = S.now[name], c = cluster[name];
    if (p && (c.running || c[CYCLES[name].at] !== p.last || Date.now() - p.sent > NOW_WAIT_MS)) delete S.now[name];
  }
}

async function refresh() {
  try {
    await load();
    S.offline = false;
  } catch (err) {
    S.offline = true;
  }
  render();
}

// answering is true while the page renders because a detail request ended. The render shows the answer
// of that request and starts nothing new.
let answering = false;

// diffOf gives the state of one detail request, and starts the request the first time. The key holds
// the clock of the cycle that the side comes from, so a new cycle asks again. A request that failed is
// not kept: a 503 is a normal answer while a cycle holds the slot. So the next render that the person or
// the clock causes asks again. A render that an answer causes does not, or two requests that fail would
// ask each other again without end.
function diffOf(subject, p, side) {
  const stamp = VERSION_SIDES.includes(side) ? D.cluster.versions.readAt : D.cluster.content.checkedAt;
  const key = [subject, p.key, side, stamp].join("|");
  if (!answering && S.diffs[key] && S.diffs[key].state === "error") delete S.diffs[key];
  if (!S.diffs[key]) {
    S.diffs[key] = {state: "loading"};
    const diffs = S.diffs;
    fetchDiff(subject, p, side).then(r => {
      diffs[key] = r;
      answering = true;
      try { render(); } finally { answering = false; }
    });
  }
  return S.diffs[key];
}

async function fetchDiff(subject, p, side) {
  const url = `/v1/packs/${enc(subject)}/${enc(p.layer)}/${enc(p.name)}/diff?side=${side}`;
  try {
    const r = await fetch(url, {cache: "no-store"});
    if (r.status === 404) return {state: "none"};
    if (!r.ok) return {state: "error", status: r.status};
    return {state: "ok", data: await r.json()};
  } catch (err) {
    return {state: "error", status: 0};
  }
}

/* ---- Small helpers ---- */
const enc = s => encodeURIComponent(s).replace(/%40/g, "@");
// shq quotes a value for a shell, because a name from Palette is text of a person. A value of safe characters stays as it is, so
// that a normal command reads easily. Any other value goes in single quotes, and each ' in it becomes '\''. Each builder of a
// command quotes the values it places in the command, and no other code puts a value in a command.
const SAFE_ARGUMENT = /^[A-Za-z0-9_@%+:,./-]+$/;
const shq = s => SAFE_ARGUMENT.test(s) ? String(s) : `'${String(s).replace(/'/g, `'\\''`)}'`;
const subjectOf = p => `${p.name}@${p.version}`;
// at writes a name and a version in the form of a ref, as strata does: name@version. move writes a move as two refs, as the head of
// strata compare does. A column with the head Version or Move shows the bare version, because the name has its own column.
const at = (name, version) => `${name}@${version}`;
// ref draws the same ref on the page: the name, then @version in a softer face, so the eye finds the name first. at stays for plain
// text, such as an option of a select, a command, and a tip.
const ref = (name, version) => h`${name}<span class="ver">@${version}</span>`;
// subjectParts splits a subject name@version at its last "@", or gives null for a subject with no "@" after its first letter.
const subjectParts = subject => { const i = subject.lastIndexOf("@"); return i > 0 ? [subject.slice(0, i), subject.slice(i + 1)] : null; };
const subjectRef = subject => { const parts = subjectParts(subject); return parts ? ref(...parts) : subject; };
const move = (name, from, to) => h`${ref(name, from)}${ARROW}${ref(name, to)}`;
const profileHash = subject => `#p-${enc(subject)}`;
const packHash = (subject, p) => `#k-${enc(subject)}/${enc(p.layer)}/${enc(p.name)}`;
const imageHash = ref => `#i-${enc(ref)}`;
const plural = (n, one, many) => n === 1 ? one : many;
// hex12 cuts a root to the first 12 digits. digest shows an upstream digest in the form sha256:12 digits.
const hex12 = s => s ? s.slice(0, 12) : "";
const digest = s => { const m = /^(?:sha256:)?([0-9a-f]{12})[0-9a-f]*$/.exec(s); return m ? "sha256:" + m[1] : s; };
const when = iso => iso ? Date.parse(iso) : NaN;
const clock = iso => Number.isNaN(when(iso)) ? "" : new Date(when(iso)).toISOString().slice(11, 16) + "Z";
const day = iso => Number.isNaN(when(iso)) ? "" : new Date(when(iso)).toISOString().slice(0, 10);
const ago = iso => {
  if (Number.isNaN(when(iso))) return "";
  const s = Math.max(0, Math.floor((Date.now() - when(iso)) / 1000));
  const d = Math.floor(s / 86400), hh = Math.floor(s % 86400 / 3600), mm = Math.floor(s % 3600 / 60);
  return d ? `${d}d` : hh ? (mm ? `${hh}h ${mm}m` : `${hh}h`) : s < 60 ? "under 1m" : `${mm}m`;
};
// neverChecked says that no content check has ended well, as at the first start or after a restart in an
// outage. The counts of the page are then 0 because nothing was read, and not because nothing is wrong.
const neverChecked = () => !D.cluster.content.checkedAt;
const hasChecks = () => D.cluster.page !== PAGE.FIRST_LOAD && D.cluster.page !== PAGE.NO_RECORDS && !neverChecked();
const num = n => hasChecks() ? n : "-";
const known = n => neverChecked() ? "-" : n;
// podsRead says that the observer holds a read of the pods. A count of images is "-" before that, as num() is for a check.
// podsOff says that the observer does not read the pods, because the install has no --read-pods.
const podsOff = () => D.images.pods === PODS.OFF;
// versionsRead says that a read of the versions ended well since the observer started. Before it, a count of newer versions means nothing.
const versionsRead = () => !!D.cluster.versions.readAt;
const podsRead = () => D.images.pods === PODS.READ;
const imageNum = n => podsRead() ? n : "-";
const asCount = st => st === STATUS.NO_RECORD ? STATUS.NOT_CHECKED : st;
const packName = key => key.slice(key.indexOf("/") + 1);
const profileOf = subject => D.profiles.find(p => subjectOf(p) === subject);
const impactOf = i => IMPACT[i] || ["y", i];

/* ---- Sort and filter ----
   The head of a column with a key sorts its table. A click sorts up, a second click sorts down, and a third gives back the order of the
   page. A sort keeps the order of the page among equal rows. A version sorts in the order of strata: see versionCmp. */
const natural = (a, b) => String(a).localeCompare(String(b), undefined, {numeric: true, sensitivity: "base"});

/* versionCmp orders two versions as src/reference/version.rs does, and that file states each rule. The page cannot ask strata to
   order the newest versions of other packs, so it keeps this copy. The file testdata/versions.json holds the cases, and two tests,
   one of strata and one of this page, sort them. A change of one order fails the test of the other.
   1. A version reads as its three numbers, and the numbers decide first. A text that does not parse gives its first three numbers,
      or the one or two numbers at its start. One "v" in front does not count. A number above the largest u64 holds no version.
   2. For one set of numbers, text before the numbers is the lowest, in its natural order. Then the forms go up: one or two numbers
      and a tail, a tail after a dot after the three numbers, a pre-release, the version, and a fourth number.
   3. Then the tails decide, part by part, and then a pre-release after "-" is above no pre-release.
   4. A text with no version sorts below each version, in the natural order of its runs. The empty text is the lowest. */
const DIGITS = /^\d+$/;
// NUMBER_AND_TEXT reads a part as a number, the letters right after it, and a number after the letters: "4", "4rc", or "4rc1".
const NUMBER_AND_TEXT = /^(\d+)([A-Za-z]*)(\d*)$/;
const IDENTIFIER = /^[0-9A-Za-z-]+$/;
const U64_MAX = "18446744073709551615";
// The forms of a version around its three numbers, from the lowest to the highest for one set of numbers.
const FORM = {SHORT: 0, TEXT: 1, PARSED: 2, MORE: 3};
const byText = (x, y) => x < y ? -1 : x > y ? 1 : 0;
// byValue compares two runs of digits by their value, of any length.
const byValue = (x, y) => { const a = x.replace(/^0+/, ""), b = y.replace(/^0+/, ""); return a.length - b.length || byText(a, b); };
const fits = n => byValue(n, U64_MAX) <= 0;
const noV = v => v.startsWith("v") ? v.slice(1) : v;
// byRuns compares runs of digits by value and other runs as text. When all pairs agree, the text with more runs is higher.
function byRuns(a, b) {
  const x = a.match(/\d+|\D+/g) || [], y = b.match(/\d+|\D+/g) || [];
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = DIGITS.test(x[i]) && DIGITS.test(y[i]) ? byValue(x[i], y[i]) : byText(x[i], y[i]);
    if (d) return d;
  }
  return x.length - y.length;
}
// byIds compares two lists of identifiers from the left. A number is below a text, and the longer list is higher.
function byIds(a, b) {
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    const an = DIGITS.test(a[i]), bn = DIGITS.test(b[i]);
    const d = an && bn ? byValue(a[i], b[i]) : an ? -1 : bn ? 1 : byText(a[i], b[i]);
    if (d) return d;
  }
  return a.length - b.length;
}
// byPart compares two parts of a tail. A number is above a text, and a number with letters right after it is below the number.
function byPart(a, b) {
  const x = NUMBER_AND_TEXT.exec(a), y = NUMBER_AND_TEXT.exec(b);
  if (!x || !y) return x ? 1 : y ? -1 : byText(a, b);
  return byValue(x[1], y[1]) || (x[2] === "") - (y[2] === "") || byText(x[2], y[2]) ||
    (x[3] !== "") - (y[3] !== "") || byValue(x[3], y[3]);
}
// byTails compares two tails part by part, at their dots. When one tail is the start of the other, the longer tail is higher.
function byTails(a, b) {
  const x = a.split("."), y = b.split(".");
  for (let i = 0; i < Math.min(x.length, y.length); i++) { const d = byPart(x[i], y[i]); if (d) return d; }
  return x.length - y.length;
}
// splitVersion gives the text before the pre-release, the pre-release, and the build data, or null for a part that is missing. The
// build data starts at the first "+", and the pre-release at the first "-" before it.
function splitVersion(v) {
  const plus = v.indexOf("+"), head = plus < 0 ? v : v.slice(0, plus), build = plus < 0 ? null : v.slice(plus + 1);
  const dash = head.indexOf("-");
  return dash < 0 ? [head, null, build] : [head.slice(0, dash), head.slice(dash + 1), build];
}
// splitPre gives the text of a tail and the identifiers of its pre-release. The build data does not count.
const splitPre = v => { const [tail, pre] = splitVersion(v); return [tail, pre === null ? [] : pre.split(".")]; };
const versionOf = (numbers, count, form, attached, pre, tail) => ({before: "", numbers, count, form, attached, pre, tail});
// parsedVersion reads a version as Palette parses it, or gives null.
function parsedVersion(v) {
  const [core, pre, build] = splitVersion(noV(v)), n = core.split(".");
  const ids = s => s.split(".").every(id => IDENTIFIER.test(id));
  if (n.length > 3 || !n.every(x => DIGITS.test(x) && fits(x))) return null;
  if ((build !== null && !ids(build)) || (pre !== null && !ids(pre))) return null;
  return versionOf([n[0], n[1] || "0", n[2] || "0"], n.length, FORM.PARSED, false, pre === null ? [] : pre.split("."), "");
}
// threeNumbers reads the first three numbers of a text and the form around them, or gives null.
function threeNumbers(v) {
  const t = /\d+\.\d+\.\d+/.exec(v);
  if (!t) return null;
  const before = v.slice(0, t.index), end = t.index + t[0].length;
  if (before !== "" && before !== "v") { const after = readVersion(v.slice(t.index)); return after && {...after, before}; }
  const numbers = t[0].split(".");
  if (!numbers.every(fits)) return null;
  const [rest, pre] = splitPre(v.slice(end)), tail = rest.startsWith(".") ? rest.slice(1) : null;
  // a number with text right after it counts as a fourth number only in the last part
  const dot = tail === null ? -1 : tail.indexOf(".");
  const fourth = tail !== null && (dot < 0 ? NUMBER_AND_TEXT.test(tail) : DIGITS.test(tail.slice(0, dot)));
  // text right after the third number: that number and the text are one part, so the version has two numbers and that tail
  if (tail === null && rest !== "") {
    return versionOf(numbers, 2, FORM.SHORT, false, pre, v.slice(v.slice(0, end).lastIndexOf(".") + 1, end + rest.length));
  }
  return versionOf(numbers, 3, fourth ? FORM.MORE : FORM.TEXT, false, pre, tail === null ? rest : tail);
}
// shortVersion reads the one or two numbers at the start of a text and the tail after them, or gives null.
function shortVersion(v) {
  let [rest, pre] = splitPre(noV(v)), count = 0;
  const numbers = ["0", "0", "0"];
  for (;;) {
    const dot = rest.indexOf("."), part = dot < 0 ? rest : rest.slice(0, dot);
    if (!DIGITS.test(part)) break;
    if (count === 3 || !fits(part)) return null;
    numbers[count++] = part;
    rest = dot < 0 ? "" : rest.slice(dot + 1);
  }
  // the number of a last part with text right after it, as in "1.2rc1"
  const attached = rest.includes(".") ? null : NUMBER_AND_TEXT.exec(rest);
  if (attached) {
    if (count === 3 || !fits(attached[1])) return null;
    numbers[count++] = attached[1];
  }
  return count ? versionOf(numbers, count, FORM.SHORT, !!attached, pre, rest) : null;
}
// readVersion gives the version that strata reads in a text, or null for a text with no version.
const readVersion = v => parsedVersion(v) || threeNumbers(v) || shortVersion(v);
// byForm orders two versions of one form, after their numbers and the text before them.
function byForm(x, y) {
  const tails = () => byTails(x.tail, y.tail) || (x.pre.length > 0) - (y.pre.length > 0) || byIds(x.pre, y.pre);
  switch (x.form) {
    case FORM.SHORT: return x.count - y.count || y.attached - x.attached || tails();
    case FORM.PARSED: return (x.pre.length === 0) - (y.pre.length === 0) || byIds(x.pre, y.pre);
    default: return tails();
  }
}
function versionCmp(a, b) {
  const s = String(a), t = String(b), x = readVersion(s), y = readVersion(t);
  if (!x || !y) return x ? 1 : y ? -1 : byRuns(s, t);
  for (let i = 0; i < 3; i++) { const d = byValue(x.numbers[i], y.numbers[i]); if (d) return d; }
  return (x.before === "") - (y.before === "") || byRuns(x.before, y.before) || x.form - y.form || byForm(x, y);
}
const ORDERS = {num: (a, b) => a - b, text: natural, version: versionCmp};
const rank = list => v => { const i = list.indexOf(v); return i < 0 ? list.length : i; };

// sortBy sorts a list by the column that S.sort holds for the table. keys maps a column to [the value of a row, the kind of order].
function sortBy(table, list, keys) {
  const s = S.sort[table], k = s && keys[s.col];
  if (!k) return list;
  const [get, kind] = k;
  return list.map((x, i) => [x, i]).sort(([a, i], [b, j]) => ORDERS[kind](get(a), get(b)) * s.dir || i - j).map(([x]) => x);
}

// sortTh draws the head of a column that sorts its table.
// SORT_ICON is two triangles, one up and one down. A column that sorts its table lights the triangle of its order. The icon is an
// inline drawing: the page loads nothing from outside the cluster.
const SORT_ICON = h`<svg class="sorti" viewBox="0 0 10 14" width="10" height="14" aria-hidden="true"><path class="u" d="M5 1 9.5 6h-9z"/><path class="d" d="M5 13 .5 8h9z"/></svg>`;
function sortTh(table, col, label, cls = "") {
  const s = S.sort[table], on = !!s && s.col === col, dir = on ? (s.dir > 0 ? "asc" : "desc") : "";
  return h`<th class="${cls}" aria-sort="${on ? (s.dir > 0 ? "ascending" : "descending") : "none"}"><button type="button" class="sort${on ? ` on ${dir}` : ""}" data-sort="${table}" data-col="${col}">${label}${SORT_ICON}</button></th>`;
}

// filterBox is a search field for one list. The page keeps what a person types in S.qs, and matches is the test of a row.
const filterBox = (key, words) => h`<input class="search" id="q-${key}" data-q="${key}" type="search" placeholder="${words}" aria-label="${words}" value="${S.qs[key] || ""}">`;
const matches = (key, ...texts) => { const q = (S.qs[key] || "").toLowerCase(); return !q || texts.some(t => String(t).toLowerCase().includes(q)); };

const badge = (cls, text) => h`<span class="badge ${cls}">${text}</span>`;
// newChip is the chip "new" after a version of the last 7 days, or "" when the version is older.
const newChip = on => on ? h` <span class="badge fresh">new</span>` : "";
// badgeOf draws the badge of a word from one of the tables above. A word that the table lacks shows as it is.
const badgeOf = (table, key) => { const [c, t] = table[key] || ["y", key]; return badge(c, t); };
const statusBadge = st => badgeOf(BADGE, st);
const impactBadge = i => badgeOf(IMPACT, i);
const imageBadge = st => badgeOf(IMAGE_BADGE, st);
const digestBadge = st => badgeOf(DIGEST_BADGE, st);
const lab = (k, v) => h`<span class="lab"><i>${k}=</i>"${v}"</span>`;
const hbar = x => h`<span class="hbar" role="img" aria-label="Status of each pack">${x.packs.map(p => h`<i class="${(BADGE[p.status] || ["y"])[0]}" title="${p.name}: ${(BADGE[p.status] || [0, p.status])[1]}"></i>`)}</span>`;
const mut = text => h`<span class="mut">${text}</span>`;
// The images of a pack in its record are the list that its values declare, pack.content.images. Many packs declare none, and they
// still run images: an OS pack, Kubernetes, a chart that names its images inside. So 0 reads as none listed, and not as no image.
const IMAGES_LISTED = "The images that the values of the pack list in pack.content.images. A pack that lists none can still run images.";
const imagesListed = n => n ? String(n) : h`<span class="mut" title="${IMAGES_LISTED}">none listed</span>`;
const openAttr = k => S.open[k] ? raw(" open") : "";
const root = r => r ? h`<span title="${r}">${hex12(r)}</span>` : "";

// newerRowOf finds the row of /v1/newer that a pack of a profile belongs to.
const newerRowOf = (subject, p) => D.newer.find(r => r.profiles.includes(subject) && r.name === p.name && r.version === p.version && r.registry === p.registry);
// copyStatusOf gives the status of the cluster copy of the profile x. It is undefined when the summary holds no cluster copy.
const copyStatusOf = x => x.clusterCopy && x.clusterCopy.status;
// notRunHere says that the cluster does not run the profile version x: its cluster copy is not on this cluster. The summary uses the
// same rule, and gives such a record no newer version. A record with a finding has the status drift, and the rule holds for it too.
const notRunHere = x => copyStatusOf(x) === COPY.NOT_ON_CLUSTER;
// NEWER_STATE is what strata knows of a newer version of a pack. Strata does not look for a newer version of a pack of a profile version
// that the cluster does not run, so such a pack is NOT_RUN, and never CURRENT. Each screen reads the state from newerStateOf. The state
// is no word of the API, so its values are numbers.
const NEWER_STATE = {NEWER: 0, NOT_CHECKED: 1, NOT_READ: 2, CURRENT: 3, NOT_RUN: 4};
function newerStateOf(x, p) {
  if (notRunHere(x)) return NEWER_STATE.NOT_RUN;
  if (p.newer) return NEWER_STATE.NEWER;
  if (p.status === STATUS.NOT_CHECKED) return NEWER_STATE.NOT_CHECKED;
  return versionsRead() ? NEWER_STATE.CURRENT : NEWER_STATE.NOT_READ;
}
// movePackOf finds a pack in the move of a profile to its newest version. It is undefined when the move does not change the pack, and
// when strata did not compare the record with that version.
const movePackOf = (x, key) => x.newerVersion && x.newerVersion.move ? x.newerVersion.move.packs.find(q => q.key === key) : undefined;
// packOfRow finds the pack of a profile that a row of /v1/newer moves. A profile that is not known gives null.
const packOfRow = (profile, r) => profile ? profile.packs.find(q => q.name === r.name && q.version === r.version) : null;
// causeNames says that a row of Not checked with the cause names the profile, and the pack too when a key is given.
const causeNames = (cause, subject, key) => D.notChecked.some(r => r.cause === cause && r.profiles.includes(subject) && (key === undefined || r.packs.includes(key)));
// moveUnread says that strata could not compare the two versions of a pack. The row has no counts, and Not checked names the pack.
const moveUnread = (subject, p) => !!p && causeNames(CAUSE.PIN_ERROR, subject, p.key);
const impactCell = (subject, p, row) => moveUnread(subject, p) ? badge("y", "Not read") : impactBadge(row.impact);
// readErrorOf says that the last read of a profile failed in part. The summary then holds the last good answer of that part, with no flag
// for it, and the cause read-error names the profile. So a side with no change reads as not checked, and never as unchanged or the same.
const readErrorOf = subject => causeNames(CAUSE.READ_ERROR, subject);
// CARRIED_WORDS tells why a side with no change reads as not checked.
const CARRIED_WORDS = "The last read failed, so strata cannot say that nothing changed.";
// NOT_ON_CLUSTER_WORDS tell a record of a profile version that the cluster does not run, on its row and on its card of the cluster copy.
const NOT_ON_CLUSTER_WORDS = "This cluster does not run this version.";
// orNone shows a blank value as the word none.
const orNone = v => v || "none";
// clusterName names the cluster of the page.
const clusterName = () => D.cluster.cluster.name || D.cluster.cluster.uid || "this cluster";

/* ---- The splash ----
   One component draws each place that has nothing to show, and each state that the page tells: splash(tone, title,
   body, extra, size). The headline can have some fun, and the body states the fact in plain words. It gives the next
   step when one exists. It never reads calmer than the check allows: calmOk() is the one gate of a calm tone. A
   finding always shows its data and never a splash, and a lost or stale page that holds findings stays a bar over them. */

// mark draws the mark of the logo. The tone is a class, and app.css sets the fill and the animation of each slab.
const mark = (tone, px) => h`<svg class="slabs tone-${tone}" viewBox="0 0 100 100" width="${px}" height="${px}" aria-hidden="true">${SLABS.map((d, i) => h`<path class="s s${i}" d="${d}"/>`)}</svg>`;

function splash(tone, title, body, extra = "", size = SIZE.PAGE) {
  return h`<div class="splash ${size} tone-${tone}">${mark(tone, size === SIZE.PAGE ? 112 : 72)}<h3>${title}</h3><p>${body}</p>${extra}</div>`;
}

// inline draws a small mark and one line, for a table or a list that has no row.
const inline = (tone, text, extra = "") => h`<div class="inl tone-${tone}">${mark(tone, 26)}<span>${text}</span>${extra}</div>`;
// cmd draws a command with a button that copies it.
const cmd = text => h`<div class="cmd"><code>${text}</code><button type="button" class="copy" data-copy="${text}">Copy</button></div>`;
// facts draws the values under a splash. A value with the class ok is a good one.
const facts = list => h`<div class="facts">${list.map(([value, label, cls]) => h`<div><b class="${cls || ""}">${value}</b><span>${label}</span></div>`)}</div>`;
// next draws the next step, with a command or a button after it.
const next = (words, rest = "") => h`<div class="next"><b>Next</b> ${words}</div>${rest}`;
const tryButton = what => h`<div class="actions"><button type="button" class="btn" data-retry="${what}">Try now</button></div>`;

// limits lists what keeps a check from speaking for the whole cluster: a page that is lost or stale, a read that
// failed, and a cause in Not checked. A finding shows its own rows.
function limits() {
  const c = D.cluster, n = c.counts, out = [];
  if (c.page === PAGE.LOST) out.push("Palette is out of reach, so the counts are from the last good check.");
  if (c.page === PAGE.STALE) out.push("Checks stopped, so the counts are from the last good check.");
  if (c.page === PAGE.OK && c.content.failedRuns > 0) out.push(`The last ${c.content.failedRuns} content ${plural(c.content.failedRuns, "check", "checks")} failed.`);
  if (c.versions.failedRuns > 0) out.push(`The last ${c.versions.failedRuns} ${plural(c.versions.failedRuns, "read", "reads")} of the versions failed.`);
  if (D.images.error) out.push(`The last read of the pods failed: ${podsErrorWords(D.images.error)}.`);
  if (n.notChecked > 0) out.push(`${n.notChecked} ${plural(n.notChecked, "cause", "causes")} in Not checked limit what a check says.`);
  return out;
}

// trusted says that the checks can speak: the page holds data, it is not lost or stale, and it reaches the observer.
const trusted = () => hasChecks() && D.cluster.page === PAGE.OK && !S.offline;
// calmOk says that a calm tone is true. A calm tone needs a good check: the page holds data, it is not lost, stale, or offline, no
// finding is open, no read failed, and no cause is in Not checked. Words that speak of versions need a read of the versions too:
// calmOk(true). Each calm word asks this, and no other test.
const calmOk = (aboutVersions = false) => trusted() && D.cluster.counts.findings === 0 && D.findings.length === 0 && limits().length === 0 && (!aboutVersions || !!D.cluster.versions.readAt);
// settledTone gives the tone of a place that has nothing to show: calm when the check allows it, amber for a limit, else the outline.
const settledTone = () => calmOk() ? TONE.CALM : limits().length ? TONE.WARN : TONE.OFF;

// failedChecks tells the failed content checks and their error, for a splash of a page that never had a good check.
function failedChecks() {
  const k = D.cluster.content;
  if (!k.failedRuns) return "";
  return h`The last ${k.failedRuns} ${plural(k.failedRuns, "check", "checks")} failed${k.lastError ? h`: <b>${errorWords(k.lastError)}.</b>` : "."} `;
}

// lostNext gives the next step for the error that lost the page.
function lostNext(err) {
  if (err && err.kind === "api" && wasRefused(err.status)) return h`Check the key in the Secret <span class="m">strata-observer-key</span>, then read the log.`;
  return "Check that the observer can reach Palette, then read the log.";
}

// firstSeenCommand gives the command that records what the profiles of this cluster hold now. A cluster with no uid gets a placeholder.
const firstSeenCommand = () => `scripts/ci/observer-records.sh --first-seen ${D.cluster.cluster.uid ? shq(D.cluster.cluster.uid) : "<cluster uid>"} | kubectl apply --server-side -f -`;

// stateSplash gives the splash for a page that holds nothing to show yet, or null when the page holds data. A page that is
// lost or stale after a good check is not here: it keeps its data under a bar.
function stateSplash() {
  if (hasChecks()) return null;
  const c = D.cluster;
  switch (c.page) {
    case PAGE.NO_RECORDS:
      return splash(TONE.OFF, "Nothing to compare against yet", "A record is the copy of a profile that the observer checks against. Start with what the cluster runs now, and let your pipeline approve records later.",
        h`<p class="sub"><b>Start now.</b> An install with the chart runs a job that records what each profile of this cluster holds now. Its log says what it did.</p>${cmd(SEED_LOG_COMMAND)}<p class="sub">With no chart, this command records the same. No pipeline approved that content.</p>${cmd(firstSeenCommand())}<p class="sub"><b>From your pipeline.</b> Publish the records that a review approved.</p>${cmd(RECORDS_COMMAND)}`);
    case PAGE.LOST:
      return splash(TONE.LOST, "We lost the signal from Palette", h`${failedChecks()}No check has ended well, and no finding clears until a check proves it.`,
        next(lostNext(c.content.lastError), cmd(LOG_COMMAND)));
    case PAGE.STALE:
      return splash(TONE.WARN, "Checks stopped before one ended well", h`${failedChecks()}The content check runs every ${c.content.every}. No check has ended well, so this page has nothing to show yet.`,
        next("Read the log.", cmd(LOG_COMMAND)));
    default:
      return splash(TONE.BUILD, "Reading the layers", "The first check is running, and this page fills in when it ends. The observer reads each record, then asks Palette for the same version.");
  }
}

// withChecks gives the view of a screen that reads the checks. A page with nothing read shows the splash of its state, else the body.
function withChecks(nav, crumb, build) {
  return {nav, crumbs: crumb, body: h`${alertBars()}${stateSplash() || build()}`};
}

// noFindings is the splash of an empty list of findings. It reads calm only when the check allows it.
function noFindings() {
  const c = D.cluster, n = c.counts, when = `${ago(c.content.checkedAt)} ago`, lim = limits();
  if (calmOk()) {
    return splash(TONE.CALM, `All quiet on ${clusterName()}`, h`Nothing needs attention. <b>${n.packsInFull} of ${n.packs} packs</b> were checked in full, ${when}.`,
      facts(inOrder({
        findings: [n.profiles, plural(n.profiles, "profile matches its record", "profiles match their records")],
        newer: [c.versions.readAt ? n.newer : "-", plural(c.versions.readAt ? n.newer : 0, "newer version", "newer versions")],
        notchecked: [n.notChecked, "not checked"],
      })), SIZE.CARD);
  }
  const more = n.notChecked > 0 ? next(h`<a href="#notchecked">Read the causes in Not checked.</a>`) : "";
  return splash(settledTone(), "No finding", `${n.packsInFull} of ${n.packs} packs were checked in full, ${when}. ${lim.join(" ")}`, more, SIZE.CARD);
}

// noNewer is the splash of an empty list of newer versions. It says that each pack is current only for a read that ended well.
function noNewer(size) {
  const v = D.cluster.versions;
  if (!v.readAt) {
    if (v.failedRuns > 0) return splash(TONE.WARN, "No version was read yet", `The last ${v.failedRuns} ${plural(v.failedRuns, "read", "reads")} of the versions failed${v.lastError ? `: ${errorWords(v.lastError)}` : ""}.`, "", size);
    return splash(TONE.BUILD, "Reading the registries", "The first read of the versions has not ended. This page lists a newer version when it does.", "", size);
  }
  const read = `Read ${ago(v.readAt)} ago, every ${v.every}.`;
  if (calmOk(true)) return splash(TONE.CALM, "Nothing newer to dig up", `Every pack is current. The registries list nothing newer than what you pin. ${read}`, "", size);
  const lim = limits();
  return splash(settledTone(), "No newer version is listed", `${trusted() && !lim.length ? "Every pack is current." : "No newer version is listed for what was read."} ${read} ${lim.join(" ")}`, "", size);
}

// noCauses is the splash of an empty list of causes in Not checked.
function noCauses() {
  const more = h`<div class="next"><button type="button" class="lnk" data-jump="coverage">What a check does not cover</button></div>`;
  if (calmOk()) {
    return splash(TONE.CALM, "Every layer was read", "Each pack was checked in full. Strata read each record, and each answer came back. A check still does not cover the items below.", more, SIZE.CARD);
  }
  return splash(settledTone(), "Every pack was checked in full", `Each pack was checked in full. ${limits().join(" ")} A check still does not cover the items below.`, more, SIZE.CARD);
}

// offlineSplash is the screen of a page that cannot reach the observer and holds no data yet, as after a port-forward that ended. With
// data on the page, the same fault is a bar over that data.
const offlineSplash = () => splash(TONE.LOST, "This page lost its line to the observer",
  `The port-forward may have ended, or the pod may have restarted. This page asks again ${REFRESH_WORDS}.`,
  h`${cmd(portForwardCommand())}${tryButton("page")}`);

// missingSplash is the screen of a page that the data does not hold. w gives the title, the name, the reason, and the way back.
const missingSplash = w => splash(TONE.OFF, w.title, h`<span class="m">${w.name}</span> ${w.why}`, h`<div class="actions"><a class="btn" href="${w.home[0]}">${w.home[1]}</a></div>`);

/* ---- The question to Palette ----
   Only a click on a button sends a question: nothing asks on a timer, and no render asks. One answer is for
   one move: the move of a pack of a profile to the newest version. */
const askId = (subject, p) => `${subject}|${p.key}`;
// answerOf finds the answer of Palette for the move of a pack. A move that nobody asked about has none. The observer holds one
// answer for a pack of a profile, and it drops the answer when the move changes. The registry can gain a version after the last
// cycle. The answer is then for a newer version than the row, and it shows.
const answerOf = (subject, p) => D.asks.find(a => a.profile === subject && a.key === p.key);
const isAnswer = a => !!a && ["profile", "key", "newest", "verdict"].every(k => typeof a[k] === "string");
// verdictOf reads the verdict of an answer. A word that the page does not know reads as no answer, and the page claims nothing.
const verdictOf = a => a.verdict === VERDICT.NO_ERROR || a.verdict === VERDICT.REFUSES ? a.verdict : VERDICT.NO_ANSWER;

// askButton draws the button of a move. It is off while the question is on its way.
function askButton(cls, subject, p, label) {
  const asking = S.asking[askId(subject, p)];
  return h`<button class="${cls}" data-ask="${subject}" data-pack="${p.key}"${asking ? raw(" disabled") : ""}>${asking ? "Asking..." : label}</button>`;
}

// askCell is the cell "Palette" of a row of the table of newer versions.
function askCell(subject, p) {
  const a = answerOf(subject, p);
  const failed = S.askFailed[askId(subject, p)] ? h`<div class="small mut askmsg">${ASK_FAILED}</div>` : "";
  if (!a) return h`${askButton("lnk", subject, p, "Ask Palette")}${failed}`;
  const verdict = verdictOf(a);
  if (verdict === VERDICT.NO_ANSWER) return h`<span class="mut">${ASK_ROW[verdict]}</span> ${askButton("lnk", subject, p, "Ask again")}${failed}`;
  return h`<span class="fw">${ASK_ROW[verdict]}</span>`;
}

// refusalLines draws one error that Palette gives: the code, the message, and the clusters.
const refusalLines = r => h`<div class="mt3"><b>Palette refuses:</b> <span class="m">${r.code}</span></div>${r.message ? h`<div>${r.message}</div>` : ""}${r.clusters && r.clusters.length ? h`<div class="mut">on ${r.clusters.join(", ")}</div>` : ""}`;

// askBlock is the part of the card "Move to" that asks Palette, or that shows its answer.
function askBlock(subject, p) {
  const a = answerOf(subject, p);
  const failed = S.askFailed[askId(subject, p)] ? h`<div class="note a small mt9">${ASK_FAILED}</div>` : "";
  if (!a) return h`<div class="small mut">${ASK_WHAT}</div><div class="mt9">${askButton("btn", subject, p, "Ask Palette about this move")}</div>${failed}`;
  switch (verdictOf(a)) {
    case VERDICT.NO_ERROR:
      return h`<div class="note small"><b>Palette gave no error.</b> ${ASK_COVERS}</div>`;
    case VERDICT.REFUSES: {
      const list = a.refusals || [];
      return h`<div class="note a small">${list.length ? list.map(refusalLines) : h`<div><b>Palette refuses.</b></div>`}<div class="mt6">Palette would not take this move as the profile is now. Nothing on this cluster changed.</div></div>`;
    }
    default:
      return h`<div class="note small"><b>Palette gave no answer${a.status ? `: HTTP ${a.status}` : ""}.</b> This is not a finding.</div><div class="mt9">${askButton("btn", subject, p, "Ask again")}</div>${failed}`;
  }
}

// askPalette sends the question about one move. A click calls it, and it sends one request. A good
// answer goes into the list. A request that fails leaves a short note, and the page keeps no answer.
async function askPalette(subject, key) {
  const x = profileOf(subject), p = x ? x.packs.find(q => q.key === key) : null;
  const id = p ? askId(subject, p) : "";
  if (!p || S.asking[id]) return;
  S.asking[id] = true;
  delete S.askFailed[id];
  render();
  let answer = null;
  try {
    const r = await fetch(`/v1/packs/${enc(subject)}/${enc(p.layer)}/${enc(p.name)}/ask`, {method: "POST", cache: "no-store"});
    if (r.ok) answer = await r.json();
  } catch (err) {
    answer = null;
  }
  delete S.asking[id];
  if (isAnswer(answer)) {
    D.asks = [...D.asks.filter(a => !(a.profile === answer.profile && a.key === answer.key)), answer];
  } else {
    S.askFailed[id] = true;
  }
  render();
}

// errorWords tells a failed run in words. own holds the words of a read that has its own, as POD_ERROR_TEXT.
function errorWords(err, own = {}) {
  if (!err) return "";
  const f = own[err.kind] || ERROR_TEXT[err.kind];
  return f ? f(err.status) : "strata gave an error";
}
const podsErrorWords = err => errorWords(err, POD_ERROR_TEXT);
