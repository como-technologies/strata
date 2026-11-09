"use strict";
/* The dashboard of the observer. It reads the routes of the API, draws each screen with h``, and
   writes the page in one place: put(). Each value goes through e(), so a name from a record or from
   Palette shows as text and never as markup. */

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
const STATUS = {DRIFT: "drift", NEWER: "newer", NOT_CHECKED: "not-checked", OK: "ok", NO_RECORD: "no-record"};
const VERIFY = {UNCHANGED: "unchanged", DRIFTED: "drifted", UNVERIFIED: "unverified", ERROR: "error"};
const COPY = {SAME: "same", SAME_CONTENT: "same-content", DIFFERS: "differs", NOT_ON_CLUSTER: "not-on-this-cluster", NOT_READ: "not-read"};
// A copy that strata read matches the record, also by content, or it differs.
const COPY_KEPT = [COPY.SAME, COPY.SAME_CONTENT];
const COPY_READ = [...COPY_KEPT, COPY.DIFFERS];
const KIND = {PUBLISHED: "published-changed", OVERWRITTEN: "overwritten", RENAMED: "renamed", GONE: "gone", CLUSTER: "cluster-differs", IMAGE_ID: "image-id-changed", IMAGE_MOVED: "image-moved", IMAGE_SPLIT: "image-split"};
// The findings of the pods. They set the status of no profile and no pack.
const IMAGE_KINDS = [KIND.IMAGE_MOVED, KIND.IMAGE_SPLIT];
const SIDE = {PUBLISHED: "published", CLUSTER: "cluster", NEWER: "newer"};
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
const TONE = {CALM: "calm", BUILD: "build", OFF: "off", WARN: "warn", LOST: "lost"};
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
// The page was reached on a local port of a port-forward, often not 8080 because that port is in use. The command keeps that port.
const portForwardCommand = () => `kubectl -n strata-observer port-forward deploy/strata-observer ${location.port || "80"}:8080`;
const RECORDS_COMMAND = "scripts/ci/observer-records.sh --revision \"$GIT_SHA\" releases/*.json | kubectl apply --server-side -f -";
// The next step of a choice that the install makes. The page names the section of the book, and no file of the repository: a
// reader of the page needs not know how strata keeps its manifests.
const PODS_NEXT = "Turn on the read of the pods when you install the observer. The book of strata tells how, in The observer: The pods of the cluster.";
const ALERTS_NEXT = "Turn on the alerts when you install the observer. The book of strata tells how, in The observer: Alerts.";
const CAN_LIST_PODS_COMMAND = "kubectl auth can-i list pods --all-namespaces --as=system:serviceaccount:strata-observer:strata-observer";
// The records are in four Secrets, and the page does not know which one holds a record. A key is the name and the version
// of the profile, as scripts/ci/observer-records.sh writes it.
const RECORD_SECRETS = [0, 1, 2, 3];
const RECORD_FILE = "record.json";

// The color class and the word of a status, a kind of news, and an impact level.
const BADGE = {
  [STATUS.DRIFT]: ["r", "Drift"], [STATUS.NEWER]: ["b", "Newer"], [STATUS.NOT_CHECKED]: ["y", "Not checked"],
  [STATUS.OK]: ["g", "OK"], [STATUS.NO_RECORD]: ["y", "No record"],
};
const EVENT_BADGE = {
  [EVENT.DRIFT]: ["r", "Drift"], [EVENT.NEWER]: ["b", "Newer"], [EVENT.NOT_CHECKED]: ["y", "Not checked"], [EVENT.INFO]: ["y", "Info"],
};
// The badge of the copy of this cluster. The row of the card and the head of the version card both read this table.
const COPY_BADGE = {
  [COPY.SAME]: BADGE[STATUS.OK], [COPY.SAME_CONTENT]: ["g", "Same content"], [COPY.DIFFERS]: BADGE[STATUS.DRIFT],
  [COPY.NOT_ON_CLUSTER]: ["y", "Not on this cluster"], [COPY.NOT_READ]: ["y", "Not read"],
};
const IMPACT = {
  "repave": ["a", "Repave"], "may-repave": ["a", "May repave"], "cluster-wide": ["b", "Cluster-wide"],
  "in-place": ["y", "In place"], "sync": ["y", "Sync"], "none": ["y", "None"],
};
const SEVERITY = [STATUS.DRIFT, STATUS.NEWER, STATUS.NOT_CHECKED, STATUS.NO_RECORD, STATUS.OK];
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
  [CAUSE.NO_RECORD]: "No record holds this profile, so strata cannot say that anything changed.",
  [CAUSE.READ_ERROR]: "Strata could not read the record or its answer.",
  [CAUSE.PIN_ERROR]: "Strata could not look for a newer version of this pack, or could not compare it with the newest version.",
  [CAUSE.PODS_NOT_READ]: "The observer could not read the pods, so the list of images is old or empty.",
};
// What a profile says in the list for a cause, with the names of its packs.
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
const S = {filter: "all", q: "", qs: {}, sort: {}, tabs: {}, side: {}, open: {}, diffs: {}, view: "unified", asking: {}, askFailed: {}, offline: false, loadedAt: null};

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
  const stamp = side === SIDE.NEWER ? D.cluster.versions.readAt : D.cluster.content.checkedAt;
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
const subjectRef = subject => { const i = subject.lastIndexOf("@"); return i > 0 ? ref(subject.slice(0, i), subject.slice(i + 1)) : subject; };
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
const podsRead = () => D.images.pods === PODS.READ;
const imageNum = n => podsRead() ? n : "-";
const asCount = st => st === STATUS.NO_RECORD ? STATUS.NOT_CHECKED : st;
const packName = key => key.slice(key.indexOf("/") + 1);
const profileOf = subject => D.profiles.find(p => subjectOf(p) === subject);
const impactOf = i => IMPACT[i] || ["y", i];

/* ---- Sort and filter ----
   The head of a column with a key sorts its table. A click sorts up, a second click sorts down, and a third gives back the order of the
   page. A sort keeps the order of the page among equal rows. A version sorts by the precedence of SemVer 2.0.0 when both parse, so a
   pre-release sorts below its release; any other version sorts in natural order. */
