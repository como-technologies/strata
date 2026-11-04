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
const COPY = {SAME: "same", SAME_CONTENT: "same-content", DIFFERS: "differs", NOT_ON_CLUSTER: "not-on-this-cluster", NOT_READ: "not-read"};
const KIND = {PUBLISHED: "published-changed", OVERWRITTEN: "overwritten", RENAMED: "renamed", GONE: "gone", CLUSTER: "cluster-differs", IMAGE_ID: "image-id-changed"};
const SIDE = {PUBLISHED: "published", CLUSTER: "cluster", NEWER: "newer"};
const CAUSE = {NO_DIGEST: "no-upstream-digest", OLD_COPY: "older-registry-copy", NO_RECORD: "no-record", READ_ERROR: "read-error", PIN_ERROR: "pin-error"};
const EVENT = {DRIFT: "drift", NEWER: "newer", NOT_CHECKED: "not-checked", INFO: "info"};

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

// What a finding says, by kind. Title is the heading, and line is the sentence for a profile.
const FINDING = {
  [KIND.PUBLISHED]: {title: "Published version changed after its record", line: "changed in the published version."},
  [KIND.OVERWRITTEN]: {title: "Another version replaced the recorded version", line: "Another version replaced the recorded version in place."},
  [KIND.RENAMED]: {title: "The recorded version has another name", line: "The recorded version has another name now."},
  [KIND.GONE]: {title: "Palette no longer holds the recorded version", line: "Palette no longer holds the recorded version."},
  [KIND.CLUSTER]: {title: "This cluster runs other content than its record", line: "differ on this cluster."},
  [KIND.IMAGE_ID]: {title: "A pack names another machine image", line: "name another machine image."},
};
const CAUSE_TEXT = {
  [CAUSE.NO_DIGEST]: "Palette gives no upstream digest for this registry kind, so a change of the archive does not show.",
  [CAUSE.OLD_COPY]: "Current against an older copy of the registry. A newer version can be missing here.",
  [CAUSE.NO_RECORD]: "No record holds this profile, so strata cannot say that anything changed.",
  [CAUSE.READ_ERROR]: "Strata could not read the record or its answer.",
  [CAUSE.PIN_ERROR]: "Palette gave an error for this pack, so strata cannot look for a newer version.",
};
// What a profile says in the list for a cause, with the names of its packs.
const CAUSE_LINE = {
  [CAUSE.NO_DIGEST]: (names) => `${names} has no upstream digest.`,
  [CAUSE.OLD_COPY]: (names, registry) => `${names} is current against an older copy of ${registry}.`,
  [CAUSE.NO_RECORD]: () => "No record.",
  [CAUSE.READ_ERROR]: () => "Strata could not read the record.",
  [CAUSE.PIN_ERROR]: (names) => `Palette gave an error for ${names}.`,
};
const ERROR_TEXT = {
  api: status => status === 401 || status === 403 ? `HTTP ${status}, the API key was refused` : `HTTP ${status}, Palette gave an error`,
  transport: () => "the link to Palette failed",
  "not-json": () => "Palette sent an answer that is not JSON",
  timeout: () => "Palette did not answer in time",
};

/* ---- State ---- */
// D holds the last answers of the routes. S holds what the person did on the page.
let D = null;
const S = {filter: "all", q: "", tabs: {}, side: {}, open: {}, diffs: {}, offline: false, loadedAt: null};

async function getJSON(url) {
  const r = await fetch(url, {cache: "no-store"});
  if (!r.ok) throw new Error(url + " " + r.status);
  return r.json();
}

