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
const PAGE = {OK: "ok", FIRST_LOAD: "first-load", NO_RECORDS: "no-records", STALE: "stale", LOST: "lost"};
const BASELINE = {APPROVED: "approved", FIRST_SEEN: "first-seen", UNKNOWN: "unknown"};
const STATUS = {DRIFT: "drift", NEWER: "newer", NOT_CHECKED: "not-checked", OK: "ok", NO_RECORD: "no-record"};
const VERIFY = {UNCHANGED: "unchanged", DRIFTED: "drifted", ERROR: "error"};
const COPY = {SAME: "same", SAME_CONTENT: "same-content", DIFFERS: "differs", NOT_ON_CLUSTER: "not-on-this-cluster", NOT_READ: "not-read"};
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
// The two layouts of a diff, and the note of a last line that has no end of line.
const VIEWS = [["unified", "Unified"], ["split", "Split"]];
const NO_EOL = "\\ No newline at end of file";

// The color class and the word of a status, a kind of news, and an impact level.
const BADGE = {
  [STATUS.DRIFT]: ["r", "Drift"], [STATUS.NEWER]: ["b", "Newer"], [STATUS.NOT_CHECKED]: ["y", "Not checked"],
  [STATUS.OK]: ["g", "OK"], [STATUS.NO_RECORD]: ["y", "No record"],
};
const EVENT_BADGE = {
  [EVENT.DRIFT]: ["r", "Drift"], [EVENT.NEWER]: ["b", "Newer"], [EVENT.NOT_CHECKED]: ["y", "Not checked"], [EVENT.INFO]: ["y", "Info"],
};
const IMPACT = {
  "repave": ["a", "Repave"], "may-repave": ["a", "May repave"], "cluster-wide": ["b", "Cluster-wide"],
  "in-place": ["y", "In place"], "sync": ["y", "Sync"], "none": ["y", "None"],
};
const SEVERITY = [STATUS.DRIFT, STATUS.NEWER, STATUS.NOT_CHECKED, STATUS.NO_RECORD, STATUS.OK];
const FLAGS = ["IDENTITY", "UPSTREAM", "AUTHORED", "IMAGES", "CHARTS"];
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
const ERROR_TEXT = {
  api: status => status === 401 || status === 403 ? `HTTP ${status}, the API key was refused` : `HTTP ${status}, Palette gave an error`,
  transport: () => "the link to Palette failed",
  "not-json": () => "Palette sent an answer that is not JSON",
  timeout: () => "Palette did not answer in time",
  "kube-api": status => status === 401 || status === 403 ? `HTTP ${status}, the role of the observer was refused` : `HTTP ${status}, the API server gave an error`,
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
const S = {filter: "all", q: "", tabs: {}, side: {}, open: {}, diffs: {}, view: "unified", asking: {}, askFailed: {}, offline: false, loadedAt: null};

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
    if (!r.ok) return {state: "error"};
    return {state: "ok", data: await r.json()};
  } catch (err) {
    return {state: "error"};
  }
}

/* ---- Small helpers ---- */
const enc = s => encodeURIComponent(s).replace(/%40/g, "@");
const subjectOf = p => `${p.name}@${p.version}`;
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
const podsRead = () => D.images.pods === PODS.READ;
const imageNum = n => podsRead() ? n : "-";
const NEEDS_RECORD = "Strata needs a record to look for newer versions. A record is the copy your team approved. Your pipeline writes records from Git.";
const NOT_READ_YET = "No check has ended well, so this page has nothing to show yet.";
// noChecksWords says why the page has no newer version to show.
const noChecksWords = () => D.cluster.page === PAGE.NO_RECORDS ? NEEDS_RECORD : NOT_READ_YET;
// noNewerWords says why the list of newer versions is empty.
const noNewerWords = () => D.cluster.counts.notChecked ? "No newer version is listed." : `All ${D.cluster.counts.packs} packs are current.`;
const asCount = st => st === STATUS.NO_RECORD ? STATUS.NOT_CHECKED : st;
const packName = key => key.slice(key.indexOf("/") + 1);
const profileOf = subject => D.profiles.find(p => subjectOf(p) === subject);
const impactOf = i => IMPACT[i] || ["y", i];

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
const openAttr = k => S.open[k] ? raw(" open") : "";
const flagRow = on => h`<span class="tb">${FLAGS.map(f => h`<span class="badge ${on.includes(f) ? "b" : "y off"}">${f}</span>`)}</span>`;
const root = r => r ? h`<span title="${r}">${hex12(r)}</span>` : "";

// newerRowOf finds the row of /v1/newer that a pack of a profile belongs to.
const newerRowOf = (subject, p) => D.newer.find(r => r.profiles.includes(subject) && r.name === p.name && r.version === p.version && r.registry === p.registry);
// packOfRow finds the pack of a profile that a row of /v1/newer moves. A profile that is not known gives null.
const packOfRow = (profile, r) => profile ? profile.packs.find(q => q.name === r.name && q.version === r.version) : null;
// moveUnread says that strata could not compare the two versions of a pack. The row has no counts, and Not checked names the pack.
const moveUnread = (subject, p) => !!p && D.notChecked.some(r => r.cause === CAUSE.PIN_ERROR && r.profiles.includes(subject) && r.packs.includes(p.key));
const impactCell = (subject, p, row) => moveUnread(subject, p) ? badge("y", "Not read") : impactBadge(row.impact);
// worstOf gives the status that wins in a list of statuses.
const worstOf = list => SEVERITY.find(s => list.includes(s)) || STATUS.OK;

/* ---- The question to Palette ----
   Only a click on a button sends a question: nothing asks on a timer, and no render asks. One answer is for
   one move: the move of a pack of a profile to the newest version. */
const askId = (subject, p) => `${subject}|${p.key}`;
// answerOf finds the answer of Palette for a move. A move that nobody asked about has none.
const answerOf = (subject, p, row) => D.asks.find(a => a.profile === subject && a.key === p.key && a.newest === row.newest);
const isAnswer = a => !!a && ["profile", "key", "newest", "verdict"].every(k => typeof a[k] === "string");
// verdictOf reads the verdict of an answer. A word that the page does not know reads as no answer, and the page claims nothing.
const verdictOf = a => a.verdict === VERDICT.NO_ERROR || a.verdict === VERDICT.REFUSES ? a.verdict : VERDICT.NO_ANSWER;