const SEMVER = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;
const natural = (a, b) => String(a).localeCompare(String(b), undefined, {numeric: true, sensitivity: "base"});
function semverCmp(a, b) {
  const x = SEMVER.exec(a), y = SEMVER.exec(b);
  if (!x || !y) return natural(a, b);
  for (let i = 1; i <= 3; i++) if (Number(x[i]) !== Number(y[i])) return Number(x[i]) - Number(y[i]);
  if (!x[4] || !y[4]) return x[4] ? -1 : y[4] ? 1 : 0;
  const p = x[4].split("."), q = y[4].split(".");
  for (let i = 0; i < Math.max(p.length, q.length); i++) {
    if (p[i] === undefined) return -1;
    if (q[i] === undefined) return 1;
    const pn = /^\d+$/.test(p[i]), qn = /^\d+$/.test(q[i]);
    if (pn && qn && Number(p[i]) !== Number(q[i])) return Number(p[i]) - Number(q[i]);
    if (pn !== qn) return pn ? -1 : 1;
    if (!pn && p[i] !== q[i]) return p[i] < q[i] ? -1 : 1;
  }
  return 0;
}
const ORDERS = {num: (a, b) => a - b, text: natural, version: semverCmp};
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
// worstOf gives the status that wins in a list of statuses.
const worstOf = list => SEVERITY.find(s => list.includes(s)) || STATUS.OK;
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
      return splash(TONE.OFF, "Nothing to compare against yet", "A record is the copy your team approved. Your pipeline writes the records, and the observer checks against them.",
        h`${cmd(RECORDS_COMMAND)}<p class="sub">On a test cluster, this command records what each profile holds now. No person approved that content.</p>${cmd(firstSeenCommand())}`);
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
  const failed = S.askFailed[askId(subject, p)] ? h`<div class="small mut mt6">${ASK_FAILED}</div>` : "";
  if (!a) return h`<div class="small mut mt10">${ASK_WHAT}</div><div class="mt6">${askButton("btn", subject, p, "Ask Palette about this move")}${failed}</div>`;
  switch (verdictOf(a)) {
    case VERDICT.NO_ERROR:
      return h`<div class="note small mt10"><b>Palette gave no error.</b> ${ASK_COVERS}</div>`;
    case VERDICT.REFUSES: {
      const list = a.refusals || [];
      return h`<div class="note a small mt10">${list.length ? list.map(refusalLines) : h`<div><b>Palette refuses.</b></div>`}<div class="mt6">Palette would not take this move as the profile is now. Nothing on this cluster changed.</div></div>`;
    }
    default:
      return h`<div class="note small mt10"><b>Palette gave no answer${a.status ? `: HTTP ${a.status}` : ""}.</b> This is not a finding.</div><div class="mt6">${askButton("btn", subject, p, "Ask again")}${failed}</div>`;
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

/* ---- The frame: nav, top bar, alert bars ---- */
// alertsCount is the count of the nav item Alerts: the rules that fire, or "Not set" when the observer sends no alert.
function alertsCount() {
  const a = D.alerts;
  if (!a.set) return h`<span class="cnt">Not set</span>`;
  const firing = a.rules.filter(r => r.state === ALERT_STATE.FIRING).length;
  return h`<span class="cnt ${firing ? "r" : ""}">${firing}</span>`;
}

// imagesCount is the count of the nav item Images: "Off" when the observer does not read the pods, and "-" before the first read.
function imagesCount() {
  return h`<span class="cnt">${podsOff() ? "Off" : imageNum(D.images.counts.images)}</span>`;
}

// ORDER is the one order of the sections: the nav, the tiles of the overview, and the facts of a calm splash keep it.
const ORDER = ["findings", "images", "newer", "notchecked", "profiles"];
const inOrder = byKey => ORDER.filter(k => byKey[k]).map(k => byKey[k]);

function navLinks(cur) {
  const c = D.cluster, n = c.counts;
  const sections = {
    findings: ["findings", "Findings", h`<span class="cnt ${n.findings ? "r" : ""}">${num(n.findings)}</span>`],
    images: ["images", "Images", imagesCount()],
    newer: ["newer", "Newer versions", h`<span class="cnt">${num(n.newer)}</span>`],
    notchecked: ["notchecked", "Not checked", h`<span class="cnt">${num(n.notChecked)}</span>`],
    profiles: ["profiles", "Profiles", h`<span class="cnt">${known(n.profiles)}</span>`],
  };
  const items = [["overview", "Overview", ""], ...inOrder(sections), ["alerts", "Alerts", alertsCount()]];
  return items.map(([k, l, cnt]) => h`<a href="#${k}" class="${k === cur ? "on" : ""}"><span>${l}</span>${cnt}</a>`);
}

function clocks() {
  const c = D.cluster, content = c.content, versions = c.versions;
  const lost = c.page === PAGE.LOST, stale = c.page === PAGE.STALE;
  let a;
  if (lost) {
    a = h`<span class="chip" title="${content.checkedAt}"><span class="dot r"></span>Palette <b>lost</b>${content.checkedAt ? h` · content checked <b>${ago(content.checkedAt)} ago</b>` : ""}</span>`;
  } else if (!content.checkedAt) {
    a = h`<span class="chip"><span class="dot a"></span>Content <b>not checked yet</b> · every ${content.every}</span>`;
  } else {
    const dot = stale || content.failedRuns > 0 ? "a" : "";
    a = h`<span class="chip" title="${content.checkedAt}"><span class="dot ${dot}"></span>Content <b>${ago(content.checkedAt)} ago</b> · every ${content.every}</span>`;
  }
  if (stale || lost) return a;
  if (!versions.readAt) return [a, h`<span class="chip"><span class="dot a"></span>Versions <b>not read yet</b> · every ${versions.every}</span>`];
  const dot = versions.failedRuns > 0 ? "a" : "";
  return [a, h`<span class="chip" title="${versions.readAt}"><span class="dot ${dot}"></span>Versions <b>${ago(versions.readAt)} ago</b> · every ${versions.every}</span>`];
}

// recordsChip names the baseline in one place and one form: the revision that a pipeline approved, first-seen, or unknown. An
// amber dot marks a baseline that no pipeline approved.
function recordsChip(r) {
  if (r.baseline === BASELINE.FIRST_SEEN) return h`<span class="chip" title="No pipeline approved these records yet"><span class="dot a"></span>records: <b>first-seen</b></span>`;
  if (r.baseline === BASELINE.UNKNOWN) return h`<span class="chip" title="The observer holds no record yet"><span class="dot a"></span>records: <b>unknown</b></span>`;
  return h`<span class="chip">records: <b>${r.revision ? `rev ${r.revision}` : "approved"}</b></span>`;
}

function topBar(crumbs) {
  return h`<nav class="crumbs" aria-label="Where you are">${crumbs}</nav><div class="topr">${recordsChip(D.cluster.records)}${clocks()}</div>`;
}

const crumbs = (...a) => a.map((x, i) => i === a.length - 1 ? h`<b>${x}</b>` : h`${x} <span>/</span> `);

function alertBars() {
  const c = D.cluster, bars = [];
  const content = c.content;
  if (S.offline) {
    bars.push(h`<div class="alertbar a">The page cannot reach the observer. <span>It shows the data of ${clock(S.loadedAt)}. It tries again ${REFRESH_WORDS}.</span></div>`);
  }
  const counts = content.checkedAt ? h`Counts below are from ${clock(content.checkedAt)}` : h`No check has ended well`;
  const failed = content.failedRuns > 0 ? h`The last ${content.failedRuns} ${plural(content.failedRuns, "check", "checks")} failed${content.lastError ? h`: ${errorWords(content.lastError)}` : ""}. ` : "";
  // A page with nothing read tells its state in a splash. A page with a good check behind it keeps its data under a bar.
  if (hasChecks()) {
    switch (c.page) {
      case PAGE.LOST:
        bars.push(h`<div class="alertbar r">Lost contact with Palette. <span>${failed}${counts}, and no finding clears until a check proves it.</span></div>`);
        break;
      case PAGE.STALE:
        bars.push(h`<div class="alertbar a">Checks stopped. <span>${content.checkedAt ? h`Last content check ${clock(content.checkedAt)}, expected every ${content.every}. ` : ""}${failed}${counts}.</span></div>`);
        break;
    }
  }
  if (c.records.baseline === BASELINE.FIRST_SEEN) {
    const from = c.records.firstSeenFrom;
    bars.push(h`<div class="alertbar a">No person approved these records. <span>They come from the first sight of ${from ? h`the cluster <span class="m">${from}</span>` : "a cluster"}.</span></div>`);
  }
  const pods = D.images;
  if (pods.pods === PODS.READ && pods.error) {
    bars.push(h`<div class="alertbar a">The last read of the pods failed: ${podsErrorWords(pods.error)}. <span>The list shows the read of ${clock(pods.readAt)}.</span></div>`);
  }
  return bars;
}

/* ---- The cards ---- */
function tiles() {
  const c = D.cluster, n = c.counts;
  const T = (cls, label, value, sub, href) => h`<a class="tile ${cls}" href="#${href}"><div class="l">${label}</div><div class="v">${value}</div><div class="s">${sub}</div></a>`;
  const driftSub = n.drift === 0 ? (calmOk() ? "Nothing changed" : "None in what was checked") : n.drift === 1 ? driftSubject() : `${n.drift} profiles`;
  const fresh = D.newer.filter(r => r.new).length;
  const newerSub = n.newer === 0 ? (calmOk(true) ? `All ${n.packs} packs current` : "None listed") : `${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")} · ${fresh} this week`;
  const notCheckedSub = n.notChecked === 0 ? `${n.packsInFull} of ${n.packs} packs in full` : `${n.notChecked} ${plural(n.notChecked, "cause", "causes")} · ${n.packsInFull} of ${n.packs} packs in full`;
  // The tile "Image changes" is there only when the observer reads the pods. The other four fill the row.
  const images = () => { const [cls, value, sub] = imageTile(); return T(cls, "Image changes", value, sub, "images"); };
  return h`<section class="tiles" aria-label="Summary">${inOrder({
    findings: T(n.drift ? "r" : "g", "Drift", n.drift, driftSub, "findings"),
    images: podsOff() ? "" : images(),
    newer: T("b", "Newer versions", n.newer, newerSub, "newer"),
    notchecked: T("y", "Not checked", n.notChecked, notCheckedSub, "notchecked"),
    profiles: T("", "Profiles", n.profiles, `${n.packs} packs on this cluster`, "profiles"),
  })}</section>`;
}

// imageTile gives the color, the value, and the words of the tile "Image changes", for an observer that reads the pods. Before a
// read that ended, it claims nothing: the value is "-". The Drift tile does not count these changes. The tile is green only when the
// checks can speak and the last read of the pods worked.
function imageTile() {
  const o = D.images, n = o.counts;
  if (o.pods === PODS.NOT_READ) return ["y", "-", "Not read yet"];
  if (n.changed > 0) return ["a", n.changed, "Moved or split"];
  return [trusted() && !o.error ? "g" : "y", n.changed, `${n.images} ${plural(n.images, "image", "images")} in pods`];
}

// driftSubject names the profile that drifts, for the tile.
function driftSubject() {
  const x = D.profiles.find(p => p.status === STATUS.DRIFT);
  return x ? subjectOf(x) : "1 profile";
}

function findingTitle(f) { return (FINDING[f.kind] || {title: f.kind}).title; }

// findingHref gives the screen that a finding opens: the image page for a finding of the pods, else its pack or its profile.
function findingHref(f) {
  if (IMAGE_KINDS.includes(f.kind) && f.images && f.images[0]) return imageHash(f.images[0]);
  const p = profileOf(f.profile);
  const key = f.packs[0] && p ? p.packs.find(k => k.key === f.packs[0]) : null;
  return key ? packHash(f.profile, key) : profileHash(f.profile);
}

// findingBadge gives the badge of a finding. A finding of the pods is no drift of a record, so it has the badge of its image.
const findingBadge = f => IMAGE_KINDS.includes(f.kind) ? badgeOf(IMAGE_FINDING_BADGE, f.kind) : statusBadge(STATUS.DRIFT);

// imageLab is the label of an image reference. A reference is long, so it can wrap.
const imageLab = ref => h`<span class="lab wrap"><i>image=</i>"${ref}"</span>`;

// findingLabs names what a finding is about: the images for a finding of the pods, else the packs.
const findingLabs = f => IMAGE_KINDS.includes(f.kind) ? (f.images || []).map(imageLab) : f.packs.map(k => lab("pack", packName(k)));

function findingsCard() {
  const rows = D.findings;
  const body = rows.length ? h`<div class="tw"><table><tbody>${rows.map(f => {
    const href = findingHref(f);
    return h`<tr class="link" data-go="${href}"><td class="w90">${findingBadge(f)}${f.reBaselineNeeded ? h` <span class="badge y">re-baseline needed</span>` : ""}</td>
      <td><div class="fw"><a href="${href}">${findingTitle(f)}</a></div><div>${lab("profile", f.profile)}${findingLabs(f)}</div><div class="mut small narrow-show">${ago(f.firstSeen)}</div></td>
      <td class="n mut small w60 nw narrow-hide">${ago(f.firstSeen)}</td></tr>`;
  })}</tbody></table></div>` : noFindings();
  return h`<section class="card"><div class="chead"><h2>Needs attention <span class="sub">${known(rows.length)}</span></h2><a class="small" href="#findings">All findings</a></div>${body}</section>`;
}

// objectLinks names the objects of an event. A profile or an image that the page holds is a link to its page.
function objectLinks(object) {
  const parts = String(object).split(", ").map(o => {
    if (profileOf(o)) return h`<a href="${profileHash(o)}">${subjectRef(o)}</a>`;
    if (D.images.images.some(i => i.ref === o)) return h`<a href="${imageHash(o)}">${o}</a>`;
    return o;
  });
  return parts.flatMap((x, i) => i ? [", ", x] : [x]);
}

function eventRows(list) {
  return list.map(x => {
    return h`<tr><td>${badgeOf(EVENT_BADGE, x.type)}</td><td class="m narrow-hide">${x.reason}</td><td class="mut nw">${ago(x.time)}</td><td class="m narrow-hide">${objectLinks(x.object)}</td><td>${x.message}<div class="m mut narrow-show">${objectLinks(x.object)}</div></td></tr>`;
  });
}

const EVENTS_SHOWN = 8;

const EVENT_KEYS = {type: [x => x.type, "text"], reason: [x => x.reason, "text"], age: [x => -when(x.time), "num"], object: [x => x.object, "text"]};
const eventHead = table => h`<thead><tr>${sortTh(table, "type", "Type")}${sortTh(table, "reason", "Reason", "narrow-hide")}${sortTh(table, "age", "Age")}${sortTh(table, "object", "Object", "narrow-hide")}<th>Message</th></tr></thead>`;

function eventsCard() {
  const all = sortBy("et", D.events, EVENT_KEYS), first = all.slice(0, EVENTS_SHOWN), rest = all.slice(EVENTS_SHOWN);
  const more = rest.length ? h`<details data-open="events" ${openAttr("events")}><summary>${rest.length} more ${plural(rest.length, "event", "events")}</summary><div class="tw"><table>${eventHead("et")}<tbody>${eventRows(rest)}</tbody></table></div></details>` : "";
  const body = all.length ? h`<div class="tw"><table>${eventHead("et")}<tbody>${eventRows(first)}</tbody></table></div>${more}` : h`<div class="body">${inline(quietTone(), calmOk() ? "All quiet: no events in the last 7 days." : "No events in the last 7 days.")}</div>`;
  return h`<section class="card"><div class="chead"><h2>Recent events</h2><span class="small mut">Last 7 days</span></div>${body}</section>`;
}

function coverageCard() {
  const l = D.limits;
  const row = (title, sub, value) => h`<tr><td>${title}<div class="small mut">${sub}</div></td><td class="n">${value}</td></tr>`;
  const behindATag = podsRead() ? h`<a href="#images">Compared with the first digest seen in pods</a>` : "Strata compares the name and the tag.";
  return h`<section class="card" id="coverage"><div class="chead"><h2>What a check does not cover</h2></div><div class="tw"><table><tbody>
    ${row("Image behind a tag", behindATag, num(l.imagesNamedByATag))}
    ${row("Bytes of a chart", "The record holds the digest Palette lists", num(l.helmPacks))}
    ${row("Masked values", "An equal root does not show an equal secret", num(l.maskedPaths))}
    ${row("Edits with kubectl", "Strata compares what Palette holds", h`<span class="mut">n/a</span>`)}
  </tbody></table></div></section>`;
}

/* What a profile says in the list: the news that matters most first. */
function findingLine(f, x) {
  const w = FINDING[f.kind] || {line: f.kind};
  const names = f.packs.map(packName).join(", ");
  let text = NAMES_PACKS.includes(f.kind) ? (names ? `${names} ${w.line}` : w.title + ".") : w.line;
  if (f.kind === KIND.PUBLISHED) {
    const copy = x.clusterCopy && x.clusterCopy.status;
    if (f.alsoOnCluster) text += " This cluster has the edit too.";
    else if (COPY_KEPT.includes(copy)) text += " This cluster keeps its copy.";
  }
  return text;
}

// newsOf gives the news of a profile. A finding of the pods sets no status, so a profile that does not
// drift adds the line of the finding to its other news, and an OK profile gives only that line.
function newsOf(x) {
  const news = recordNews(x);
  if (x.status === STATUS.DRIFT) return news;
  const subject = subjectOf(x);
  const pods = D.findings.filter(f => f.profile === subject && IMAGE_KINDS.includes(f.kind)).map(f => findingLine(f, x));
  if (!pods.length) return news;
  return x.status === STATUS.OK ? pods.join(" ") : h`${news}<div>${pods.join(" ")}</div>`;
}

function recordNews(x) {
  const subject = subjectOf(x);
  const ms = x.packs.filter(p => p.newer);
  switch (x.status) {
    case STATUS.DRIFT: {
      const lines = D.findings.filter(f => f.profile === subject).map(f => findingLine(f, x));
      if (ms.length) lines.push(`Also ${ms.length} newer ${plural(ms.length, "version", "versions")}.`);
      return lines.length ? lines.join(" ") : "Drift.";
    }
    case STATUS.NEWER: {
      if (ms.length === 1) {
        const row = newerRowOf(subject, ms[0]);
        return h`${move(ms[0].name, ms[0].version, ms[0].newer)}${row && row.new ? h` <span class="badge fresh">new</span>` : ""}`;
      }
      const rows = D.newer.filter(r => r.profiles.includes(subject));
      const repave = rows.filter(r => r.impact === "repave").length, may = rows.filter(r => r.impact === "may-repave").length;
      return `${ms.length} of ${x.packs.length} packs have a newer version.${repave ? ` ${repave} repave.` : ""}${may ? ` ${may} can repave.` : ""}`;
    }
    case STATUS.NOT_CHECKED: return notCheckedLine(x);
    case STATUS.NO_RECORD: return mut("No record.");
    default: return mut("No change.");
  }
}

// notCheckedLine tells why a profile was not fully checked, from the rows of /v1/notchecked.
function notCheckedLine(x) {
  const subject = subjectOf(x);
  const lines = D.notChecked.filter(r => r.profiles.includes(subject)).map(r => {
    const names = x.packs.filter(p => r.packs.includes(p.key)).map(p => p.name).join(", ");
    const f = CAUSE_LINE[r.cause];
    return f ? f(names || x.name, r.registry || "the registry") : r.cause;
  });
  return lines.length ? lines.join(" ") : "Not fully checked.";
}

function profilesCard() {
  // A profile with a pack that has a newer version shows under "Newer", also when its status is a drift.
  const has = (x, k) => k === "all" || (k === STATUS.OK ? x.status === STATUS.OK : k === STATUS.NEWER ? x.packs.some(p => p.newer) : asCount(x.status) === k || x.packs.some(p => p.status === k));
  const list = [...D.profiles].sort((a, b) => SEVERITY.indexOf(a.status) - SEVERITY.indexOf(b.status));
  const cnt = {};
  FILTERS.forEach(([k]) => { cnt[k] = list.filter(x => has(x, k)).length; });
  const q = S.q.toLowerCase();
  const shown = sortBy("pt", list.filter(x => has(x, S.filter) && (!q || subjectOf(x).toLowerCase().includes(q))), {
    status: [x => SEVERITY.indexOf(x.status), "num"], profile: [x => x.name, "text"], type: [x => x.type, "text"], packs: [x => x.packs.length, "num"]});
  const rows = shown.map(x => {
    const subject = subjectOf(x);
    return h`<tr class="link" data-go="${profileHash(subject)}"><td class="w110">${statusBadge(x.status)}</td><td class="m nw"><a href="${profileHash(subject)}">${x.name}</a><span class="mut">@${x.version}</span></td><td class="mut narrow-hide">${x.type}</td><td class="narrow-hide nw">${hbar(x)}</td><td class="msg">${newsOf(x)}</td></tr>`;
  });
  const what = S.q ? h`No profile matches <span class="m">${S.q}</span>.` : "No profile has this status.";
  const clear = h` <button type="button" class="lnk" data-clear="filter">Clear the filter</button>`;
  const empty = h`<tr><td colspan="5">${D.profiles.length ? inline(TONE.OFF, what, clear) : inline(TONE.OFF, "No profile yet.")}</td></tr>`;
  return h`<section class="card"><div class="chead"><h2>Profiles <span class="sub">${neverChecked() ? "Not read yet" : `${D.cluster.counts.profiles} profiles, ${D.cluster.counts.packs} packs`}</span></h2>
    <div class="tb"><div class="seg" role="group" aria-label="Filter by status">${FILTERS.map(([k, l]) => h`<button data-filter="${k}" class="${S.filter === k ? "on" : ""}">${l} ${num(cnt[k])}</button>`)}</div>
    <input class="search" id="q" type="search" placeholder="Filter by name" aria-label="Filter by name" value="${S.q}"></div></div>
    <div class="tw"><table id="pt"><thead><tr>${sortTh("pt", "status", "Status")}${sortTh("pt", "profile", "Profile")}${sortTh("pt", "type", "Type", "narrow-hide")}${sortTh("pt", "packs", "Packs", "narrow-hide")}<th>Message</th></tr></thead><tbody>${rows.length ? rows : empty}</tbody></table></div></section>`;
}

/* ---- The screens ---- */
function viewOverview() {
  return withChecks("overview", crumbs("Overview"), () => h`${tiles()}<div class="grid2"><div class="stack">${findingsCard()}${eventsCard()}</div>${coverageCard()}</div>${profilesCard()}`);
}

function viewProfiles() {
  return withChecks("profiles", crumbs("Profiles"), profilesCard);
}

function viewFindings() {
  const rows = D.newer;
  const card = () => {
    if (!rows.length) return noNewer(SIZE.CARD);
    return h`<div class="tw"><table><tbody>${rows.map(r => {
      const first = r.profiles[0], k = packOfRow(first ? profileOf(first) : null, r);
      const href = k ? packHash(first, k) : first ? profileHash(first) : "#newer";
      return h`<tr class="link" data-go="${href}"><td class="w90">${statusBadge(STATUS.NEWER)}</td><td class="m"><a href="${href}">${move(r.name, r.version, r.newest)}</a></td><td>${r.profiles.map(s => lab("profile", s))}</td><td>${impactCell(first, k, r)}</td></tr>`;
    })}</tbody></table></div>`;
  };
  return withChecks("findings", crumbs("Findings"), () => h`${findingsCard()}
    <section class="card"><div class="chead"><h2>Newer versions <span class="sub">${rows.length}. A choice, not a fault, so they stay out of Needs attention.</span></h2><a class="small" href="#newer">Open the table</a></div>${card()}</section>`);
}

function viewNewer() {
  return withChecks("newer", crumbs("Newer versions"), newerCard);
}

// newerCard draws the table of the newer versions, or the splash of an empty list.
const NEWER_KEYS = {pack: [r => r.name, "text"], move: [r => r.newest, "version"], impact: [r => rank(Object.keys(IMPACT))(r.impact), "num"],
  settings: [r => r.fold.settings, "num"], images: [r => r.images, "num"], charts: [r => r.charts, "num"], listed: [r => when(r.listed) || 0, "num"]};

function newerCard() {
  const n = D.cluster.counts, rows = D.newer;
  let body;
  if (!rows.length) {
    body = noNewer(SIZE.CARD);
  } else {
    const groups = [];
    rows.forEach(r => r.profiles.forEach(s => {
      let g = groups.find(q => q.subject === s);
      if (!g) { g = {subject: s, rows: []}; groups.push(g); }
      g.rows.push(r);
    }));
    groups.sort((a, b) => a.subject.localeCompare(b.subject));
    // A filter keeps a whole group when its profile matches, and else the rows whose pack matches.
    const shownGroups = groups.map(g => matches("newer", g.subject) ? g : {...g, rows: g.rows.filter(r => matches("newer", r.name))}).filter(g => g.rows.length);
    body = h`<div class="body small mut pb9"><b>Settings</b> counts what a move really changes. Impact is a hint from strata's table, so read the two together.</div>
      <div class="body tb pb8">${filterBox("newer", "Filter by pack or profile")}</div>
      <div class="tw"><table class="tree"><thead><tr>${sortTh("nt", "pack", "Pack")}${sortTh("nt", "move", "Move")}${sortTh("nt", "impact", "Impact")}${sortTh("nt", "settings", "Settings", "n")}${sortTh("nt", "images", "Images", "n narrow-hide")}${sortTh("nt", "charts", "Charts", "n narrow-hide")}${sortTh("nt", "listed", "Listed", "narrow-hide")}<th class="narrow-hide" title="${ASK_WHAT}">Palette</th></tr></thead><tbody>${shownGroups.length ? shownGroups.map(g => {
        const p = profileOf(g.subject);
        const total = p ? p.packs.length : "";
        return [h`<tr class="g"><td colspan="8"><a class="m" href="${profileHash(g.subject)}">${subjectRef(g.subject)}</a> <span class="mut">· ${g.rows.length}${total !== "" ? h` of ${total}` : ""} ${plural(total === "" ? g.rows.length : total, "pack", "packs")}</span></td></tr>`,
          sortBy("nt", g.rows, NEWER_KEYS).map(r => {
            const k = packOfRow(p, r);
            const href = k ? packHash(g.subject, k) : profileHash(g.subject);
            const count = v => moveUnread(g.subject, k) ? DASH : v;
            return h`<tr class="link" data-go="${href}"><td class="m"><a href="${href}">${r.name}</a></td><td class="m nw">${r.version}${ARROW}${r.newest}</td><td>${impactCell(g.subject, k, r)}</td><td class="n"><b>${count(r.fold.settings)}</b></td><td class="n narrow-hide">${count(r.images)}</td><td class="n narrow-hide">${count(r.charts)}</td><td class="m mut narrow-hide nw">${day(r.listed)}${r.new ? h` <span class="badge fresh">new</span>` : ""}</td><td class="narrow-hide">${k ? askCell(g.subject, k) : ""}</td></tr>`;
          })];
      }) : h`<tr><td colspan="8">${inline(TONE.OFF, h`No pack or profile matches <span class="m">${S.qs.newer}</span>.`)}</td></tr>`}</tbody></table></div>`;
  }
  const others = packsWithoutMove();
  const sub = rows.length ? h`<span class="sub">${rows.length} in ${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")}. ${others.current} other ${plural(others.current, "pack is", "packs are")} current.${others.unchecked ? ` ${others.unchecked} ${plural(others.unchecked, "is", "are")} not checked.` : ""}</span>` : h`<span class="sub">${num(rows.length)}</span>`;
  return h`<section class="card"><div class="chead"><h2>Newer versions ${sub}</h2></div>${body}</section>`;
}

// packsWithoutMove counts the packs that have no newer version: the ones that are current, and the ones that were not checked.
function packsWithoutMove() {
  let current = 0, unchecked = 0;
  D.profiles.forEach(x => x.packs.forEach(p => {
    if (p.newer) return;
    if (p.status === STATUS.NOT_CHECKED) unchecked++; else current++;
  }));
  return {current, unchecked};
}

function viewNotChecked() {
  return withChecks("notchecked", crumbs("Not checked"), () => {
    const rows = D.notChecked;
    const body = rows.length ? h`<div class="tw"><table><thead><tr><th>What</th><th>Profile</th><th>Why</th></tr></thead><tbody>${rows.map(r => {
      const first = r.profiles[0];
      const p = first ? profileOf(first) : null;
      const names = r.packs.map(k => {
        const found = D.profiles.filter(x => r.profiles.includes(subjectOf(x))).flatMap(x => x.packs).find(q => q.key === k);
        return found ? at(found.name, found.version) : packName(k);
      });
      const what = [...new Set(names)].join(", ");
      const why = r.cause === CAUSE.OLD_COPY && r.registry ? `Current against an older copy of ${r.registry}. A newer version can be missing here.` : (CAUSE_TEXT[r.cause] || r.cause);
      const pods = r.cause === CAUSE.PODS_NOT_READ;
      return h`<tr class="link" data-go="${pods ? "#images" : p ? profileHash(first) : "#profiles"}"><td class="m">${what || mut(pods ? "The pods" : "The profile")}</td><td class="m">${r.profiles.length ? r.profiles.join(", ") : mut("-")}</td><td>${why}</td></tr>`;
    })}</tbody></table></div>` : noCauses();
    return h`<section class="card"><div class="chead"><h2>Not checked on this run <span class="sub">${rows.length}. This never raises a severity, and it stops the page from saying "No change".</span></h2></div>${body}</section>${coverageCard()}`;
  });
}

/* The two sides of a profile: the published version, and the copy of this cluster in Palette. */
function verdict(x) {
  const pub = x.published, cp = x.clusterCopy, carried = readErrorOf(subjectOf(x));
  let pubCard;
  if (!pub) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${badge("y", "No record")}</div><div class="small mut mt6">Nothing to compare with.</div></div>`;
  } else if (pub.status === VERIFY.DRIFTED) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${publishedBadge(pub)}</div><div class="m mt6">root <span class="was">${root(pub.rootRecorded)}</span>${ARROW}<span class="now">${root(pub.rootLive)}</span></div></div>`;
  } else if (pub.status === VERIFY.UNCHANGED && carried) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${publishedBadge(pub, carried)}</div><div class="m mt6">root ${root(pub.rootRecorded)}</div><div class="small mut mt3">${CARRIED_WORDS}</div></div>`;
  } else if (pub.status === VERIFY.UNCHANGED) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${publishedBadge(pub)}</div><div class="m mt6">root ${root(pub.rootRecorded)}</div><div class="small mut mt3">Unchanged since the record</div></div>`;
  } else {
    pubCard = h`<div class="vcard"><div class="h">Published version ${publishedBadge(pub)}</div><div class="m mt6">root ${root(pub.rootRecorded)}</div><div class="small mut mt3">Strata could not read this version.</div></div>`;
  }
  let copyCard;
  if (!cp) {
    copyCard = h`<div class="vcard"><div class="h">This cluster's copy in Palette ${badge("y", "No record")}</div><div class="small mut mt6">Nothing to compare with.</div></div>`;
    return h`<div class="vs">${pubCard}${copyCard}</div>`;
  }
  const head = h`<div class="h">This cluster's copy in Palette ${copyBadge(cp, carried)}</div>`;
  if (carried && COPY_KEPT.includes(cp.status)) {
    return h`<div class="vs">${pubCard}<div class="vcard">${head}<div class="m mt6">root ${root(cp.rootB)}</div><div class="small mut mt3">${CARRIED_WORDS}</div></div></div>`;
  }
  switch (cp.status) {
    case COPY.SAME:
      copyCard = h`<div class="vcard">${head}<div class="m mt6">root ${root(cp.rootB)}</div><div class="small mut mt3">Unchanged since the record</div></div>`; break;
    case COPY.SAME_CONTENT:
      copyCard = h`<div class="vcard">${head}<div class="m mt6">root ${root(cp.rootB)}</div><div class="small mut mt3">The roots differ and the content does not. This is not a drift.</div></div>`; break;
    case COPY.DIFFERS:
      copyCard = h`<div class="vcard">${head}<div class="m mt6">root ${root(pub && pub.rootRecorded)}${ARROW}<span class="now">${root(cp.rootB)}</span></div></div>`; break;
    case COPY.NOT_ON_CLUSTER:
      copyCard = h`<div class="vcard">${head}<div class="small mut mt6">This cluster does not run this version.</div></div>`; break;
    default:
      copyCard = h`<div class="vcard">${head}<div class="small mut mt6">Strata could not read this copy.</div></div>`;
  }
  return h`<div class="vs">${pubCard}${copyCard}</div>`;
}