// load reads the six summary routes. The detail route is read when a screen needs it.
async function load() {
  const [cluster, findings, profiles, newer, notChecked, events] = await Promise.all(
    ["/v1/cluster", "/v1/findings", "/v1/profiles", "/v1/newer", "/v1/notchecked", "/v1/events"].map(getJSON));
  if (D && (D.cluster.content.checkedAt !== cluster.content.checkedAt || D.cluster.versions.readAt !== cluster.versions.readAt)) {
    S.diffs = {};
  }
  D = {cluster, findings: findings.findings, profiles: profiles.profiles, newer: newer.newer, notChecked: notChecked.notChecked, limits: notChecked.limits, events: events.events};
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

// diffOf gives the state of one detail request, and starts the request the first time. The key holds
// the clock of the cycle that the side comes from, so a new cycle asks again.
function diffOf(subject, p, side) {
  const stamp = side === SIDE.NEWER ? D.cluster.versions.readAt : D.cluster.content.checkedAt;
  const key = [subject, p.key, side, stamp].join("|");
  if (!S.diffs[key]) {
    S.diffs[key] = {state: "loading"};
    const diffs = S.diffs;
    fetchDiff(subject, p, side).then(r => { diffs[key] = r; render(); });
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
const hasChecks = () => D.cluster.page !== PAGE.FIRST_LOAD && D.cluster.page !== PAGE.NO_RECORDS;
const num = n => hasChecks() ? n : "-";
const asCount = st => st === STATUS.NO_RECORD ? STATUS.NOT_CHECKED : st;
const packName = key => key.slice(key.indexOf("/") + 1);
const profileOf = subject => D.profiles.find(p => subjectOf(p) === subject);
const impactOf = i => IMPACT[i] || ["y", i];

const badge = (cls, text) => h`<span class="badge ${cls}">${text}</span>`;
const statusBadge = st => { const [c, t] = BADGE[st] || ["y", st]; return badge(c, t); };
const impactBadge = i => { const [c, t] = impactOf(i); return badge(c, t); };
const lab = (k, v) => h`<span class="lab"><i>${k}=</i>"${v}"</span>`;
const hbar = x => h`<span class="hbar" role="img" aria-label="Status of each pack">${x.packs.map(p => h`<i class="${(BADGE[p.status] || ["y"])[0]}" title="${p.name}: ${(BADGE[p.status] || [0, p.status])[1]}"></i>`)}</span>`;
const mut = text => h`<span class="mut">${text}</span>`;
const openAttr = k => S.open[k] ? raw(" open") : "";
const flagRow = on => h`<span class="tb">${FLAGS.map(f => h`<span class="badge ${on.includes(f) ? "b" : "y off"}">${f}</span>`)}</span>`;
const root = r => r ? h`<span title="${r}">${hex12(r)}</span>` : "";

// newerRowOf finds the row of /v1/newer that a pack of a profile belongs to.
const newerRowOf = (subject, p) => D.newer.find(r => r.profiles.includes(subject) && r.name === p.name && r.version === p.version && r.registry === p.registry);
// worstOf gives the status that wins in a list of statuses.
const worstOf = list => SEVERITY.find(s => list.includes(s)) || STATUS.OK;

function errorWords(err) {
  if (!err) return "";
  const f = ERROR_TEXT[err.kind];
  return f ? f(err.status) : "strata gave an error";
}

/* ---- The frame: nav, top bar, alert bars ---- */
function navLinks(cur) {
  const c = D.cluster, n = c.counts;
  const items = [["overview", "Overview", ""], ["findings", "Findings", h`<span class="cnt ${n.findings ? "r" : ""}">${num(n.findings)}</span>`],
    ["profiles", "Profiles", h`<span class="cnt">${n.profiles}</span>`], ["newer", "Newer versions", h`<span class="cnt">${num(n.newer)}</span>`],
    ["notchecked", "Not checked", h`<span class="cnt">${num(n.notChecked)}</span>`]];
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
  return bars;
}

/* ---- The cards ---- */
function tiles() {
  const c = D.cluster, n = c.counts, p = c.page, checked = hasChecks();
  const T = (cls, label, value, sub, href) => h`<a class="tile ${cls}" href="#${href}"><div class="l">${label}</div><div class="v">${value}</div><div class="s">${sub}</div></a>`;
  const none = n.notChecked === 0;
  let driftSub, newerSub, notCheckedSub;
  if (!checked) {
    const word = p === PAGE.FIRST_LOAD ? "Not read yet" : "No records";
    driftSub = word; newerSub = p === PAGE.FIRST_LOAD ? "Not read yet" : "Needs a record"; notCheckedSub = word;
  } else {
    driftSub = n.drift === 0 ? (none ? "Nothing changed" : "None in what was checked") : n.drift === 1 ? driftSubject() : `${n.drift} profiles`;
    const fresh = D.newer.filter(r => r.new).length;
    newerSub = n.newer === 0 ? (none ? `All ${n.packs} packs current` : "None listed") : `${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")} · ${fresh} this week`;
    notCheckedSub = n.notChecked === 0 ? `${n.packsInFull} of ${n.packs} packs in full` : `${n.notChecked} ${plural(n.notChecked, "cause", "causes")} · ${n.packsInFull} of ${n.packs} packs in full`;
  }
  return h`<section class="tiles" aria-label="Summary">
    ${T(!checked ? "y" : n.drift ? "r" : "g", "Drift", num(n.drift), driftSub, "findings")}
    ${T("b", "Newer versions", num(n.newer), newerSub, "newer")}
    ${T("y", "Not checked", num(n.notChecked), notCheckedSub, "notchecked")}
    ${T("", "Profiles", p === PAGE.FIRST_LOAD ? "-" : n.profiles, `${n.packs} packs on this cluster`, "profiles")}
  </section>`;
}

// driftSubject names the profile that drifts, for the tile.
function driftSubject() {
  const x = D.profiles.find(p => p.status === STATUS.DRIFT);
  return x ? subjectOf(x) : "1 profile";
}

function findingTitle(f) { return (FINDING[f.kind] || {title: f.kind}).title; }

function findingsCard() {
  const rows = D.findings;
  const body = rows.length ? h`<div class="tw"><table><tbody>${rows.map(f => {
    const p = profileOf(f.profile);
    const key = f.packs[0] && p ? p.packs.find(k => k.key === f.packs[0]) : null;
    const href = key ? packHash(f.profile, key) : profileHash(f.profile);
    return h`<tr class="link" data-go="${href}"><td class="w90">${statusBadge(STATUS.DRIFT)}${f.reBaselineNeeded ? h` <span class="badge y">re-baseline needed</span>` : ""}</td>
      <td><div class="fw"><a href="${href}">${findingTitle(f)}</a></div><div>${lab("profile", f.profile)}${f.packs.map(k => lab("pack", packName(k)))}</div><div class="mut small narrow-show">${ago(f.firstSeen)}</div></td>
      <td class="n mut small w60 nw narrow-hide">${ago(f.firstSeen)}</td></tr>`;
  })}</tbody></table></div>` : h`<div class="body mut">Nothing needs attention.</div>`;
  return h`<section class="card"><div class="chead"><h2>Needs attention <span class="sub">${rows.length}</span></h2><a class="small" href="#findings">All findings</a></div>${body}</section>`;
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
  return h`<section class="card"><div class="chead"><h2>What a check does not cover</h2></div><div class="tw"><table><tbody>
    ${row("Image behind a tag", "Strata compares the name and the tag.", num(l.imagesNamedByATag))}
    ${row("Bytes of a chart", "The record holds the digest Palette lists", num(l.helmPacks))}
    ${row("Masked values", "An equal root does not show an equal secret", num(l.maskedPaths))}
    ${row("Edits with kubectl", "Strata compares what Palette holds", h`<span class="mut">n/a</span>`)}
  </tbody></table></div></section>`;
}

/* What a profile says in the list: the news that matters most first. */
function findingLine(f, x) {
  const w = FINDING[f.kind] || {line: f.kind};
  const names = f.packs.map(packName).join(", ");
  let text = f.kind === KIND.PUBLISHED || f.kind === KIND.CLUSTER || f.kind === KIND.IMAGE_ID ? (names ? `${names} ${w.line}` : w.title + ".") : w.line;
  if (f.kind === KIND.PUBLISHED) {
    const copy = x.clusterCopy && x.clusterCopy.status;
    if (f.alsoOnCluster) text += " This cluster has the edit too.";
    else if (copy === COPY.SAME || copy === COPY.SAME_CONTENT) text += " This cluster keeps its copy.";
  }
  return text;
}

function newsOf(x) {
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
        return h`${ms[0].name} ${ms[0].version}${ARROW}${ms[0].newer}${row && row.new ? h` <span class="badge y">new</span>` : ""}`;
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
  return h`<section class="card"><div class="chead"><h2>Profiles <span class="sub">${D.cluster.counts.profiles} profiles, ${D.cluster.counts.packs} packs</span></h2>
    <div class="tb"><div class="seg" role="group" aria-label="Filter by status">${FILTERS.map(([k, l]) => h`<button data-filter="${k}" class="${S.filter === k ? "on" : ""}">${l} ${cnt[k]}</button>`)}</div>
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
    if (!checked) return h`<div class="body mut">Strata needs a record to look for newer versions. A record is the copy your team approved. Your pipeline writes records from Git.</div>`;
    if (!rows.length) return h`<div class="body mut">${D.cluster.counts.notChecked ? "No newer version is listed." : `All ${D.cluster.counts.packs} packs are current.`}</div>`;
    return h`<div class="tw"><table><tbody>${rows.map(r => {
      const first = r.profiles[0], p = first ? profileOf(first) : null, k = p ? p.packs.find(q => q.name === r.name && q.version === r.version) : null;
      const href = k ? packHash(first, k) : first ? profileHash(first) : "#newer";
      return h`<tr class="link" data-go="${href}"><td class="w90">${statusBadge(STATUS.NEWER)}</td><td class="m"><a href="${href}">${r.name} ${r.version}${ARROW}${r.newest}</a></td><td>${r.profiles.map(s => lab("profile", s))}</td><td>${impactBadge(r.impact)}</td></tr>`;
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
    body = h`<div class="body mut">Strata needs a record to look for newer versions. A record is the copy your team approved. Your pipeline writes records from Git.</div>`;
  } else if (!rows.length) {
    body = h`<div class="body mut">${n.notChecked ? "No newer version is listed." : `All ${n.packs} packs are current.`}</div>`;
  } else {
    const groups = [];
    rows.forEach(r => r.profiles.forEach(s => {
      let g = groups.find(q => q.subject === s);
      if (!g) { g = {subject: s, rows: []}; groups.push(g); }
      g.rows.push(r);
    }));
    groups.sort((a, b) => a.subject.localeCompare(b.subject));
    body = h`<div class="body small mut pb9"><b>Settings</b> counts what a move really changes. Impact is a hint from strata's table, so read the two together.</div>
      <div class="tw"><table><thead><tr><th>Pack</th><th>Move</th><th>Impact</th><th class="n">Settings</th><th class="n narrow-hide">Images</th><th class="n narrow-hide">Charts</th><th class="narrow-hide">Listed</th></tr></thead><tbody>${groups.map(g => {
        const p = profileOf(g.subject);
        const total = p ? p.packs.length : "";
        return [h`<tr class="g"><td colspan="7"><a class="m" href="${profileHash(g.subject)}">${g.subject}</a> <span class="mut">· ${g.rows.length}${total !== "" ? h` of ${total}` : ""} ${plural(total === "" ? g.rows.length : total, "pack", "packs")}</span></td></tr>`,
          g.rows.map(r => {
            const k = p ? p.packs.find(q => q.name === r.name && q.version === r.version) : null;
            const href = k ? packHash(g.subject, k) : profileHash(g.subject);
            return h`<tr class="link" data-go="${href}"><td class="m"><a href="${href}">${r.name}</a></td><td class="m nw">${r.version}${ARROW}${r.newest}</td><td>${impactBadge(r.impact)}</td><td class="n"><b>${r.fold.settings}</b></td><td class="n narrow-hide">${r.images}</td><td class="n narrow-hide">${r.charts}</td><td class="m mut narrow-hide nw">${day(r.listed)}${r.new ? h` <span class="badge b">new</span>` : ""}</td></tr>`;
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
      return h`<tr class="link" data-go="${p ? profileHash(first) : "#profiles"}"><td class="m">${what || mut("The profile")}</td><td class="m">${r.profiles.join(", ")}</td><td>${why}</td></tr>`;
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
  } else if (pub.status === "drifted") {
    pubCard = h`<div class="vcard"><div class="h">Published version ${statusBadge(STATUS.DRIFT)}</div><div class="m mt6">root <span class="was">${root(pub.rootRecorded)}</span>${ARROW}<span class="now">${root(pub.rootLive)}</span></div></div>`;
  } else if (pub.status === "unchanged") {
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

/* ---- The comparison of a pack ---- */
const settingsTable = rows => h`<div class="tw"><table><thead><tr><th>Setting</th><th>Before</th><th>After</th></tr></thead><tbody>${rows.map(r => h`<tr><td class="m">${r.path}</td><td class="was val">${leafText(r.old)}</td><td class="now val">${leafText(r.new)}</td></tr>`)}</tbody></table></div>`;

// leafText shows a changed value. A string has quotes, and a value that is absent shows a dash.
function leafText(v) {
  if (v === undefined || v === null) return DASH;
  return typeof v === "string" ? `"${v}"` : JSON.stringify(v);
}

function tabBar(id, items) {
  const cur = S.tabs[id] || items[0][0];
  return h`<div class="tabs" role="tablist">${items.map(([k, l, n]) => h`<button class="tab ${k === cur ? "on" : ""}" role="tab" aria-selected="${k === cur}" data-tab="${id}" data-k="${k}">${l}<em>${n}</em></button>`)}</div>`;
}

function changesPanel(list, noun, head) {
  if (!list.length) return h`<div class="body small mut">No ${noun} changed.</div>`;
  return h`<div class="tw"><table><thead><tr><th></th><th>${head}</th><th>Before</th><th>After</th></tr></thead><tbody>${list.map(c => h`<tr><td class="m">${OPS[c.op] || c.op}</td><td class="m">${c.name}</td><td class="was">${c.old || DASH}</td><td class="now">${c.new || DASH}</td></tr>`)}</tbody></table></div>`;
}

function manifestsPanel(d) {
  const list = [...d.manifests.map(m => ["Manifest", m]), ...d.paramChanges.map(m => ["Parameter", m])];
  if (!list.length) return h`<div class="body small mut">No manifest changed.</div>`;
  return h`<div class="tw"><table><thead><tr><th>Change</th><th>Kind</th><th>Name</th></tr></thead><tbody>${list.map(([kind, m]) => h`<tr><td class="m">${m.op}</td><td>${kind}</td><td class="m">${m.name}</td></tr>`)}</tbody></table></div>`;
}

function valuesPanel(d, id) {
  const v = d.values, s = v.settings, f = v.followsTheVersion, r = v.repeatsTheLists, total = s.length + f.length + r.length;
  if (!v.parseable && total === 0) return h`<div class="body small mut">Strata cannot read the values of this pack as settings.</div>`;
  const lead = total ? h`<div class="body small mut pb8">${total} changed ${plural(total, "value", "values")}: ${s.length} ${plural(s.length, "setting", "settings")}, ${f.length} follow the version, ${r.length} repeat the lists.</div>` : h`<div class="body small mut">No value changed.</div>`;
  const masked = d.masked.length ? h`<div class="note small nt"><b>Masked:</b> ${d.masked.map(m => h`<span class="m">${m}</span>`)}. An equal root does not show an equal secret.</div>` : "";
  const follows = f.length ? h`<details data-open="${id}-f" ${openAttr(id + "-f")}><summary>Follows the version: ${f.length} ${plural(f.length, "value", "values")}</summary>${settingsTable(f)}</details>` : "";
  const repeats = r.length ? h`<details data-open="${id}-r" ${openAttr(id + "-r")}><summary>Repeats the lists: ${r.length} ${plural(r.length, "value", "values")}</summary><div class="body small mut">The Images and Charts tabs show these.</div></details>` : "";
  return h`${lead}${s.length ? settingsTable(s) : ""}${masked}${follows}${repeats}`;
}

/* compareCard draws one comparison: the head, the flags, the tabs, and the panel of the tab. */
function compareCard(id, headLeft, d, whenNone) {
  let inner;
  let flags = "";
  switch (d.state) {
    case "loading": inner = h`<div class="body mut">Loading the changes.</div>`; break;
    case "none": inner = h`<div class="body mut">${whenNone}</div>`; break;
    case "ok": {
      const data = d.data, tab = S.tabs[id] || "values";
      flags = flagRow(data.flags);
      const nv = data.values.settings.length + data.values.followsTheVersion.length + data.values.repeatsTheLists.length;
      const panel = tab === "manifests" ? manifestsPanel(data) : tab === "images" ? changesPanel(data.imageChanges, "image", "Repository") : tab === "charts" ? changesPanel(data.chartChanges, "chart", "Chart") : valuesPanel(data, id);
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
  const fs = D.findings.filter(f => f.profile === subject && f.packs.includes(p.key));
  const clusterOnly = fs.length > 0 && fs.every(f => f.kind === KIND.CLUSTER || f.kind === KIND.IMAGE_ID);
  const side = S.side[id] || (clusterOnly && sides.length > 1 ? SIDE.CLUSTER : SIDE.PUBLISHED);
  const pub = x.published, cp = x.clusterCopy;
  const to = side === SIDE.CLUSTER ? cp && cp.rootB : pub && pub.rootLive;
  const select = sides.length > 1 ? h`<select id="side" class="search auto" data-id="${id}" aria-label="Compare">${sides.map(([k, l]) => h`<option value="${k}" ${k === side ? raw("selected") : ""}>${l}</option>`)}</select>` : h`<span>${sides[0][1]}</span>`;
  const d = diffOf(subject, p, side);
  const state = d.state === "ok" ? (d.data.status === "unchanged" ? badge("g", "Identical") : badge("r", "Changed")) : d.state === "none" ? badge("g", "Identical") : "";
  const head = h`<label for="side" class="small mut">Compare</label>${select}<span class="m mut">root ${root(pub && pub.rootRecorded)}${ARROW}${root(to)}</span>${state}`;
  const tabId = id + "-" + side;
  const none = side === SIDE.CLUSTER ? "Identical. This cluster's copy still holds what the record holds." : "Identical. The published version still holds what the record holds.";
  return compareCard(tabId, head, d, none);
}

function moveCards(subject, p) {
  const row = newerRowOf(subject, p);
  if (!row) return "";
  const f = row.fold, total = f.settings + f.followsTheVersion + f.repeatsTheLists;
  const pinned = PINNED[row.pinnedState];
  const d = diffOf(subject, p, SIDE.NEWER);
  const head = h`<span class="small mut">Compare</span><span>Pack ${row.version}${ARROW}pack ${row.newest}</span>`;
  const summary = h`<section class="card"><div class="chead"><h2>Move to ${row.newest} <span class="sub">Listed ${day(row.listed)}.</span></h2>${impactBadge(row.impact)}</div><div class="body">
    <div class="meta mt0"><div><span>Settings</span><b>${f.settings}</b> of ${total} ${plural(total, "value", "values")}</div><div><span>Images</span>${row.images}</div><div><span>Charts</span>${row.charts}</div>${row.drops.length ? h`<div><span>Drops</span>${row.drops.join(", ")}</div>` : ""}${row.floating ? h`<div><span>Tag</span>Floating</div>` : ""}</div>
    <div class="small mut mt9">${impactOf(row.impact)[1]} is a hint from strata's table.${pinned ? ` The pinned version is ${pinned} in the registry.` : ""}</div></div></section>`;
  return h`${summary}${compareCard("move-" + subject + "-" + p.key, head, d, "The observer holds no changes for this move.")}`;
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

/* ---- Routes, rendering, events ---- */
function route() {
  const hash = (location.hash || "#overview").slice(1);
  try {
    if (hash === "findings") return viewFindings();
    if (hash === "profiles") return viewProfiles();
    if (hash === "newer") return viewNewer();
    if (hash === "notchecked") return viewNotChecked();
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