// askButton draws the button of a move. It is off while the question is on its way.
function askButton(cls, subject, p, label) {
  const asking = S.asking[askId(subject, p)];
  return h`<button class="${cls}" data-ask="${subject}" data-pack="${p.key}"${asking ? raw(" disabled") : ""}>${asking ? "Asking" : label}</button>`;
}

// askCell is the cell "Palette" of a row of the table of newer versions.
function askCell(subject, p, row) {
  const a = answerOf(subject, p, row);
  const failed = S.askFailed[askId(subject, p)] ? h`<div class="small mut askmsg">${ASK_FAILED}</div>` : "";
  if (!a) return h`${askButton("lnk", subject, p, "Ask Palette")}${failed}`;
  const verdict = verdictOf(a);
  if (verdict === VERDICT.NO_ANSWER) return h`<span class="mut">${ASK_ROW[verdict]}</span> ${askButton("lnk", subject, p, "Ask again")}${failed}`;
  return h`<span class="fw">${ASK_ROW[verdict]}</span>`;
}

// refusalLines draws one error that Palette gives: the code, the message, and the clusters.
const refusalLines = r => h`<div class="mt3"><b>Palette refuses:</b> <span class="m">${r.code}</span></div>${r.message ? h`<div>${r.message}</div>` : ""}${r.clusters && r.clusters.length ? h`<div class="mut">on ${r.clusters.join(", ")}</div>` : ""}`;