function viewProfile(subject) {
  const x = profileOf(subject);
  if (!x) return viewMissing(crumbs(h`<a href="#profiles">Profiles</a>`, subject), "profiles", {title: "This profile is not here", name: subject, why: "is in no record now. A new revision of the records can remove it.", home: ["#profiles", "All profiles"]});
  const r = D.cluster.records;
  const record = x.status === STATUS.NO_RECORD ? "none" : r.revision ? `rev ${r.revision}` : r.baseline;
  const fs = D.findings.filter(f => f.profile === subject);
  const ev = D.events.filter(q => q.object.split(", ").includes(subject));
  const events = ev.length ? h`<section class="card"><div class="chead"><h2>Events</h2></div><div class="tw"><table><thead><tr><th>Type</th><th class="narrow-hide">Reason</th><th>Age</th><th class="narrow-hide">From</th><th>Message</th></tr></thead><tbody>${ev.map(q => {
    return h`<tr><td>${badgeOf(EVENT_BADGE, q.type)}</td><td class="m narrow-hide">${q.reason}</td><td class="mut nw">${ago(q.time)}</td><td class="mut narrow-hide">${eventFrom(q)}</td><td>${q.message}</td></tr>`;
  })}</tbody></table></div></section>` : "";
  const advice = fs.some(f => f.kind === KIND.PUBLISHED && !f.alsoOnCluster) ? h`<div class="note r small"><b>What to do.</b> Do not repair the edited version in place: that is a second edit. Publish a new version with the change through review, and move the clusters to it. A new cluster gets this edit at once. The finding clears when the version goes back to its record.${D.cluster.cluster.updatesPending ? " An update waits for this cluster. Do not accept it until the content is what the team approved." : ""}</div>` : "";
  const packs = x.packs.length ? packStack(subject, x.packs, false, "") : h`<div class="body">${inline(TONE.OFF, x.status === STATUS.NO_RECORD ? "No record holds this profile, so the packs are not listed." : "This profile holds no pack.")}</div>`;
  return {nav: "profiles", crumbs: crumbs(h`<a href="#profiles">Profiles</a>`, subjectRef(subject)), body: h`${alertBars()}
    <section class="card"><div class="body"><div class="title"><h1 class="m h1m">${subjectRef(subject)}</h1>${statusBadge(x.status)}</div>
    <div class="meta"><div><span>Type</span>${x.type}</div><div><span>Scope</span>${x.scope || "-"}</div><div><span>Packs</span>${x.packs.length}</div><div><span>Record</span>${record}</div></div></div></section>
    ${verdict(x)}${events}${advice}
    <section class="card"><div class="chead"><h2>Packs <span class="sub">${x.packs.length}, from the top layer to the bottom</span></h2></div>${packs}</section>
    ${x.status === STATUS.NO_RECORD ? "" : checkCard(terminalRows(subject, x, null))}`};
}

// eventFrom names where an event comes from.
function eventFrom(q) {
  switch (q.type) {
    case EVENT.DRIFT: return "strata-observer";
    case EVENT.INFO: return "audit log";
    default: return "palette";
  }
}

// newerCell never says "current" for a pack that was not checked, or before the versions were read.
function newerCell(p) {
  if (p.newer) return h`${ARROW}${p.newer}`;
  if (p.status === STATUS.NOT_CHECKED) return mut("not checked");
  if (!D.cluster.versions.readAt) return mut("-");
  return mut("current");
}

/* ---- The stack of the packs ----
   A profile shows its packs as a stack of layers, as Palette builds a cluster: the add-ons on top, the OS at the bottom. The color on
   the left of a slab is the status of the pack. The page of a pack shows the same stack, small, with its own slab lit, so a person
   sees one picture of a pack on both pages and steps from one pack to the next. */
function newerWords(p) {
  if (p.newer) return h`newer: <span class="m">${ref(p.name, p.newer)}</span>`;
  if (p.status === STATUS.NOT_CHECKED) return "not checked";
  if (!D.cluster.versions.readAt) return "versions not read yet";
  return "current";
}
const imagesWords = n => n ? `${n} ${plural(n, "image", "images")} listed` : h`<span title="${IMAGES_LISTED}">no image list</span>`;

function packSlab(subject, p, compact, here) {
  const href = packHash(subject, p), tone = (BADGE[p.status] || ["y"])[0];
  const sub = compact ? "" : h`<span class="sub">${newerWords(p)} · ${imagesWords(p.images)}</span>`;
  return h`<li class="pslab t-${tone}${here ? " here" : ""}"><a href="${href}"${here ? h` aria-current="page"` : ""}><span class="layer">${p.layer}</span><span class="who"><span class="m ref">${ref(p.name, p.version)}</span>${sub}</span>${statusBadge(p.status)}</a></li>`;
}

// packStack lists the packs from the top layer to the bottom one. here is the key of the lit slab, or "".
function packStack(subject, packs, compact, here) {
  return h`<ul class="pstack${compact ? " compact" : ""}" aria-label="The packs of ${subject}, from the top layer to the bottom">${[...packs].reverse().map(p => packSlab(subject, p, compact, p.key === here))}</ul>`;
}

// viewMissing is the view of a page that the data does not hold. A page with nothing read shows the splash of its state.
function viewMissing(cr, nav, what) {
  return {nav, crumbs: cr, body: h`${alertBars()}${stateSplash() || missingSplash(what)}`};
}

/* ---- The images that pods run ---- */
const podsOf = img => img.digests.reduce((sum, d) => sum + d.pods, 0);
// realDigests lists the digests that a pod reported. A pod that never started, and a local image, report none.
const realDigests = img => img.digests.filter(d => d.digest);
const digestText = d => d ? h`<span title="${d}">${digest(d)}</span>` : mut("none");

// packCell names the pack of an image. An image that matches a pack by name and tag says so.
function packCell(img) {
  if (!img.packs.length) return mut("no pack");
  const more = img.packs.length > 1 ? h` <span class="mut small">+${img.packs.length - 1}</span>` : "";
  const how = img.match === MATCH.NAME_AND_TAG ? h`<div class="mut small">matched by name and tag</div>` : "";
  return h`${lab("pack", packName(img.packs[0].key))}${more}${how}`;
}

// digestCell shows the first digest that a pod reported, and +1 for each other one. An image with no digest reads Not checked.
function digestCell(img) {
  const real = realDigests(img);
  if (!real.length) return mut("Not checked");
  return h`${digestText(real[0].digest)}${real.length > 1 ? h` <span class="mut">+${real.length - 1}</span>` : ""}`;
}

function imageRow(img) {
  const href = imageHash(img.ref);
  return h`<tr class="link" data-go="${href}"><td class="w110">${imageBadge(img.status)}</td><td class="m wrap"><a href="${href}">${img.ref}</a><div class="narrow-show">${packCell(img)}</div></td><td class="narrow-hide">${packCell(img)}</td><td class="n narrow-hide">${podsOf(img)}</td><td class="m nw">${digestCell(img)}</td></tr>`;
}

// podsOffSplash is the offer of the read of the pods. The read is a choice of the team, so it is no fault.
const podsOffSplash = () => splash(TONE.OFF, "Pods are not on the menu here",
  h`This install does not read the pods. With the read on, the page shows the digest that each tag runs, and finds a tag that moved. The read needs a role that can list pods, and that role can also read their <span class="m">env</span>.`,
  next(PODS_NEXT), SIZE.CARD);

// podsFailedSplash tells that the first reads of the pods failed. A refused role gets the command that asks the API server.
function podsFailedSplash(err) {
  const refused = err.kind === "kube-api" && wasRefused(err.status);
  return splash(TONE.WARN, "The pods stay silent", h`The last read of the pods failed. <b>${podsErrorWords(err)}.</b> The findings of the content check are not affected.`,
    refused ? next("Ask the API server if the role of the observer can list pods.", cmd(CAN_LIST_PODS_COMMAND)) : next("Read the log.", cmd(LOG_COMMAND)), SIZE.CARD);
}