// askBlock is the part of the card "Move to" that asks Palette, or that shows its answer.
function askBlock(subject, p, row) {
  const a = answerOf(subject, p, row);
  const failed = S.askFailed[askId(subject, p)] ? h`<div class="small mut mt6">${ASK_FAILED}</div>` : "";
  if (!a) return h`<div class="mt10">${askButton("btn", subject, p, "Ask Palette about this move")}${failed}</div>`;
  switch (verdictOf(a)) {
    case VERDICT.NO_ERROR:
      return h`<div class="note small mt10"><b>Palette gave no error.</b> The page "behind" in the book says what this answer covers.</div>`;
    case VERDICT.REFUSES: {
      const list = a.refusals || [];
      return h`<div class="note a small mt10">${list.length ? list.map(refusalLines) : h`<div><b>Palette refuses.</b></div>`}<div class="mt6">Nothing on this cluster changed.</div></div>`;
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
    D.asks = [...D.asks.filter(a => !(a.profile === answer.profile && a.key === answer.key && a.newest === answer.newest)), answer];
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

function navLinks(cur) {
  const c = D.cluster, n = c.counts;
  const items = [["overview", "Overview", ""], ["findings", "Findings", h`<span class="cnt ${n.findings ? "r" : ""}">${num(n.findings)}</span>`],
    ["profiles", "Profiles", h`<span class="cnt">${known(n.profiles)}</span>`], ["newer", "Newer versions", h`<span class="cnt">${num(n.newer)}</span>`],
    ["images", "Images", h`<span class="cnt">${imageNum(D.images.counts.images)}</span>`],
    ["notchecked", "Not checked", h`<span class="cnt">${num(n.notChecked)}</span>`], ["alerts", "Alerts", alertsCount()]];
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

function topBar(crumbs) {
  const r = D.cluster.records;
  let pill = "", rec = "";
  if (r.baseline === BASELINE.FIRST_SEEN) pill = badge("a", "First-seen records");
  else if (r.baseline === BASELINE.UNKNOWN) pill = badge("y", "Baseline unknown");
  else if (r.revision) rec = h`<span class="chip">records: <b>rev ${r.revision}</b></span>`;
  return h`<nav class="crumbs" aria-label="Where you are">${crumbs}</nav><div class="topr">${pill}${rec}${clocks()}</div>`;
}

const crumbs = (...a) => a.map((x, i) => i === a.length - 1 ? h`<b>${x}</b>` : h`${x} <span>/</span> `);

function alertBars() {
  const c = D.cluster, bars = [];
  const content = c.content;
  if (S.offline) {
    bars.push(h`<div class="alertbar a">The page cannot reach the observer. <span>It shows the data of ${clock(S.loadedAt)}. It tries again each 30 seconds.</span></div>`);
  }
  const counts = content.checkedAt ? h`Counts below are from ${clock(content.checkedAt)}` : h`No check has ended well`;
  const failed = content.failedRuns > 0 ? h`The last ${content.failedRuns} ${plural(content.failedRuns, "check", "checks")} failed${content.lastError ? h`: ${errorWords(content.lastError)}` : ""}. ` : "";
  switch (c.page) {
    case PAGE.LOST:
      bars.push(h`<div class="alertbar r">Lost contact with Palette. <span>${failed}${counts}, and no finding clears until a check proves it.</span></div>`);
      break;
    case PAGE.STALE:
      bars.push(h`<div class="alertbar a">Checks stopped. <span>${content.checkedAt ? h`Last content check ${clock(content.checkedAt)}, expected every ${content.every}. ` : ""}${failed}${counts}.</span></div>`);
      break;
    case PAGE.NO_RECORDS:
      bars.push(h`<div class="alertbar b">No records yet. <span>A record is the copy your team approved. Your pipeline writes records from Git.</span></div>`);
      break;
    case PAGE.FIRST_LOAD:
      bars.push(h`<div class="alertbar b">The first check has not ended. <span>This page fills in when it does.</span></div>`);
      break;
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
  const c = D.cluster, n = c.counts, p = c.page, checked = hasChecks();
  const T = (cls, label, value, sub, href) => h`<a class="tile ${cls}" href="#${href}"><div class="l">${label}</div><div class="v">${value}</div><div class="s">${sub}</div></a>`;
  const none = n.notChecked === 0;
  let driftSub, newerSub, notCheckedSub;
  if (!checked) {
    const noRecords = p === PAGE.NO_RECORDS, word = noRecords ? "No records" : "Not read yet";
    driftSub = word; newerSub = noRecords ? "Needs a record" : word; notCheckedSub = word;
  } else {
    driftSub = n.drift === 0 ? (none ? "Nothing changed" : "None in what was checked") : n.drift === 1 ? driftSubject() : `${n.drift} profiles`;
    const fresh = D.newer.filter(r => r.new).length;
    newerSub = n.newer === 0 ? (none ? `All ${n.packs} packs current` : "None listed") : `${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")} · ${fresh} this week`;
    notCheckedSub = n.notChecked === 0 ? `${n.packsInFull} of ${n.packs} packs in full` : `${n.notChecked} ${plural(n.notChecked, "cause", "causes")} · ${n.packsInFull} of ${n.packs} packs in full`;
  }
  const [imageCls, imageValue, imageSub] = imageTile();
  return h`<section class="tiles" aria-label="Summary">
    ${T(!checked ? "y" : n.drift ? "r" : "g", "Drift", num(n.drift), driftSub, "findings")}
    ${T(imageCls, "Image changes", imageValue, imageSub, "images")}
    ${T("b", "Newer versions", num(n.newer), newerSub, "newer")}
    ${T("y", "Not checked", num(n.notChecked), notCheckedSub, "notchecked")}
    ${T("", "Profiles", known(n.profiles), neverChecked() ? "Not read yet" : `${n.packs} packs on this cluster`, "profiles")}
  </section>`;
}

// imageTile gives the color, the value, and the words of the tile "Image changes". Before a read of the
// pods it claims nothing: the value is "-". The Drift tile does not count these changes.
function imageTile() {
  const o = D.images, n = o.counts;
  if (o.pods === PODS.OFF) return ["y", "-", "Pods not read"];
  if (o.pods === PODS.NOT_READ) return ["y", "-", "Not read yet"];
  if (n.changed > 0) return ["a", n.changed, "Moved or split"];
  return ["g", n.changed, `${n.images} ${plural(n.images, "image", "images")} in pods`];
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
  })}</tbody></table></div>` : h`<div class="body mut">${neverChecked() ? "Not read yet." : "Nothing needs attention."}</div>`;
  return h`<section class="card"><div class="chead"><h2>Needs attention <span class="sub">${known(rows.length)}</span></h2><a class="small" href="#findings">All findings</a></div>${body}</section>`;
}

function eventRows(list) {
  return list.map(x => {
    const [c, t] = EVENT_BADGE[x.type] || ["y", x.type];
    return h`<tr><td>${badge(c, t)}</td><td class="m narrow-hide">${x.reason}</td><td class="mut nw">${ago(x.time)}</td><td class="m narrow-hide">${x.object}</td><td>${x.message}<div class="m mut narrow-show">${x.object}</div></td></tr>`;
  });
}

const EVENT_HEAD = h`<thead><tr><th>Type</th><th class="narrow-hide">Reason</th><th>Age</th><th class="narrow-hide">Object</th><th>Message</th></tr></thead>`;
const EVENTS_SHOWN = 8;

function eventsCard() {
  const all = D.events, first = all.slice(0, EVENTS_SHOWN), rest = all.slice(EVENTS_SHOWN);
  const more = rest.length ? h`<details data-open="events" ${openAttr("events")}><summary>${rest.length} more ${plural(rest.length, "event", "events")}</summary><div class="tw"><table>${EVENT_HEAD}<tbody>${eventRows(rest)}</tbody></table></div></details>` : "";
  const body = all.length ? h`<div class="tw"><table>${EVENT_HEAD}<tbody>${eventRows(first)}</tbody></table></div>${more}` : h`<div class="body mut">No events.</div>`;
  return h`<section class="card"><div class="chead"><h2>Recent events</h2><span class="small mut">Last 7 days</span></div>${body}</section>`;
}

function coverageCard() {
  const l = D.limits;
  const row = (title, sub, value) => h`<tr><td>${title}<div class="small mut">${sub}</div></td><td class="n">${value}</td></tr>`;
  const behindATag = podsRead() ? h`<a href="#images">Compared with the first digest seen in pods</a>` : "Strata compares the name and the tag.";
  return h`<section class="card"><div class="chead"><h2>What a check does not cover</h2></div><div class="tw"><table><tbody>
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
    else if (copy === COPY.SAME || copy === COPY.SAME_CONTENT) text += " This cluster keeps its copy.";
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
        return h`${ms[0].name} ${ms[0].version}${ARROW}${ms[0].newer}${row && row.new ? h` <span class="badge fresh">new</span>` : ""}`;
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
  const shown = list.filter(x => has(x, S.filter) && (!q || subjectOf(x).toLowerCase().includes(q)));
  const rows = shown.map(x => {
    const subject = subjectOf(x);
    return h`<tr class="link" data-go="${profileHash(subject)}"><td class="w110">${statusBadge(x.status)}</td><td class="m nw"><a href="${profileHash(subject)}">${x.name}</a><span class="mut">@${x.version}</span></td><td class="mut narrow-hide">${x.type}</td><td class="narrow-hide nw">${hbar(x)}</td><td class="msg">${newsOf(x)}</td></tr>`;
  });
  const empty = h`<tr><td colspan="5" class="mut">${D.profiles.length ? "No profile matches." : "No profile yet."}</td></tr>`;
  return h`<section class="card"><div class="chead"><h2>Profiles <span class="sub">${neverChecked() ? "Not read yet" : `${D.cluster.counts.profiles} profiles, ${D.cluster.counts.packs} packs`}</span></h2>
    <div class="tb"><div class="seg" role="group" aria-label="Filter by status">${FILTERS.map(([k, l]) => h`<button data-filter="${k}" class="${S.filter === k ? "on" : ""}">${l} ${num(cnt[k])}</button>`)}</div>
    <input class="search" id="q" type="search" placeholder="Filter by name" aria-label="Filter by name" value="${S.q}"></div></div>
    <div class="tw"><table id="pt"><thead><tr><th>Status</th><th>Profile</th><th class="narrow-hide">Type</th><th class="narrow-hide">Packs</th><th>Message</th></tr></thead><tbody>${rows.length ? rows : empty}</tbody></table></div></section>`;
}

/* ---- The screens ---- */
function viewOverview() {
  return {nav: "overview", crumbs: crumbs("Overview"), body: h`${alertBars()}${tiles()}<div class="grid2"><div class="stack">${findingsCard()}${eventsCard()}</div>${coverageCard()}</div>${profilesCard()}`};
}

function viewProfiles() {
  return {nav: "profiles", crumbs: crumbs("Profiles"), body: h`${alertBars()}${profilesCard()}`};
}

function viewFindings() {
  const rows = D.newer;
  const card = checked => {
    if (!checked) return h`<div class="body mut">${noChecksWords()}</div>`;
    if (!rows.length) return h`<div class="body mut">${noNewerWords()}</div>`;
    return h`<div class="tw"><table><tbody>${rows.map(r => {
      const first = r.profiles[0], k = packOfRow(first ? profileOf(first) : null, r);
      const href = k ? packHash(first, k) : first ? profileHash(first) : "#newer";
      return h`<tr class="link" data-go="${href}"><td class="w90">${statusBadge(STATUS.NEWER)}</td><td class="m"><a href="${href}">${r.name} ${r.version}${ARROW}${r.newest}</a></td><td>${r.profiles.map(s => lab("profile", s))}</td><td>${impactCell(first, k, r)}</td></tr>`;
    })}</tbody></table></div>`;
  };
  const count = hasChecks() ? rows.length : "-";
  return {nav: "findings", crumbs: crumbs("Findings"), body: h`${alertBars()}${findingsCard()}
    <section class="card"><div class="chead"><h2>Newer versions <span class="sub">${count}. A choice, not a fault, so they stay out of Needs attention.</span></h2><a class="small" href="#newer">Open the table</a></div>${card(hasChecks())}</section>`};
}

function viewNewer() {
  const n = D.cluster.counts, rows = D.newer;
  let body;
  if (!hasChecks()) {
    body = h`<div class="body mut">${noChecksWords()}</div>`;
  } else if (!rows.length) {
    body = h`<div class="body mut">${noNewerWords()}</div>`;
  } else {
    const groups = [];
    rows.forEach(r => r.profiles.forEach(s => {
      let g = groups.find(q => q.subject === s);
      if (!g) { g = {subject: s, rows: []}; groups.push(g); }
      g.rows.push(r);
    }));
    groups.sort((a, b) => a.subject.localeCompare(b.subject));
    body = h`<div class="body small mut pb9"><b>Settings</b> counts what a move really changes. Impact is a hint from strata's table, so read the two together.</div>
      <div class="tw"><table><thead><tr><th>Pack</th><th>Move</th><th>Impact</th><th class="n">Settings</th><th class="n narrow-hide">Images</th><th class="n narrow-hide">Charts</th><th class="narrow-hide">Listed</th><th class="narrow-hide">Palette</th></tr></thead><tbody>${groups.map(g => {
        const p = profileOf(g.subject);
        const total = p ? p.packs.length : "";
        return [h`<tr class="g"><td colspan="8"><a class="m" href="${profileHash(g.subject)}">${g.subject}</a> <span class="mut">· ${g.rows.length}${total !== "" ? h` of ${total}` : ""} ${plural(total === "" ? g.rows.length : total, "pack", "packs")}</span></td></tr>`,
          g.rows.map(r => {
            const k = packOfRow(p, r);
            const href = k ? packHash(g.subject, k) : profileHash(g.subject);
            const count = v => moveUnread(g.subject, k) ? DASH : v;
            return h`<tr class="link" data-go="${href}"><td class="m"><a href="${href}">${r.name}</a></td><td class="m nw">${r.version}${ARROW}${r.newest}</td><td>${impactCell(g.subject, k, r)}</td><td class="n"><b>${count(r.fold.settings)}</b></td><td class="n narrow-hide">${count(r.images)}</td><td class="n narrow-hide">${count(r.charts)}</td><td class="m mut narrow-hide nw">${day(r.listed)}${r.new ? h` <span class="badge fresh">new</span>` : ""}</td><td class="narrow-hide">${k ? askCell(g.subject, k, r) : ""}</td></tr>`;
          })];
      })}</tbody></table></div>`;
  }
  const others = packsWithoutMove();
  const sub = hasChecks() && rows.length ? h`<span class="sub">${rows.length} in ${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")}. ${others.current} other ${plural(others.current, "pack is", "packs are")} current.${others.unchecked ? ` ${others.unchecked} ${plural(others.unchecked, "is", "are")} not checked.` : ""}</span>` : h`<span class="sub">${num(rows.length)}</span>`;
  return {nav: "newer", crumbs: crumbs("Newer versions"), body: h`${alertBars()}<section class="card"><div class="chead"><h2>Newer versions ${sub}</h2></div>${body}</section>`};
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
  const rows = D.notChecked;
  let body;
  if (D.cluster.page === PAGE.FIRST_LOAD) {
    body = h`<div class="body mut">The first check has not ended yet.</div>`;
  } else if (neverChecked()) {
    body = h`<div class="body mut">${NOT_READ_YET}</div>`;
  } else if (!rows.length) {
    body = h`<div class="body mut">Nothing. All ${D.cluster.counts.packs} packs were checked in full.</div>`;
  } else {
    body = h`<div class="tw"><table><thead><tr><th>What</th><th>Profile</th><th>Why</th></tr></thead><tbody>${rows.map(r => {
      const first = r.profiles[0];
      const p = first ? profileOf(first) : null;
      const names = r.packs.map(k => {
        const found = D.profiles.filter(x => r.profiles.includes(subjectOf(x))).flatMap(x => x.packs).find(q => q.key === k);
        return found ? `${found.name} ${found.version}` : packName(k);
      });
      const what = [...new Set(names)].join(", ");
      const why = r.cause === CAUSE.OLD_COPY && r.registry ? `Current against an older copy of ${r.registry}. A newer version can be missing here.` : (CAUSE_TEXT[r.cause] || r.cause);
      const pods = r.cause === CAUSE.PODS_NOT_READ;
      return h`<tr class="link" data-go="${pods ? "#images" : p ? profileHash(first) : "#profiles"}"><td class="m">${what || mut(pods ? "The pods" : "The profile")}</td><td class="m">${r.profiles.length ? r.profiles.join(", ") : mut("-")}</td><td>${why}</td></tr>`;
    })}</tbody></table></div>`;
  }
  const sub = hasChecks() ? h`${rows.length}. This never raises a severity, and it stops the page from saying "No change".` : h`${num(rows.length)}`;
  return {nav: "notchecked", crumbs: crumbs("Not checked"), body: h`${alertBars()}<section class="card"><div class="chead"><h2>Not checked on this run <span class="sub">${sub}</span></h2></div>${body}</section>${coverageCard()}`};
}

/* The two sides of a profile: the published version, and the copy of this cluster in Palette. */
function verdict(x) {
  const pub = x.published, cp = x.clusterCopy;
  let pubCard;
  if (!pub) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${badge("y", "No record")}</div><div class="small mut mt6">Nothing to compare with.</div></div>`;
  } else if (pub.status === VERIFY.DRIFTED) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${statusBadge(STATUS.DRIFT)}</div><div class="m mt6">root <span class="was">${root(pub.rootRecorded)}</span>${ARROW}<span class="now">${root(pub.rootLive)}</span></div></div>`;
  } else if (pub.status === VERIFY.UNCHANGED) {
    pubCard = h`<div class="vcard"><div class="h">Published version ${statusBadge(STATUS.OK)}</div><div class="m mt6">root ${root(pub.rootRecorded)}</div><div class="small mut mt3">Unchanged since the record</div></div>`;
  } else {
    pubCard = h`<div class="vcard"><div class="h">Published version ${statusBadge(STATUS.NOT_CHECKED)}</div><div class="m mt6">root ${root(pub.rootRecorded)}</div><div class="small mut mt3">Strata could not read this version.</div></div>`;
  }
  let copyCard;
  if (!cp) {
    copyCard = h`<div class="vcard"><div class="h">This cluster's copy in Palette ${badge("y", "No record")}</div><div class="small mut mt6">Nothing to compare with.</div></div>`;
    return h`<div class="vs">${pubCard}${copyCard}</div>`;
  }
  const head = b => h`<div class="h">This cluster's copy in Palette ${b}</div>`;
  switch (cp.status) {
    case COPY.SAME:
      copyCard = h`<div class="vcard">${head(statusBadge(STATUS.OK))}<div class="m mt6">root ${root(cp.rootB)}</div><div class="small mut mt3">Unchanged since the record</div></div>`; break;
    case COPY.SAME_CONTENT:
      copyCard = h`<div class="vcard">${head(badge("g", "Same content"))}<div class="m mt6">root ${root(cp.rootB)}</div><div class="small mut mt3">The roots differ and the content does not. This is not a drift.</div></div>`; break;
    case COPY.DIFFERS:
      copyCard = h`<div class="vcard">${head(statusBadge(STATUS.DRIFT))}<div class="m mt6">root ${root(pub && pub.rootRecorded)}${ARROW}<span class="now">${root(cp.rootB)}</span></div></div>`; break;
    case COPY.NOT_ON_CLUSTER:
      copyCard = h`<div class="vcard">${head(badge("y", "Not on this cluster"))}<div class="small mut mt6">This cluster does not run this version.</div></div>`; break;
    default:
      copyCard = h`<div class="vcard">${head(badge("y", "Not read"))}<div class="small mut mt6">Strata could not read this copy.</div></div>`;
  }
  return h`<div class="vs">${pubCard}${copyCard}</div>`;
}