// imagesBody gives the body of the card of the list, from the state of the read of the pods.
function imagesBody() {
  const o = D.images;
  if (podsOff()) return podsOffSplash();
  if (o.pods === PODS.NOT_READ) {
    return o.error ? podsFailedSplash(o.error) : splash(TONE.BUILD, "Reading the pods", "No read of the pods has ended yet. This page fills in when one does.", "", SIZE.CARD);
  }
  if (!o.images.length) return splash(TONE.OFF, "No image to list", "The observer read the pods, and no pod runs an image.", "", SIZE.CARD);
  const shown = sortBy("it", o.images.filter(img => matches("images", img.ref, ...img.packs.map(q => q.key))), {
    status: [img => rank([IMAGE.SPLIT, IMAGE.MOVED, IMAGE.NOT_CHECKED, IMAGE.STEADY])(img.status), "num"], image: [img => img.ref, "text"],
    pack: [img => img.packs.length ? packName(img.packs[0].key) : "~", "text"], pods: [podsOf, "num"]});
  const none = h`<tr><td colspan="5">${inline(TONE.OFF, h`No image matches <span class="m">${S.qs.images}</span>.`)}</td></tr>`;
  return h`<div class="body tb pb8">${filterBox("images", "Filter by image or pack")}</div><div class="tw"><table><thead><tr>${sortTh("it", "status", "Status")}${sortTh("it", "image", "Image")}${sortTh("it", "pack", "Pack", "narrow-hide")}${sortTh("it", "pods", "Pods", "n narrow-hide")}<th>Digest</th></tr></thead><tbody>${shown.length ? shown.map(imageRow) : none}</tbody></table></div>`;
}

function viewImages() {
  const o = D.images, n = o.counts, rest = n.images - n.matched;
  const sub = podsRead() ? h`${n.images} ${plural(n.images, "image", "images")}. ${n.matched} ${plural(n.matched, "matches", "match")} a pack. ${rest} ${plural(rest, "does", "do")} not.` : podsOff() ? "Pods not read" : "Not read yet";
  const note = podsRead() && o.images.length ? h`<div class="note small">This page says so when what this cluster pulled differs from what it first pulled, or when two pods of one tag differ. It cannot say whether a tag moved at its source.</div>` : "";
  return {nav: "images", crumbs: crumbs("Images"), body: h`${alertBars()}<section class="card"><div class="chead"><h2>Images <span class="sub">${sub}</span></h2></div>${imagesBody()}</section>${note}`};
}

// recordRows gives one row for each pack that names the image. The status is the one of its profile.
function recordRows(img) {
  return img.packs.map(p => {
    const x = profileOf(p.profile);
    const where = img.packs.length > 1 ? h`Record ${mut("· " + packName(p.key))}` : "Record";
    return {where, digest: img.byDigest ? "digest" : mut("tag only"), status: x ? statusBadge(x.status) : mut("-")};
  });
}

// podRows gives one row for each digest that pods run. A digest that is not a repo digest reads Not checked, with the reason.
function podRows(img) {
  return img.digests.map(d => ({
    where: `Pods (${d.pods})`, digest: digestText(d.digest),
    status: h`${digestBadge(d.status)}${DIGEST_WHY[d.status] ? h` ${mut(DIGEST_WHY[d.status])}` : ""}`,
  }));
}

const IMAGE_NOTE = {
  [IMAGE.MOVED]: ["a", "The digest that the observer saw first is not the digest that the pods run now. A pull of this tag gave other content."],
  [IMAGE.SPLIT]: ["r", "Pods run this tag with two digests at one time. The tag moved between two pulls."],
};

function imageTable(img) {
  const rows = [...recordRows(img), ...podRows(img)];
  const [cls, words] = IMAGE_NOTE[img.status] || [];
  const note = words ? h`<div class="body"><div class="note ${cls} small">${words}</div></div>` : "";
  return h`<div class="tw"><table><thead><tr><th>Where</th><th class="narrow-hide">Reference</th><th>Digest</th><th>Status</th></tr></thead><tbody>${rows.map(r => h`<tr><td>${r.where}</td><td class="m wrap narrow-hide">${img.ref}</td><td class="m nw">${r.digest}</td><td>${r.status}</td></tr>`)}</tbody></table></div>${note}`;
}

// podsCatch says what a read of the pods catches and what it does not. It is the same for each image.
const PODS_CATCH = h`<div class="body small"><b>When this happens</b><ul class="tight"><li>A tag pulled directly from a mutable registry, when a pod lands on a node with no cached copy.</li><li><span class="m">imagePullPolicy: Always</span> or <span class="m">:latest</span>, at any restart.</li><li>A mutable mirror, or a pipeline that copies over the same tag.</li></ul>
  <div class="mut mt6">Needs read access to pods, and no registry and no credential. No Palette route has this digest.</div>
  <div class="mt9"><b>What it does not catch</b></div><ul class="tight"><li>A move with no new pull.</li><li>An image that no pod runs.</li><li>A difference between clusters.</li></ul>
  <div class="mut mt6">It stays silent when the pods pull an unchanged digest from a mirror, also when the vendor rebuilt the tag.</div></div>`;

function viewImage(ref) {
  const img = D.images.images.find(i => i.ref === ref);
  const cr = crumbs(h`<a href="#images">Images</a>`, ref);
  if (!img) return viewMissing(cr, "images", {title: "No pod runs this image", name: ref, why: "runs in no pod now. A new read of the pods can remove it.", home: ["#images", "All images"]});
  const id = "image-" + ref, tab = S.tabs[id] || "this";
  const profiles = [...new Set(img.packs.map(p => p.profile))];
  const packs = img.packs.length ? img.packs.map(p => lab("pack", packName(p.key))) : mut("none");
  const links = profiles.length ? profiles.map(s => h`<a class="m" href="${profileHash(s)}">${s}</a> `) : mut("none");
  const fix = IMAGE_NOTE[img.status] ? h`<section class="card"><div class="chead"><h2>The fix</h2></div><div class="body"><ul class="tight"><li>Name the image by its digest where the chart or the manifest reads it, in a new version of the pack.</li><li>Or copy the image, by digest, to a registry with immutable tags, and name the copy in the pack.</li></ul></div></section>` : "";
  return {nav: "images", crumbs: cr, body: h`${alertBars()}
    <section class="card"><div class="body"><div class="title"><h1 class="m h1m">${ref}</h1>${imageBadge(img.status)}</div>
    <div class="meta"><div><span>${img.packs.length > 1 ? "Packs" : "Pack"}</span>${packs}</div><div><span>${profiles.length > 1 ? "Profiles" : "Profile"}</span>${links}</div><div><span>Named by</span>${img.byDigest ? "a digest" : "a tag"}</div><div><span>Match</span>${MATCH_WORDS[img.match] || img.match}</div></div></div></section>
    <section class="card">${tabBar(id, [["this", "This image"], ["catch", "What pods do catch"]])}${tab === "catch" ? PODS_CATCH : imageTable(img)}</section>${fix}`};
}

/* ---- The comparison of a pack ---- */
// settingsTable draws the changed values. jump is the id of the text of the values, or "" when the page holds none.
const settingsTable = (rows, jump) => h`<div class="tw"><table><thead><tr><th>Setting</th><th>Before</th><th>After</th></tr></thead><tbody>${rows.map(r => settingRow(r, jump))}</tbody></table></div>`;

// settingRow draws one changed value. A value of one line shows whole. A value of more lines shows only the lines that differ.
function settingRow(r, jump) {
  const cells = lineCells(r.old, r.new, jump) || h`<td class="was val">${leafText(r.old)}</td><td class="now val">${leafText(r.new)}</td>`;
  return h`<tr><td class="m">${r.path}</td>${cells}</tr>`;
}

// A long value shows the lines between its common start and its common end, with at most CONTEXT lines of the common
// start and end around them: up to CONTEXT - 1 above, and the rest below. More than MAX_DIFFER lines between them show as
// a count. It is no full diff: the text of the values holds that.
const CONTEXT = 3;
const MAX_DIFFER = 40;

// linesOf splits a value into its lines. An absent value has none.
function linesOf(v) {
  if (v === undefined || v === null) return [];
  return (typeof v === "string" ? v : JSON.stringify(v)).split("\n");
}