function viewProfile(subject) {
  const x = profileOf(subject);
  if (!x) return viewMissing(crumbs(h`<a href="#profiles">Profiles</a>`, subject), "profiles");
  const r = D.cluster.records;
  const record = x.status === STATUS.NO_RECORD ? "none" : r.revision ? `rev ${r.revision}` : r.baseline;
  const fs = D.findings.filter(f => f.profile === subject);
  const ev = D.events.filter(q => q.object.split(", ").includes(subject));
  const events = ev.length ? h`<section class="card"><div class="chead"><h2>Events</h2></div><div class="tw"><table><thead><tr><th>Type</th><th class="narrow-hide">Reason</th><th>Age</th><th class="narrow-hide">From</th><th>Message</th></tr></thead><tbody>${ev.map(q => {
    const [c, t] = EVENT_BADGE[q.type] || ["y", q.type];
    return h`<tr><td>${badge(c, t)}</td><td class="m narrow-hide">${q.reason}</td><td class="mut nw">${ago(q.time)}</td><td class="mut narrow-hide">${eventFrom(q)}</td><td>${q.message}</td></tr>`;
  })}</tbody></table></div></section>` : "";
  const advice = fs.some(f => f.kind === KIND.PUBLISHED && !f.alsoOnCluster) ? h`<div class="note r small"><b>What to do.</b> Restore the version first. To change a release, publish a new version and record it. A new cluster gets this edit at once.${D.cluster.cluster.updatesPending ? " An update waits for this cluster. Do not accept it until the content is what the team approved." : ""}</div>` : "";
  const rows = x.packs.map(p => {
    const href = packHash(subject, p);
    return h`<tr class="link" data-go="${href}"><td class="w110">${statusBadge(p.status)}</td><td class="m"><a href="${href}">${p.name}</a></td><td class="mut narrow-hide">${p.layer}</td><td class="m">${p.version}</td><td class="m">${newerCell(p)}</td><td class="n narrow-hide">${p.images}</td></tr>`;
  });
  const packs = x.packs.length ? h`<div class="tw"><table><thead><tr><th>Status</th><th>Pack</th><th class="narrow-hide">Layer</th><th>Version</th><th>Newer</th><th class="n narrow-hide">Images</th></tr></thead><tbody>${rows}</tbody></table></div>` : h`<div class="body mut">${x.status === STATUS.NO_RECORD ? "No record holds this profile, so the packs are not listed." : "No pack."}</div>`;
  return {nav: "profiles", crumbs: crumbs(h`<a href="#profiles">Profiles</a>`, subject), body: h`${alertBars()}
    <section class="card"><div class="body"><div class="title"><h1 class="m h1m">${subject}</h1>${statusBadge(x.status)}</div>
    <div class="meta"><div><span>Type</span>${x.type}</div><div><span>Scope</span>${x.scope || "-"}</div><div><span>Packs</span>${x.packs.length}</div><div><span>Record</span>${record}</div></div></div></section>
    ${verdict(x)}${events}${advice}
    <section class="card"><div class="chead"><h2>Packs <span class="sub">${x.packs.length}</span></h2></div>${packs}</section>
    <section class="card"><div class="chead"><h2>At a terminal</h2></div><div class="pre mt0">strata verify RECORD --who
strata compare RECORD cluster:${D.cluster.cluster.name || D.cluster.cluster.uid}/${x.name}</div></section>`};
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

function viewMissing(cr, nav) {
  return {nav, crumbs: cr, body: h`${alertBars()}<section class="card"><div class="body mut">This page is not in the data. It is gone, or the observer has not read it yet.</div></section>`};
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

const PODS_OFF_WORDS = h`<div>The observer does not read pods. The read is opt-in.</div>
  <div class="mt6">A <span class="m">list</span> call on pods gives the whole spec of each pod to the holder of the token. So a team must ask for it.</div>
  <div class="mt6">The page "The observer" in the book says how to turn it on.</div>`;

// imagesBody gives the body of the card of the list, from the state of the read of the pods.
function imagesBody() {
  const o = D.images;
  if (o.pods === PODS.OFF) return h`<div class="body">${PODS_OFF_WORDS}</div>`;
  if (o.pods === PODS.NOT_READ) {
    const why = o.error ? h`The last read failed: ${podsErrorWords(o.error)}.` : "No read has ended yet.";
    return h`<div class="body"><div>The observer could not read the pods yet.</div><div class="mt6 mut">${why}</div></div>`;
  }
  if (!o.images.length) return h`<div class="body mut">No pod runs an image.</div>`;
  return h`<div class="tw"><table><thead><tr><th>Status</th><th>Image</th><th class="narrow-hide">Pack</th><th class="n narrow-hide">Pods</th><th>Digest</th></tr></thead><tbody>${o.images.map(imageRow)}</tbody></table></div>`;
}

function viewImages() {
  const o = D.images, n = o.counts, rest = n.images - n.matched;
  const sub = podsRead() ? h`${n.images} ${plural(n.images, "image", "images")}. ${n.matched} ${plural(n.matched, "matches", "match")} a pack. ${rest} ${plural(rest, "does", "do")} not.` : o.pods === PODS.OFF ? "Pods not read" : "Not read yet";
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
  if (!img) return viewMissing(cr, "images");
  const id = "image-" + ref, tab = S.tabs[id] || "this";
  const profiles = [...new Set(img.packs.map(p => p.profile))];
  const packs = img.packs.length ? img.packs.map(p => lab("pack", packName(p.key))) : mut("none");
  const links = profiles.length ? profiles.map(s => h`<a class="m" href="${profileHash(s)}">${s}</a> `) : mut("none");
  const fix = IMAGE_NOTE[img.status] ? h`<section class="card"><div class="chead"><h2>The fix</h2></div><div class="body">The use case "A vendor rebuilds an image under one tag" in the book shows the fix.</div></section>` : "";
  return {nav: "images", crumbs: cr, body: h`${alertBars()}
    <section class="card"><div class="body"><div class="title"><h1 class="m h1m">${ref}</h1>${imageBadge(img.status)}</div>
    <div class="meta"><div><span>${img.packs.length > 1 ? "Packs" : "Pack"}</span>${packs}</div><div><span>${profiles.length > 1 ? "Profiles" : "Profile"}</span>${links}</div><div><span>Named by</span>${img.byDigest ? "a digest" : "a tag"}</div><div><span>Match</span>${MATCH_WORDS[img.match] || img.match}</div></div></div></section>
    <section class="card">${tabBar(id, [["this", "This image"], ["catch", "What pods do catch"]])}${tab === "catch" ? PODS_CATCH : imageTable(img)}</section>${fix}`};
}

/* ---- The comparison of a pack ---- */
const settingsTable = rows => h`<div class="tw"><table><thead><tr><th>Setting</th><th>Before</th><th>After</th></tr></thead><tbody>${rows.map(r => h`<tr><td class="m">${r.path}</td><td class="was val">${leafText(r.old)}</td><td class="now val">${leafText(r.new)}</td></tr>`)}</tbody></table></div>`;

// leafText shows a changed value. A string has quotes, and a value that is absent shows a dash.
function leafText(v) {
  if (v === undefined || v === null) return DASH;
  return typeof v === "string" ? `"${v}"` : JSON.stringify(v);
}

function tabBar(id, items) {
  const cur = S.tabs[id] || items[0][0];
  return h`<div class="tabs" role="tablist">${items.map(([k, l, n]) => h`<button class="tab ${k === cur ? "on" : ""}" role="tab" aria-selected="${k === cur}" data-tab="${id}" data-k="${k}">${l}${n === undefined ? "" : h`<em>${n}</em>`}</button>`)}</div>`;
}

function changesPanel(list, noun, head) {
  if (!list.length) return h`<div class="body small mut">No ${noun} changed.</div>`;
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

// diffBlock draws the head and the lines of one diff. The control of the layout sits in the head, and it sets the one layout of the page.
function diffBlock(title, op, rows) {
  const seg = h`<span class="seg" role="group" aria-label="Diff layout">${VIEWS.map(([k, l]) => h`<button data-view="${k}" aria-pressed="${S.view === k ? "true" : "false"}" class="${S.view === k ? "on" : ""}">${l}</button>`)}</span>`;
  const lines = S.view === "split" ? splitRows(rows) : unifiedRows(rows);
  return h`<div class="dblock"><div class="diffhead"><span>${title}${op ? h` <span class="mut">${op}</span>` : ""} <span class="now">+${countOf(rows, "+")}</span> <span class="was">-${countOf(rows, "-")}</span></span>${seg}</div><div class="diff"><table><tbody>${lines}</tbody></table></div></div>`;
}

// tooLargeNote tells that the observer kept no text, and it names the command that prints the text.
const tooLargeNote = command => h`<div class="note small nt">The text of this pack is above 256 KiB, so the observer keeps none.</div><div class="body small mut pb8">To read it, run this at a terminal:</div><div class="pre mt0">${command}</div>`;

// manifestsPanel shows the text of each manifest that has text. A manifest with no text, and each parameter, show in a small table.
function manifestsPanel(d, command) {
  if (!d.manifests.length && !d.paramChanges.length) return h`<div class="body small mut">No manifest changed.</div>`;
  const texts = d.manifests.map(m => [m, parseUnified(m.unified || "")]);
  const plain = [...texts.filter(([, rows]) => !rows.length).map(([m]) => ["Manifest", m]), ...d.paramChanges.map(m => ["Parameter", m])];
  const table = plain.length ? h`<div class="tw"><table><thead><tr><th>Change</th><th>Kind</th><th>Name</th></tr></thead><tbody>${plain.map(([kind, m]) => h`<tr><td class="m">${m.op}</td><td>${kind}</td><td class="m">${m.name}</td></tr>`)}</tbody></table></div>` : "";
  const blocks = texts.filter(([, rows]) => rows.length).map(([m, rows]) => diffBlock(`manifest ${m.name}`, m.op, rows));
  return h`${table}${blocks}${d.textTooLarge && d.manifests.length ? tooLargeNote(command) : ""}`;
}

function valuesPanel(d, id, command) {
  const v = d.values, s = v.settings, f = v.followsTheVersion, r = v.repeatsTheLists, total = s.length + f.length + r.length;
  const rows = parseUnified(v.unified || "");
  const text = rows.length ? diffBlock(`${d.key} values`, "", rows) : d.textTooLarge && (total > 0 || !v.parseable) ? tooLargeNote(command) : "";
  if (!v.parseable && total === 0) return h`<div class="body small mut">Strata cannot read the values of this pack as settings.</div>${text}`;
  const lead = total ? h`<div class="body small mut pb8">${total} changed ${plural(total, "value", "values")}: ${s.length} ${plural(s.length, "setting", "settings")}, ${f.length} follow the version, ${r.length} repeat the lists.</div>` : h`<div class="body small mut">No value changed.</div>`;
  const masked = d.masked.length ? h`<div class="note small nt"><b>Masked:</b> ${d.masked.map(m => h`<span class="m">${m}</span>`)}. An equal root does not show an equal secret.</div>` : "";
  const follows = f.length ? h`<details data-open="${id}-f" ${openAttr(id + "-f")}><summary>Follows the version: ${f.length} ${plural(f.length, "value", "values")}</summary>${settingsTable(f)}</details>` : "";
  const repeats = r.length ? h`<details data-open="${id}-r" ${openAttr(id + "-r")}><summary>Repeats the lists: ${r.length} ${plural(r.length, "value", "values")}</summary><div class="body small mut">The Images and Charts tabs show these.</div></details>` : "";
  return h`${lead}${s.length ? settingsTable(s) : ""}${masked}${text}${follows}${repeats}`;
}

/* compareCard draws one comparison: the head, the flags, the tabs, and the panel of the tab. The command is the one that prints the text when the observer kept none. */
function compareCard(id, headLeft, d, whenNone, command) {
  let inner;
  let flags = "";
  switch (d.state) {
    case "loading": inner = h`<div class="body mut">Loading the changes.</div>`; break;
    case "none": inner = h`<div class="body mut">${whenNone}</div>`; break;
    case "ok": {
      const data = d.data, tab = S.tabs[id] || "values";
      flags = flagRow(data.flags);
      const nv = data.values.settings.length + data.values.followsTheVersion.length + data.values.repeatsTheLists.length;
      const panel = tab === "manifests" ? manifestsPanel(data, command) : tab === "images" ? changesPanel(data.imageChanges, "image", "Repository") : tab === "charts" ? changesPanel(data.chartChanges, "chart", "Chart") : valuesPanel(data, id, command);
      inner = h`${tabBar(id, [["values", "Values", nv], ["manifests", "Manifests", data.manifests.length + data.paramChanges.length], ["images", "Images", data.imageChanges.length], ["charts", "Charts", data.chartChanges.length]])}${panel}`;
      break;
    }
    default: inner = h`<div class="body mut">The observer could not read these changes now. Try again later.</div>`;
  }
  return h`<section class="card"><div class="chead"><div class="tb">${headLeft}</div>${flags}</div>${inner}</section>`;
}

// sidesOf lists the comparisons of the record that a pack can show.
function sidesOf(x) {
  const out = [[SIDE.PUBLISHED, "Record → Published now"]];
  const cp = x.clusterCopy && x.clusterCopy.status;
  if (cp === COPY.SAME || cp === COPY.SAME_CONTENT || cp === COPY.DIFFERS) out.push([SIDE.CLUSTER, "Record → This cluster's copy"]);
  return out;
}

function driftCard(subject, x, p) {
  const sides = sidesOf(x), id = "side-" + subject + "-" + p.key;
  // A finding of the pods says nothing about the sides of a record.
  const fs = D.findings.filter(f => f.profile === subject && f.packs.includes(p.key) && !IMAGE_KINDS.includes(f.kind));
  const clusterOnly = fs.length > 0 && fs.every(f => f.kind === KIND.CLUSTER || f.kind === KIND.IMAGE_ID);
  const side = S.side[id] || (clusterOnly && sides.length > 1 ? SIDE.CLUSTER : SIDE.PUBLISHED);
  const pub = x.published, cp = x.clusterCopy;
  const to = side === SIDE.CLUSTER ? cp && cp.rootB : pub && pub.rootLive;
  const select = sides.length > 1 ? h`<select id="side" class="search auto" data-id="${id}" aria-label="Compare">${sides.map(([k, l]) => h`<option value="${k}" ${k === side ? raw("selected") : ""}>${l}</option>`)}</select>` : h`<span>${sides[0][1]}</span>`;
  const d = diffOf(subject, p, side);
  // A side that strata could not read has no answer, and no answer is not an identical copy.
  const unread = side === SIDE.PUBLISHED && !(pub && pub.status !== VERIFY.ERROR);
  const identical = unread ? badge("y", "Not read") : badge("g", "Identical");
  const state = d.state === "ok" ? (d.data.status === "unchanged" ? badge("g", "Identical") : badge("r", "Changed")) : d.state === "none" ? identical : "";
  const head = h`<label for="side" class="small mut">Compare</label>${select}<span class="m mut">root ${root(pub && pub.rootRecorded)}${ARROW}${root(to)}</span>${state}`;
  const tabId = id + "-" + side;
  let none = "Identical. The published version still holds what the record holds.";
  if (side === SIDE.CLUSTER) none = "Identical. This cluster's copy still holds what the record holds.";
  else if (unread) none = "Not read. Strata could not read the published version.";
  const command = side === SIDE.CLUSTER ? `strata compare RECORD cluster:${D.cluster.cluster.name || D.cluster.cluster.uid}/${x.name} --full` : `strata compare RECORD ${subject} --full`;
  return compareCard(tabId, head, d, none, command);
}

function moveCards(subject, p) {
  const row = newerRowOf(subject, p);
  if (!row) return "";
  const f = row.fold, total = f.settings + f.followsTheVersion + f.repeatsTheLists;
  const unread = moveUnread(subject, p), count = v => unread ? DASH : v;
  const pinned = PINNED[row.pinnedState];
  const d = diffOf(subject, p, SIDE.NEWER);
  const head = h`<span class="small mut">Compare</span><span>Pack ${row.version}${ARROW}pack ${row.newest}</span>`;
  const summary = h`<section class="card"><div class="chead"><h2>Move to ${row.newest} <span class="sub">Listed ${day(row.listed)}.</span></h2>${impactCell(subject, p, row)}</div><div class="body">
    <div class="meta mt0"><div><span>Settings</span>${unread ? DASH : h`<b>${f.settings}</b> of ${total} ${plural(total, "value", "values")}`}</div><div><span>Images</span>${count(row.images)}</div><div><span>Charts</span>${count(row.charts)}</div>${row.drops.length ? h`<div><span>Drops</span>${row.drops.join(", ")}</div>` : ""}${row.floating ? h`<div><span>Tag</span>Floating</div>` : ""}</div>
    <div class="small mut mt9">${unread ? "Strata could not compare these versions now, so the counts are not known." : `${impactOf(row.impact)[1]} is a hint from strata's table.`}${pinned ? ` The pinned version is ${pinned} in the registry.` : ""}</div>${askBlock(subject, p, row)}</div></section>`;
  const pack = version => `pack:${row.registryUid}/${row.name}@${version}`;
  const command = `strata compare ${pack(row.version)} ${pack(row.newest)} --full`;
  return h`${summary}${compareCard("move-" + subject + "-" + p.key, head, d, "The observer holds no changes for this move.", command)}`;
}

function viewPack(subject, layer, name) {
  const x = profileOf(subject), p = x ? x.packs.find(q => q.layer === layer && q.name === name) : null;
  const cr = crumbs(h`<a href="#profiles">Profiles</a>`, h`<a href="${profileHash(subject)}">${subject}</a>`, name);
  if (!p) return viewMissing(cr, "profiles");
  const row = newerRowOf(subject, p);
  let main = "";
  if (p.status === STATUS.DRIFT) main = driftCard(subject, x, p);
  if (row) main = h`${main}${moveCards(subject, p)}`;
  if (!main) {
    const why = x.status === STATUS.NO_RECORD ? "No record." : p.status === STATUS.NOT_CHECKED ? h`<b>Not fully checked.</b> ${notCheckedLine(x)}` : "Unchanged and current.";
    main = h`<section class="card"><div class="body">${why}</div></section>`;
  }
  const extra = p.status === STATUS.DRIFT && p.newer ? statusBadge(STATUS.NEWER) : "";
  return {nav: "profiles", crumbs: cr, body: h`${alertBars()}<section class="card"><div class="body"><div class="title"><h1 class="m h1m">${p.layer}/${p.name} ${p.version}</h1>${statusBadge(p.status)}${extra}</div>
    <div class="meta"><div><span>Registry</span>${p.registry}</div><div><span>Kind</span>${p.kind}</div><div><span>Upstream</span><span class="m">${p.upstream ? digest(p.upstream) : "none"}</span></div><div><span>Profile</span><a class="m" href="${profileHash(subject)}">${subject}</a></div></div></div></section>${main}`};
}

/* ---- The alerts ----
   The page only reads the rules. A file in Git holds them, and no route of the observer writes. */
const ALERTS_OFF_WORDS = h`<div>The observer sends no alert. The send is opt-in.</div>
  <div class="mt6">A message goes to a webhook that your team names. It holds no value of a record or of the cluster.</div>
  <div class="mt6">The page "The observer" in the book says how to turn it on.</div>`;

// digestWords says when a digest goes. The API gives the period in hours.
const digestWords = every => every === WEEK ? "Weekly digest" : `Digest every ${every}`;

// ruleSend says when a rule sends. A mute says until when it holds, or that it ended.
function ruleSend(r) {
  if (r.muteUntil) return r.state === ALERT_STATE.MUTED ? `Muted until ${r.muteUntil}` : `Mute ended ${r.muteUntil}`;
  return r.send === ALERT_SEND.DIGEST ? digestWords(D.alerts.digestEvery) : "At once";
}

// ruleTarget names what a rule applies to: the cluster, a profile, or a pack.
const ruleTarget = r => r.level === "cluster" ? D.cluster.cluster.name || D.cluster.cluster.uid : r.match;

const ruleRow = r => h`<tr><td>${badgeOf(ALERT_BADGE, r.state)}</td><td>${ALERT_LEVEL[r.level] || r.level}</td><td class="m">${ruleTarget(r)}</td><td>${ALERT_NEWS[r.news] || r.news}</td><td>${ruleSend(r)}</td><td class="m">${r.to || DASH}</td></tr>`;

function alertsBody() {
  const a = D.alerts;
  if (!a.set) return h`<section class="card"><div class="chead"><h2>Alert rules <span class="sub">Not set</span></h2></div><div class="body">${ALERTS_OFF_WORDS}</div></section>`;
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
    put(main, S.offline ? h`<div class="alertbar a">The page cannot reach the observer. <span>It tries again each 30 seconds.</span></div>` : h`<div class="body mut">Loading.</div>`);
    return;
  }
  const active = document.activeElement;
  const typing = active && active.id === "q" ? {start: active.selectionStart, end: active.selectionEnd} : null;
  const v = route();
  put(document.getElementById("navl"), navLinks(v.nav));
  const c = D.cluster.cluster;
  put(document.getElementById("ctx"), h`Cluster<b>${c.name || c.uid || "-"}</b>`);
  put(document.getElementById("top"), topBar(v.crumbs));
  put(main, v.body);
  if (typing) {
    const q = document.getElementById("q");
    if (q) { q.focus(); q.setSelectionRange(typing.start, typing.end); }
  }
}

function onClick(ev) {
  const t = ev.target.closest("[data-tab]");
  if (t) { S.tabs[t.dataset.tab] = t.dataset.k; render(); return; }
  const f = ev.target.closest("[data-filter]");
  if (f) { S.filter = f.dataset.filter; render(); return; }
  const v = ev.target.closest("[data-view]");
  if (v) { S.view = v.dataset.view === "split" ? "split" : "unified"; render(); return; }
  const q = ev.target.closest("[data-ask]");
  if (q) { askPalette(q.dataset.ask, q.dataset.pack); return; }
  const g = ev.target.closest("[data-go]");
  if (g && !ev.target.closest("a,button,select,input")) location.hash = g.dataset.go;
}

function start() {
  document.addEventListener("click", onClick);
  document.addEventListener("input", ev => {
    if (ev.target.id === "q") { S.q = ev.target.value.trim(); render(); }
  });
  document.addEventListener("change", ev => {
    if (ev.target.id === "side") { S.side[ev.target.dataset.id] = ev.target.value; render(); }
  });
  document.addEventListener("toggle", ev => {
    const k = ev.target.dataset && ev.target.dataset.open;
    if (k) S.open[k] = ev.target.open;
  }, true);
  addEventListener("hashchange", () => { render(); scrollTo(0, 0); });
  render();
  refresh();
  setInterval(refresh, REFRESH_MS);
}

start();