// sharedEnds counts the lines that two values share at their start, then at their end. The end stops where the start stops.
function sharedEnds(a, b) {
  let top = 0;
  while (top < a.length && top < b.length && a[top] === b[top]) top++;
  let tail = 0;
  while (tail < a.length - top && tail < b.length - top && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail++;
  return [top, tail];
}

const valueLine = (cls, n, mark, text) => h`<div class="vl ${cls}"><span class="no">${n}</span><span class="mk">${mark}</span><span class="t">${text}</span></div>`;
const GAP = valueLine("e", "", "", "…");

// sideLines draws one side of a long value: the lines from win.top to the end less win.tail, with win.before lines
// above them and win.after lines below them. When the other side has as many lines, a line that it holds at the same
// place did not change.
function sideLines(lines, other, win, cls, mark) {
  const end = lines.length - win.tail, out = [];
  const kept = i => other.length === lines.length && other[i] === lines[i];
  if (win.top > win.before) out.push(GAP);
  for (let i = win.top - win.before; i < end + win.after; i++) {
    const changed = i >= win.top && i < end && !kept(i);
    out.push(valueLine(changed ? cls : "c", i + 1, changed ? mark : "", lines[i]));
  }
  if (end + win.after < lines.length) out.push(GAP);
  return h`<div class="vls">${out}</div>`;
}

// lineCells gives the two cells of a value of more than one line, or null for a value of one line.
function lineCells(old, now, jump) {
  const a = linesOf(old), b = linesOf(now);
  // An end of line at the end of both values is no line.
  if (a.length > 1 && b.length > 1 && !a[a.length - 1] && !b[b.length - 1]) { a.pop(); b.pop(); }
  if (a.length < 2 && b.length < 2) return null;
  const [top, tail] = sharedEnds(a, b);
  const differ = Math.max(a.length, b.length) - top - tail;
  if (differ > MAX_DIFFER) {
    const count = `${differ} lines differ`;
    return h`<td class="val" colspan="2">${jump ? h`<button class="lnk" data-jump="${jump}">${count}</button>` : count}</td>`;
  }
  const before = Math.min(top, CONTEXT - 1), win = {top, tail, before, after: Math.min(tail, CONTEXT - before)};
  const cell = (cls, lines, other, mark) => h`<td class="${cls} val vm">${lines.length ? sideLines(lines, other, win, cls === "was" ? "d" : "a", mark) : DASH}</td>`;
  return h`${cell("was", a, b, "-")}${cell("now", b, a, "+")}`;
}

// leafText shows a changed value. A string has quotes, and a value that is absent shows a dash.
function leafText(v) {
  if (v === undefined || v === null) return DASH;
  return typeof v === "string" ? `"${v}"` : JSON.stringify(v);
}

// tabBar draws the tabs of a card. The tab that a person chose is on, else the first tab of the list, or dflt.
function tabBar(id, items, dflt = items[0][0]) {
  const cur = S.tabs[id] || dflt;
  return h`<div class="tabs" role="tablist">${items.map(([k, l, n]) => h`<button class="tab ${k === cur ? "on" : ""}" role="tab" aria-selected="${k === cur}" data-tab="${id}" data-k="${k}">${l}${n === undefined ? "" : h`<em>${n}</em>`}</button>`)}</div>`;
}

// tabCounts gives the key, the label, and the number of changes of each tab of a comparison, in the order of the tabs.
function tabCounts(d) {
  const v = d.values;
  return [["values", "Values", v.settings.length + v.followsTheVersion.length + v.repeatsTheLists.length], ["manifests", "Manifests", d.manifests.length + d.paramChanges.length],
    ["images", "Images", d.imageChanges.length], ["charts", "Charts", d.chartChanges.length]];
}

// hasChange says that a tab holds a change. The Values tab also holds the text of values that strata cannot read as settings.
function hasChange(d, key) {
  const [, , n] = tabCounts(d).find(([k]) => k === key);
  return key === "values" ? n > 0 || !d.values.parseable || !!d.values.unified : n > 0;
}

// firstTab gives the tab that a comparison opens on: the first tab with a change, and Values when none has one.
const firstTab = d => (tabCounts(d).find(([k]) => hasChange(d, k)) || ["values"])[0];

// emptyTab says that a tab holds no change, and it links to the first other tab that does.
function emptyTab(id, d, here, noun) {
  const other = tabCounts(d).find(([k]) => k !== here && hasChange(d, k));
  const link = other ? h`<button type="button" class="lnk" data-tab="${id}" data-k="${other[0]}">The change is on ${other[1]} →</button>` : "";
  return h`<div class="body">${inline(quietTone(), `No ${noun} changed${other ? " here" : ""}.`, link)}</div>`;
}

function changesPanel(list, noun, head, id, d) {
  if (!list.length) return emptyTab(id, d, noun + "s", noun);
  return h`<div class="tw"><table><thead><tr><th></th><th>${head}</th><th>Before</th><th>After</th></tr></thead><tbody>${list.map(c => h`<tr><td class="m">${OPS[c.op] || c.op}</td><td class="m">${c.name}</td><td class="was">${c.old || DASH}</td><td class="now">${c.new || DASH}</td></tr>`)}</tbody></table></div>`;
}

/* The text of a diff, as `strata compare --full` prints it. A line is a value of the cluster, so each line goes through h``. */
const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

// parseUnified reads a unified diff into rows. A row is one line of a hunk with its number on each side: t is
// " " for a line on both sides, "-" for a removed line, and "+" for an added line. A row with t "@" is the head
// of a hunk that follows another hunk, and it shows the gap. noEol says that the line has no end of line. The
// two header lines come before the first hunk and give no row. In a hunk, a line "--- x" is a removed line.
function parseUnified(text) {
  const rows = [];
  let a = 0, b = 0, hunks = 0;
  const lines = String(text).split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  for (const line of lines) {
    const head = line.startsWith("@@") ? HUNK.exec(line) : null;
    if (head) {
      if (hunks++) rows.push({t: "@", x: line});
      a = Number(head[1]);
      b = Number(head[2]);
    } else if (!hunks) {
      continue;
    } else if (line[0] === "\\") {
      if (rows.length && rows[rows.length - 1].t !== "@") rows[rows.length - 1].noEol = true;
    } else if (line[0] === "-") {
      rows.push({t: "-", x: line.slice(1), na: a++, nb: ""});
    } else if (line[0] === "+") {
      rows.push({t: "+", x: line.slice(1), na: "", nb: b++});
    } else {
      rows.push({t: " ", x: line[0] === " " ? line.slice(1) : line, na: a++, nb: b++});
    }
  }
  return rows;
}

const countOf = (rows, t) => rows.filter(r => r.t === t).length;
const noEolRow = (left, right) => h`<tr class="n"><td class="no"></td><td>${left ? NO_EOL : ""}</td><td class="no"></td><td>${right ? NO_EOL : ""}</td></tr>`;

// unifiedRows lays out a diff in one column, with the number of each side.
function unifiedRows(rows) {
  return rows.flatMap(r => {
    if (r.t === "@") return [h`<tr class="h"><td class="no"></td><td class="no"></td><td class="c">${r.x}</td></tr>`];
    const line = h`<tr class="${r.t === "+" ? "a" : r.t === "-" ? "d" : ""}"><td class="no">${r.na}</td><td class="no">${r.nb}</td><td class="c">${r.t} ${r.x}</td></tr>`;
    return r.noEol ? [line, h`<tr class="n"><td class="no"></td><td class="no"></td><td class="c">${NO_EOL}</td></tr>`] : [line];
  });
}

// splitRows lays out a diff in two columns. In a run of changes, the removed lines on the left meet the added lines on the right.
function splitRows(rows) {
  const out = [];
  let dels = [], adds = [];
  const flush = () => {
    for (let i = 0; i < Math.max(dels.length, adds.length); i++) {
      const d = dels[i], a = adds[i];
      out.push(h`<tr><td class="no">${d ? d.na : ""}</td><td class="${d ? "cd" : ""}">${d ? d.x : ""}</td><td class="no">${a ? a.nb : ""}</td><td class="${a ? "ca" : ""}">${a ? a.x : ""}</td></tr>`);
    }
    const left = dels.some(d => d.noEol), right = adds.some(a => a.noEol);
    if (left || right) out.push(noEolRow(left, right));
    dels = [];
    adds = [];
  };
  for (const r of rows) {
    if (r.t === "-") { dels.push(r); continue; }
    if (r.t === "+") { adds.push(r); continue; }
    flush();
    if (r.t === "@") {
      out.push(h`<tr class="h"><td colspan="4">${r.x}</td></tr>`);
    } else {
      out.push(h`<tr><td class="no">${r.na}</td><td>${r.x}</td><td class="no">${r.nb}</td><td>${r.x}</td></tr>`);
      if (r.noEol) out.push(noEolRow(true, true));
    }
  }
  flush();
  return out;
}

// NARROW is the width below which two columns of a split view are too thin to read. There the page shows the unified
// layout and no control of the layout.
const NARROW = "(max-width: 640px)";
const narrow = () => typeof matchMedia === "function" && matchMedia(NARROW).matches;

// The columns of each layout. A table with a fixed layout takes its widths from these, so a long line wraps in its own
// column and never pushes the other side away.
const COLS = {
  unified: h`<colgroup><col class="no"><col class="no"><col></colgroup>`,
  split: h`<colgroup><col class="no"><col><col class="no"><col></colgroup>`,
};

// diffBlock draws the head and the lines of one diff. The control of the layout sits in the head, and it sets the one layout of the page.
// With an id, a link of the page can jump to the block.
function diffBlock(title, op, rows, id) {
  const view = narrow() ? "unified" : S.view;
  const seg = narrow() ? "" : h`<span class="seg" role="group" aria-label="Diff layout">${VIEWS.map(([k, l]) => h`<button data-view="${k}" aria-pressed="${S.view === k ? "true" : "false"}" class="${S.view === k ? "on" : ""}">${l}</button>`)}</span>`;
  const lines = view === "split" ? splitRows(rows) : unifiedRows(rows);
  return h`<div class="dblock"${id ? h` id="${id}"` : ""}><div class="diffhead"><span>${title}${op ? h` <span class="mut">${op}</span>` : ""} <span class="now">+${countOf(rows, "+")}</span> <span class="was">-${countOf(rows, "-")}</span></span>${seg}</div><div class="diff"><table class="${view}">${COLS[view]}<tbody>${lines}</tbody></table></div></div>`;
}

// tooLargeNote tells that the observer kept no text, and it names the command that prints the text.
const tooLargeNote = command => splash(TONE.OFF, "Too much rock to carry", "This text is too large to keep here. The observer keeps at most 256 KiB for each side. A terminal shows all of it.", cmd(command), SIZE.LINE);

// manifestsPanel shows the text of each manifest that has text. A manifest with no text, and each parameter, show in a small table.
function manifestsPanel(d, id, command) {
  if (!d.manifests.length && !d.paramChanges.length) return emptyTab(id, d, "manifests", "manifest");
  const texts = d.manifests.map(m => [m, parseUnified(m.unified || "")]);
  const plain = [...texts.filter(([, rows]) => !rows.length).map(([m]) => ["Manifest", m]), ...d.paramChanges.map(m => ["Parameter", m])];
  const table = plain.length ? h`<div class="tw"><table><thead><tr><th>Change</th><th>Kind</th><th>Name</th></tr></thead><tbody>${plain.map(([kind, m]) => h`<tr><td class="m">${m.op}</td><td>${kind}</td><td class="m">${m.name}</td></tr>`)}</tbody></table></div>` : "";
  const blocks = texts.filter(([, rows]) => rows.length).map(([m, rows]) => diffBlock(`manifest ${m.name}`, m.op, rows));
  return h`${table}${blocks}${d.textTooLarge && d.manifests.length ? tooLargeNote(command) : ""}`;
}

function valuesPanel(d, id, command) {
  const v = d.values, s = v.settings, f = v.followsTheVersion, r = v.repeatsTheLists, total = s.length + f.length + r.length;
  const rows = parseUnified(v.unified || "");
  // The long values link to the text of the values, when the page shows it.
  const textId = id + "-text", jump = rows.length ? textId : "";
  const text = rows.length ? diffBlock(`${d.key} values`, "", rows, textId) : d.textTooLarge && (total > 0 || !v.parseable) ? tooLargeNote(command) : "";
  if (!v.parseable && total === 0) return h`<div class="body">${inline(TONE.OFF, text ? "Strata cannot read the values of this pack as settings. The text below shows the change." : "Strata cannot read the values of this pack as settings.")}</div>${text}`;
  const lead = total ? h`<div class="body small mut pb8">${total} changed ${plural(total, "value", "values")}: ${s.length} ${plural(s.length, "setting", "settings")}, ${f.length} follow the version, ${r.length} repeat the lists.</div>` : emptyTab(id, d, "values", "value");
  const masked = d.masked.length ? h`<div class="note small nt"><b>Masked:</b> ${d.masked.map(m => h`<span class="m">${m}</span>`)}. An equal root does not show an equal secret.</div>` : "";
  const follows = f.length ? h`<details data-open="${id}-f" ${openAttr(id + "-f")}><summary>Follows the version: ${f.length} ${plural(f.length, "value", "values")}</summary>${settingsTable(f, jump)}</details>` : "";
  const repeats = r.length ? h`<details data-open="${id}-r" ${openAttr(id + "-r")}><summary>Repeats the lists: ${r.length} ${plural(r.length, "value", "values")}</summary><div class="body small mut">The Images and Charts tabs show these.</div></details>` : "";
  return h`${lead}${s.length ? settingsTable(s, jump) : ""}${masked}${text}${follows}${repeats}`;
}

/* A comparison of a pack has one card, and one dropdown "Compare" in its head: the record against the published version, the
   record against the copy of this cluster, and the pack against its newer version. The card opens on the first side that
   holds a change. A side with no change sends no request, and it shows the facts of the three sides. */

// The commands of the card "Check this yourself" and of the splash for a text above the cap. record.json is the file that
// the first row of the card writes. Each value from the data goes through shq(), so a command pastes as the arguments it means.
const publishedCommand = subject => `strata compare ${RECORD_FILE} ${shq(subject)} --full`;
const clusterCommand = (x, full) => `strata compare ${RECORD_FILE} ${shq(`cluster:${clusterName()}/${x.name}`)}${full ? " --full" : ""}`;
// packRef gives a pack of a registry as one word of a shell.
const packRef = (row, version) => shq(`pack:${row.registryUid}/${row.name}@${version}`);
const moveCommand = row => `strata compare ${packRef(row, row.version)} ${packRef(row, row.newest)} --full`;
const pinCommand = p => `strata behind ${RECORD_FILE} --pin ${shq(p.key)}`;
// recordKey gives the key of a record in the Secrets, as scripts/ci/observer-records.sh writes it. recordCommand reads the key in
// each of the four Secrets, because the page does not know which one holds it.
const recordKey = x => `${x.name}_${x.version}.json.gz`.replace(/[^A-Za-z0-9._-]/g, "_");
const recordCommand = x => `for i in ${RECORD_SECRETS.join(" ")}; do kubectl -n strata-observer get secret strata-observer-records-$i -o json | jq -r --arg k '${recordKey(x)}' '.data[$k] // empty'; done | base64 -d | gunzip > ${RECORD_FILE}`;

// flagTab gives the tab that a lit flag opens, or "" for a flag with no tab of its own. AUTHORED opens the first of Values and
// Manifests that holds a change.
function flagTab(flag, d) {
  switch (flag) {
    case "AUTHORED": return ["values", "manifests"].find(k => hasChange(d, k)) || "";
    case "IMAGES": return hasChange(d, "images") ? "images" : "";
    case "CHARTS": return hasChange(d, "charts") ? "charts" : "";
    default: return "";
  }
}

// flagRow draws the five flags of a comparison. A flag that lit is violet and a flag that did not is dim: that is the one signal, and
// each flag keeps one color. A hover or a focus shows what the flag means. A lit flag with a tab opens that tab, and href is the
// page of the pack. A dim flag is no link.
function flagRow(d, id, href) {
  return h`<span class="flags">${FLAGS.map(f => {
    const lit = d.flags.includes(f), tab = lit ? flagTab(f, d) : "", w = FLAG_WORDS[f], cls = lit ? "b" : "y off";
    const face = tab ? h`<a class="badge ${cls}" href="${href}" data-tab="${id}" data-k="${tab}">${f}</a>` : h`<span class="badge ${cls}" tabindex="0">${f}</span>`;
    const words = lit ? h`Lit: ${w.when}. ${w.means}` : h`Dim. It lights when ${w.when}.`;
    return h`<span class="flagw">${face}<span class="tip" role="tooltip"><b>${f}</b>${words}</span></span>`;
  })}</span>`;
}

// retrySplash tells that the detail route gave no answer. A 503 is the normal answer while a check holds the slot of the observer.
function retrySplash(d) {
  const again = `This page asks again ${REFRESH_WORDS}.`;
  if (d.status === 503) return splash(TONE.BUILD, "One moment, a check is running", `The observer answers one read at a time. ${again}`, tryButton("diff"), SIZE.LINE);
  return splash(TONE.WARN, "The observer could not read these changes", `The request failed${d.status ? ` with HTTP ${d.status}` : ""}. ${again}`, tryButton("diff"), SIZE.LINE);
}

// comparePanel gives the flags and the body of a comparison from the answer of the detail route. The command is the one that prints
// the text when the observer kept none.
// The build of a pack: the fields of its identity, and its upstream digest. A move of an OS pack can change only these: the values
// stay, and the image of the nodes changes. The table shows each field that differs, so a change of the build is never "0 of 0".
const BUILD_WORDS = {version: "Version", tag: "Tag", packUid: "Pack uid", upstream: "Upstream digest"};
const buildValue = (field, v) => field === "upstream" ? digestText(v) : v || mut("none");
function buildTable(d) {
  if (!d.build || !d.build.length) return "";
  return h`<div class="tw"><table><thead><tr><th>The build of the pack</th><th>Before</th><th>After</th></tr></thead><tbody>${d.build.map(b => h`<tr><td>${BUILD_WORDS[b.field] || b.field}</td><td class="m was">${buildValue(b.field, b.a)}</td><td class="m now">${buildValue(b.field, b.b)}</td></tr>`)}</tbody></table></div>`;
}

// sameContent says that no tab has a change and no text: only the build of the pack changed.
const sameContent = d => d.build && d.build.length > 0 && tabCounts(d).every(([, , n]) => n === 0) && !d.values.unified && !d.textTooLarge;
const sameContentWords = impact => `The values, the manifests, the images, and the charts are the same on both sides. Only the build of the pack changes.${impact ? ` Strata says: ${impact}.` : ""}`;

function comparePanel(id, d, href, command) {
  switch (d.state) {
    case "loading": return {flags: "", inner: h`<div class="body">${inline(TONE.BUILD, "Loading the changes.")}</div>`};
    case "none": return {flags: "", inner: splash(TONE.OFF, "No changes held", "The observer holds no changes for this side.", "", SIZE.LINE)};
    case "ok": {
      const data = d.data, tab = S.tabs[id] || firstTab(data);
      if (sameContent(data)) return {flags: flagRow(data, id, href), inner: h`${buildTable(data)}<div class="body">${inline(TONE.OFF, sameContentWords(data.impact.text))}</div>`};
      const panel = tab === "manifests" ? manifestsPanel(data, id, command) : tab === "images" ? changesPanel(data.imageChanges, "image", "Repository", id, data) : tab === "charts" ? changesPanel(data.chartChanges, "chart", "Chart", id, data) : valuesPanel(data, id, command);
      return {flags: flagRow(data, id, href), inner: h`${buildTable(data)}${tabBar(id, tabCounts(data), tab)}${panel}`};
    }
    default: return {flags: "", inner: retrySplash(d)};
  }
}

// sidesOf lists the comparisons that a pack can show: the record against the published version, against the copy of this cluster when
// strata read it, and the pack against its newer version.
function sidesOf(x, row) {
  const out = [[SIDE.PUBLISHED, "Record → Published now"]];
  const cp = x.clusterCopy && x.clusterCopy.status;
  if (COPY_READ.includes(cp)) out.push([SIDE.CLUSTER, "Record → This cluster's copy"]);
  if (row) out.push([SIDE.NEWER, `${at(row.name, row.version)} → ${at(row.name, row.newest)}`]);
  return out;
}

// changedSides says which sides of a pack hold a change. It reads the findings and the move, and it sends no request. A finding of
// the pods says nothing about the sides of a record.
const CLUSTER_KINDS = [KIND.CLUSTER, KIND.IMAGE_ID];
function changedSides(subject, p, row) {
  const fs = D.findings.filter(f => f.profile === subject && f.packs.includes(p.key) && !IMAGE_KINDS.includes(f.kind));
  return {
    [SIDE.PUBLISHED]: fs.some(f => !CLUSTER_KINDS.includes(f.kind)) || (p.status === STATUS.DRIFT && fs.length === 0),
    [SIDE.CLUSTER]: fs.some(f => CLUSTER_KINDS.includes(f.kind) || f.alsoOnCluster),
    [SIDE.NEWER]: !!row,
  };
}

// publishedUnread says that strata could not read the published version, or could not verify it. No answer is not an identical copy.
const publishedUnread = x => !(x.published && (x.published.status === VERIFY.UNCHANGED || x.published.status === VERIFY.DRIFTED));
// quietTone is the tone of a place that says "no change": calm only when the check allows it.
const quietTone = () => calmOk() ? TONE.CALM : TONE.OFF;

// sideFacts gives the three facts of a pack: the published version, the copy of this cluster, and the newer version.
function sideFacts(x, p, row, changed) {
  const copy = x.clusterCopy && x.clusterCopy.status;
  const readCopy = COPY_READ.includes(copy);
  const published = publishedUnread(x) ? ["Not read", "Published now"] : changed[SIDE.PUBLISHED] ? ["Changed", "Published now"] : ["Same", "Published now", "ok"];
  const cluster = readCopy ? (changed[SIDE.CLUSTER] ? ["Changed", "This cluster's copy"] : ["Same", "This cluster's copy", "ok"]) : copy === COPY.NOT_ON_CLUSTER ? ["Not here", "This cluster's copy"] : ["Not read", "This cluster's copy"];
  const newer = row ? [row.newest, "Newer version"] : p.status === STATUS.NOT_CHECKED ? ["Not checked", "Newer version"] : D.cluster.versions.readAt ? ["Current", "No newer version", "ok"] : ["Not read", "Newer version"];
  return [published, cluster, newer];
}

// undisturbed is the splash of a side with no change.
function undisturbed(side, list) {
  const words = side === SIDE.CLUSTER ? "The copy of this cluster holds what the record holds." : "The published version holds what the record holds.";
  return splash(quietTone(), "Undisturbed", words, facts(list), SIZE.CARD);
}

// notCheckedSplash is the splash of a pack that strata did not check in full. It names the cause, and it points to the screen of the causes.
const notCheckedSplash = x => splash(TONE.WARN, "Part of this layer is hidden", h`This pack was not fully checked. ${notCheckedLine(x)} The content check runs every ${D.cluster.content.every}.`,
  next(h`<a href="#notchecked">Why, on the Not checked page</a>`), SIZE.CARD);

function comparePack(subject, x, p, row) {
  const sides = sidesOf(x, row), changed = changedSides(subject, p, row), id = "side-" + subject + "-" + p.key;
  const chosen = S.side[id], side = sides.some(([k]) => k === chosen) ? chosen : (sides.find(([k]) => changed[k]) || sides[0])[0];
  const pub = x.published, cp = x.clusterCopy;
  const unread = side === SIDE.PUBLISHED && publishedUnread(x);
  const select = h`<select id="side" class="search auto" data-id="${id}" aria-label="Compare">${sides.map(([k, l]) => h`<option value="${k}" ${k === side ? raw("selected") : ""}>${l}</option>`)}</select>`;
  const to = side === SIDE.CLUSTER ? cp && cp.rootB : pub && pub.rootLive;
  const roots = side === SIDE.NEWER ? "" : h`<span class="m mut">root ${root(pub && pub.rootRecorded)}${ARROW}${root(to)}</span>`;
  const tabId = side === SIDE.NEWER ? "move-" + subject + "-" + p.key : id + "-" + side;
  const command = side === SIDE.CLUSTER ? clusterCommand(x, true) : side === SIDE.NEWER ? moveCommand(row) : publishedCommand(subject);
  // Only a side with a change sends a request. The route says "unchanged" for a side that the findings did not name.
  const d = unread || !changed[side] ? null : diffOf(subject, p, side);
  // A side with no change reads as hidden when the pack was not checked, or when the last read of the profile failed in part and the side
  // itself was not the one that failed.
  const hidden = !changed[side] && (p.status === STATUS.NOT_CHECKED || (readErrorOf(subject) && !unread));
  let state = "", flags = "", inner;
  if (hidden) {
    state = statusBadge(STATUS.NOT_CHECKED);
    inner = notCheckedSplash(x);
  } else if (unread) {
    state = badge("y", "Not read");
    inner = splash(TONE.WARN, "This side was not read", "Strata could not read the published version, so it cannot say whether it changed.", "", SIZE.CARD);
  } else if (d === null || (d.state === "ok" && d.data.status === "unchanged")) {
    state = badge("g", "Identical");
    inner = undisturbed(side, sideFacts(x, p, row, changed));
  } else {
    ({flags, inner} = comparePanel(tabId, d, packHash(subject, p), command));
    state = d.state === "ok" ? badge("r", "Changed") : "";
  }
  const head = h`<label for="side" class="small mut">Compare</label>${select}${roots}${state}`;
  return h`<section class="card"><div class="chead"><div class="tb">${head}</div>${flags}</div>${inner}</section>`;
}

// moveCard draws the summary of the move of a pack to its newest version, with the question to Palette.
function moveCard(subject, p, row) {
  const f = row.fold, total = f.settings + f.followsTheVersion + f.repeatsTheLists;
  const unread = moveUnread(subject, p), count = v => unread ? DASH : v;
  const pinned = PINNED[row.pinnedState];
  // A move that changes no value, manifest, image, or chart changes the build of the pack only. The counts would read 0 of 0.
  const buildOnly = !unread && total === 0 && !row.images && !row.charts && !row.flags.includes("AUTHORED");
  const counts = buildOnly ? h`<div><span>Content</span>The same: only the build of the pack changes${row.impactText ? h`. Strata says: ${row.impactText}` : ""}.</div>`
    : h`<div><span>Settings</span>${unread ? DASH : h`<b>${f.settings}</b> of ${total} ${plural(total, "value", "values")}`}</div><div><span>Images</span>${count(row.images)}</div><div><span>Charts</span>${count(row.charts)}</div>`;
  return h`<section class="card"><div class="chead"><h2>Move to ${ref(row.name, row.newest)} <span class="sub">Listed ${day(row.listed)}.</span></h2>${impactCell(subject, p, row)}</div><div class="body">
    <div class="meta mt0">${counts}${row.drops.length ? h`<div><span>Drops</span>${row.drops.join(", ")}</div>` : ""}${row.floating ? h`<div><span>Tag</span>Floating</div>` : ""}</div>
    <div class="small mut mt9">${unread ? "Strata could not compare these versions now, so the counts are not known." : `${impactOf(row.impact)[1]} is a hint from strata's table.`}${pinned ? ` The pinned version is ${pinned} in the registry.` : ""}</div>${askBlock(subject, p)}</div></section>`;
}

/* ---- The card "Check this yourself" ----
   Each row is a question, the command that answers it, and what the page says now. The names are real, and the rows change with
   the page: a pack with a newer version adds the move and the question about the newer versions. */
const copyBadge = (cp, carried) => carried && COPY_KEPT.includes(cp.status) ? statusBadge(STATUS.NOT_CHECKED) : badgeOf(COPY_BADGE, COPY_BADGE[cp.status] ? cp.status : COPY.NOT_READ);
const publishedBadge = (pub, carried) => pub.status === VERIFY.DRIFTED ? statusBadge(STATUS.DRIFT) : pub.status === VERIFY.UNCHANGED && !carried ? statusBadge(STATUS.OK) : statusBadge(STATUS.NOT_CHECKED);

// terminalRows gives the rows for a profile, and for a pack of it when p is not null. row is the move of the pack, or null.
function terminalRows(subject, x, p, row) {
  const pub = x.published, cp = x.clusterCopy, rec = D.cluster.records, carried = readErrorOf(subject);
  const rows = [["Get the record that this check used", recordCommand(x), rec.revision ? h`<span class="m">rev ${rec.revision}</span>` : mut(rec.baseline)]];
  if (pub) {
    const drifting = x.packs.filter(q => q.status === STATUS.DRIFT).length;
    rows.push(["Does Palette still have this version?", `strata verify ${RECORD_FILE}`, publishedBadge(pub, carried)]);
    if (x.who && x.who.length) rows.push(["Who changed it?", `strata verify ${RECORD_FILE} --who`, `${x.who[0].actor}, ${ago(x.who[0].time)} ago`]);
    rows.push(["What changed?", publishedCommand(subject), pub.status === VERIFY.DRIFTED && drifting ? `${drifting} ${plural(drifting, "pack", "packs")}` : publishedBadge(pub, carried)]);
  }
  if (cp) rows.push(["Does this cluster still run the record?", clusterCommand(x, false), copyBadge(cp, carried)]);
  if (p && row) {
    rows.push(["What does the move change?", moveCommand(row), impactCell(subject, p, row)]);
    rows.push(["Which newer versions does Palette list?", pinCommand(p), h`<span class="mut">Newest ${ref(row.name, row.newest)}</span>`]);
  }
  return rows;
}

// checkCard is closed at first: most readers trust the page, and the commands are for a person who wants the proof. Each check is a
// question with what the page says now, then the command that answers it in a terminal. An open card stays open on the next page.
function checkCard(rows) {
  return h`<section class="card"><details class="check" data-open="check"${openAttr("check")}><summary><h2>Check this yourself</h2><span class="sub">${rows.length} ${plural(rows.length, "command gives", "commands give")} the same answers in a terminal</span></summary>
    <ul class="checks">${rows.map(([q, c, a]) => h`<li><div class="qa"><span class="q">${q}</span><span class="a">${a}</span></div>${cmd(c)}</li>`)}</ul></details></section>`;
}

function viewPack(subject, layer, name) {
  const x = profileOf(subject), p = x ? x.packs.find(q => q.layer === layer && q.name === name) : null;
  const cr = crumbs(h`<a href="#profiles">Profiles</a>`, h`<a href="${profileHash(subject)}">${subjectRef(subject)}</a>`, p ? ref(name, p.version) : name);
  if (!p) {
    return viewMissing(cr, "profiles", x ? {title: "This pack is not here", name, why: "is not in this profile now.", home: [profileHash(subject), "This profile"]}
      : {title: "This profile is not here", name: subject, why: "is in no record now. A new revision of the records can remove it.", home: ["#profiles", "All profiles"]});
  }
  const row = newerRowOf(subject, p);
  const extra = p.status === STATUS.DRIFT && p.newer ? statusBadge(STATUS.NEWER) : "";
  return {nav: "profiles", crumbs: cr, body: h`${alertBars()}<section class="card"><div class="phead"><div class="body"><div class="title"><h1 class="m h1m">${ref(p.name, p.version)}</h1>${statusBadge(p.status)}${extra}</div>
    <div class="meta"><div><span>Layer</span>${p.layer}</div><div><span>Registry</span>${orNone(p.registry)}</div><div><span>Kind</span>${orNone(p.kind)}</div><div><span>Upstream</span><span class="m bright">${p.upstream ? digest(p.upstream) : "none"}</span></div><div><span title="${IMAGES_LISTED}">Images listed</span>${imagesListed(p.images)}</div><div><span>Profile</span><a class="m" href="${profileHash(subject)}">${subjectRef(subject)}</a></div></div></div>
    <div class="pside">${packStack(subject, x.packs, true, p.key)}</div></div></section>${row ? moveCard(subject, p, row) : ""}${comparePack(subject, x, p, row)}${checkCard(terminalRows(subject, x, p, row))}`};
}

/* ---- The alerts ----
   The page only reads the rules. A file in Git holds them, and no route of the observer writes. */
// alertsOffSplash is the offer of the alerts. Off is a choice of the team, so it reads as an offer and not as a fault.
const alertsOffSplash = () => splash(TONE.OFF, "Nobody gets paged yet",
  "This install has no alert rules. A message goes to a webhook when a finding opens and when it clears. The rules are a file in Git, so a mute always has a name and an end date.",
  next(ALERTS_NEXT), SIZE.CARD);

// digestWords says when a digest goes. The API gives the period in hours.
const digestWords = every => every === WEEK ? "Weekly digest" : `Digest every ${every}`;

// ruleSend says when a rule sends. A mute says until when it holds, or that it ended.
function ruleSend(r) {
  if (r.muteUntil) return r.state === ALERT_STATE.MUTED ? `Muted until ${r.muteUntil}` : `Mute ended ${r.muteUntil}`;
  return r.send === ALERT_SEND.DIGEST ? digestWords(D.alerts.digestEvery) : "At once";
}

// ruleTarget names what a rule applies to: the cluster, a profile, or a pack.
const ruleTarget = r => r.level === "cluster" ? clusterName() : r.match;

const ruleRow = r => h`<tr><td>${badgeOf(ALERT_BADGE, r.state)}</td><td>${ALERT_LEVEL[r.level] || r.level}</td><td class="m">${ruleTarget(r)}</td><td>${ALERT_NEWS[r.news] || r.news}</td><td>${ruleSend(r)}</td><td class="m">${r.to || DASH}</td></tr>`;

function alertsBody() {
  const a = D.alerts;
  if (!a.set) return h`<section class="card"><div class="chead"><h2>Alert rules <span class="sub">Not set</span></h2></div>${alertsOffSplash()}</section>`;
  const waiting = a.waiting ? h`<div class="alertbar a">${a.waiting} ${plural(a.waiting, "message waits", "messages wait")} for a retry. <span>The log names the channel and the status. It never holds the address.</span></div>` : "";
  return h`${waiting}<section class="card"><div class="chead"><h2>Alert rules <span class="sub">${a.rules.length}</span></h2></div>
    <div class="tw"><table><thead><tr><th>State</th><th>Level</th><th>Applies to</th><th>News</th><th>Send</th><th>Where</th></tr></thead><tbody>${a.rules.map(ruleRow)}</tbody></table></div></section>
    <section class="card"><div class="chead"><h2>Sample message</h2></div><div class="pre mt0">${a.sample}</div></section>`;
}

function viewAlerts() {
  return {nav: "alerts", crumbs: crumbs("Alerts"), body: h`${alertBars()}<div class="alertbar b">One message when a finding opens, and one when it clears. <span>A lower level overrides a higher one. A mute always has an end date. Messages hold no values.</span></div>${alertsBody()}`};
}

/* ---- Routes, rendering, events ---- */
function route() {
  const hash = (location.hash || "#overview").slice(1);
  try {
    if (hash === "findings") return viewFindings();
    if (hash === "profiles") return viewProfiles();
    if (hash === "newer") return viewNewer();
    if (hash === "notchecked") return viewNotChecked();
    if (hash === "alerts") return viewAlerts();
    if (hash === "images") return viewImages();
    if (hash.startsWith("i-")) return viewImage(decodeURIComponent(hash.slice(2)));
    if (hash.startsWith("p-")) return viewProfile(decodeURIComponent(hash.slice(2)));
    if (hash.startsWith("k-")) {
      const [subject, layer, name] = hash.slice(2).split("/").map(decodeURIComponent);
      return viewPack(subject, layer, name);
    }
  } catch (err) {
    if (!(err instanceof URIError)) throw err;
  }
  return viewOverview();
}

// put writes markup to one place of the page. It is the one write of markup in this file.
function put(el, markup) {
  el.innerHTML = part(markup);
}

function render() {
  const main = document.getElementById("app");
  if (!D) {
    put(main, S.offline ? offlineSplash() : splash(TONE.BUILD, "Asking the observer", "The page reads the first answers of the observer. It fills in when they arrive."));
    return;
  }
  const active = document.activeElement;
  const typing = active && /^q(-|$)/.test(active.id || "") ? {id: active.id, start: active.selectionStart, end: active.selectionEnd} : null;
  const v = route();
  put(document.getElementById("navl"), navLinks(v.nav));
  const c = D.cluster.cluster;
  put(document.getElementById("ctx"), h`Cluster<b>${c.name || c.uid || "-"}</b>`);
  put(document.getElementById("top"), topBar(v.crumbs));
  put(main, v.body);
  if (typing) {
    const q = document.getElementById(typing.id);
    if (q) { q.focus(); q.setSelectionRange(typing.start, typing.end); }
  }
}

// jumpTo scrolls to an element of the page. The hash stays as it is, so the screen does not change.
function jumpTo(id) {
  const el = document.getElementById(id);
  if (el) el.scrollIntoView();
}

// copyText puts the command of a button on the clipboard. A browser that refuses leaves the person to select the command.
async function copyText(button) {
  try {
    await navigator.clipboard.writeText(button.dataset.copy);
    button.textContent = "Copied";
  } catch (err) {
    button.textContent = "Select it";
  }
  setTimeout(() => { button.textContent = "Copy"; }, 1400);
}

function onClick(ev) {
  const t = ev.target.closest("[data-tab]");
  if (t) { S.tabs[t.dataset.tab] = t.dataset.k; render(); return; }
  const f = ev.target.closest("[data-filter]");
  if (f) { S.filter = f.dataset.filter; render(); return; }
  const v = ev.target.closest("[data-view]");
  if (v) { S.view = v.dataset.view === "split" ? "split" : "unified"; render(); return; }
  const so = ev.target.closest("[data-sort]");
  if (so) {
    const t = so.dataset.sort, c = so.dataset.col, cur = S.sort[t];
    S.sort[t] = !cur || cur.col !== c ? {col: c, dir: 1} : cur.dir > 0 ? {col: c, dir: -1} : undefined;
    render();
    return;
  }
  const q = ev.target.closest("[data-ask]");
  if (q) { askPalette(q.dataset.ask, q.dataset.pack); return; }
  const j = ev.target.closest("[data-jump]");
  if (j) { jumpTo(j.dataset.jump); return; }
  const r = ev.target.closest("[data-retry]");
  if (r) { if (r.dataset.retry === "page") refresh(); else render(); return; }
  const c = ev.target.closest("[data-copy]");
  if (c) { copyText(c); return; }
  const x = ev.target.closest("[data-clear]");
  if (x) { S.filter = "all"; S.q = ""; render(); return; }
  const g = ev.target.closest("[data-go]");
  if (g && !ev.target.closest("a,button,select,input")) location.hash = g.dataset.go;
}

function start() {
  document.addEventListener("click", onClick);
  document.addEventListener("input", ev => {
    if (ev.target.id === "q") { S.q = ev.target.value.trim(); render(); }
    else if (ev.target.dataset && ev.target.dataset.q) { S.qs[ev.target.dataset.q] = ev.target.value.trim(); render(); }
  });
  document.addEventListener("change", ev => {
    if (ev.target.id === "side") { S.side[ev.target.dataset.id] = ev.target.value; render(); }
  });
  document.addEventListener("toggle", ev => {
    const k = ev.target.dataset && ev.target.dataset.open;
    if (k) S.open[k] = ev.target.open;
  }, true);
  addEventListener("hashchange", () => { render(); scrollTo(0, 0); });
  // A turn of a phone, or a window made narrow, changes the layout of a diff.
  if (typeof matchMedia === "function") matchMedia(NARROW).addEventListener("change", render);
  render();
  refresh();
  setInterval(refresh, REFRESH_MS);
}

start();
