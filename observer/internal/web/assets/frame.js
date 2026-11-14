"use strict";
/* The frame of the page, and the cards of the overview. The head of app.js gives the order of the scripts. */

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
    newer: ["newer", "Newer versions", h`<span class="cnt">${versionsRead() ? num(n.newer) : "-"}</span>`],
    notchecked: ["notchecked", "Not checked", h`<span class="cnt">${num(n.notChecked)}</span>`],
    profiles: ["profiles", "Profiles", h`<span class="cnt">${known(n.profiles)}</span>`],
  };
  const items = [["overview", "Overview", ""], ...inOrder(sections), ["alerts", "Alerts", alertsCount()]];
  return items.map(([k, l, cnt]) => h`<a href="#${k}" class="${k === cur ? "on" : ""}"><span>${l}</span>${cnt}</a>`);
}

// ring draws the progress of a cycle: an arc of done in total. A cycle that does not know its work yet gets a dotted ring. Nothing
// turns: the slabs are the one thing on the page that moves, and the ring moves on at each read of the observer.
const CIRCLE = "M8 2a6 6 0 1 1 0 12a6 6 0 1 1 0-12";
function ring(r) {
  const open = r.total <= 1 && r.done === 0, f = Math.min(1, r.done / Math.max(1, r.total));
  const a = f * 2 * Math.PI, x = (8 + 6 * Math.sin(a)).toFixed(2), y = (8 - 6 * Math.cos(a)).toFixed(2);
  const arc = open ? "" : f >= 1 ? CIRCLE : f > 0 ? `M8 2A6 6 0 ${f > 0.5 ? 1 : 0} 1 ${x} ${y}` : "";
  return h`<svg class="ring${open ? " open" : ""}" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path class="track" d="${CIRCLE}"/>${arc ? h`<path class="arc" d="${arc}"/>` : ""}</svg>`;
}

// CYCLES are the two cycles that a person can start from the menu of their pill: the field of the end of the last good cycle,
// and the words of the menu.
const CYCLES = {
  content: {at: "checkedAt", label: "Content", run: "Check the content now", does: "Compares the records with the cluster and with Palette."},
  versions: {at: "readAt", label: "Versions", run: "Read the versions now", does: "Reads the registries for newer versions of each pack."},
};
// NOW_WAIT_MS ends the wait of the page for the cycle of a click when no read of the observer shows that cycle.
const NOW_WAIT_MS = 60000;
const menuKey = name => `cycle-${name}`;

// runningOf gives the progress that the pill of a cycle shows: the run of the observer, or the click that waits for its run.
function runningOf(name, c) {
  const p = S.now[name];
  return c.running || (p ? {startedAt: new Date(p.sent).toISOString(), done: 0, total: 0} : null);
}

// runningPill is the pill of a cycle that runs now: its progress, and the age of the last good cycle of its kind. A pill is
// [class, title, markup].
function runningPill(label, r, last) {
  const what = r.total > 1 || r.done > 0 ? `checking ${r.done} of ${r.total}` : "checking";
  return ["chip run", `Started ${ago(r.startedAt)} ago`, h`${ring(r)}${label} <b>${what}</b>${last ? h` · last ${ago(last)} ago` : ""}`];
}

function contentPill(content, lost, stale) {
  const r = runningOf("content", content);
  if (r && !lost) return runningPill("Content", r, content.checkedAt);
  if (lost) return ["chip", content.checkedAt || "", h`<span class="dot r"></span>Palette <b>lost</b>${content.checkedAt ? h` · content checked <b>${ago(content.checkedAt)} ago</b>` : ""}`];
  if (!content.checkedAt) return ["chip", "", h`<span class="dot a"></span>Content <b>not checked yet</b> · every ${content.every}`];
  const dot = stale || content.failedRuns > 0 ? "a" : "";
  return ["chip", content.checkedAt, h`<span class="dot ${dot}"></span>Content <b>${ago(content.checkedAt)} ago</b> · every ${content.every}`];
}

function versionsPill(versions) {
  const r = runningOf("versions", versions);
  if (r) return runningPill("Versions", r, versions.readAt);
  if (!versions.readAt) return ["chip", "", h`<span class="dot a"></span>Versions <b>not read yet</b> · every ${versions.every}`];
  const dot = versions.failedRuns > 0 ? "a" : "";
  return ["chip", versions.readAt, h`<span class="dot ${dot}"></span>Versions <b>${ago(versions.readAt)} ago</b> · every ${versions.every}`];
}

// periodMs reads a period of the observer, such as 15m or 6h0m0s, in milliseconds. A period that does not read gives NaN.
function periodMs(every) {
  const parts = String(every).match(/^(?:(\d+)h)?(?:(\d+)m)?(?:(\d+)s)?$/);
  if (!parts || !parts[0]) return NaN;
  return ((+parts[1] || 0) * 3600 + (+parts[2] || 0) * 60 + (+parts[3] || 0)) * 1000;
}

// cyclePanel is the menu of a cycle: the facts of its timer, what it does, and the button that starts it now. The button is off
// while the cycle runs, while a click waits for its cycle, and for a minute after a cycle ends, and a note says why.
function cyclePanel(name) {
  const m = CYCLES[name], c = D.cluster[name], last = c[m.at], r = runningOf(name, c);
  const facts = [["Last good end", last ? `${ago(last)} ago · ${clock(last)}` : "none yet"], ["Every", c.every]];
  if (r) {
    facts.push(["Now", r.total > 1 || r.done > 0 ? `checking ${r.done} of ${r.total}` : "checking"]);
  } else if (last && !Number.isNaN(periodMs(c.every))) {
    facts.push(["Next", `about ${clock(new Date(when(last) + periodMs(c.every)).toISOString())}`]);
  }
  if (c.failedRuns > 0) facts.push(["Failed runs", h`${c.failedRuns}${c.lastError ? h` · ${errorWords(c.lastError)}` : ""}`]);
  const left = S.nowWait[name] ? Math.ceil((S.nowWait[name] - Date.now()) / 1000) : 0;
  let note = "";
  if (c.running) note = "A cycle runs now. The pill shows its progress.";
  else if (S.now[name]) note = "Starting. The pill shows the progress.";
  else if (left > 0) note = `A cycle ended less than a minute ago. Try again in ${left} s.`;
  else if (S.nowFailed[name]) note = "The observer did not take the request. Try again.";
  const off = !!(r || left > 0);
  return h`<div class="cpanel"><div class="kv">${facts.map(([k, v]) => h`<div><span>${k}</span>${v}</div>`)}</div><p class="sub">${m.does}</p><div class="crow"><button class="btn" data-now="${name}"${off ? raw(" disabled") : ""}>${m.run}</button>${note ? h`<span class="sub">${note}</span>` : ""}</div></div>`;
}

// cycleMenu is the pill of a cycle as a menu that a click opens. The page draws the panel only while the menu is open.
function cycleMenu(name, [cls, title, inner]) {
  const k = menuKey(name);
  return h`<details class="cyclemenu" data-open="${k}"${openAttr(k)}><summary class="${cls}" title="${title}">${inner}<span class="caret" aria-hidden="true"></span></summary>${S.open[k] ? cyclePanel(name) : ""}</details>`;
}

function clocks() {
  const c = D.cluster, lost = c.page === PAGE.LOST, stale = c.page === PAGE.STALE;
  const a = cycleMenu("content", contentPill(c.content, lost, stale));
  if (stale || lost) return a;
  return [a, cycleMenu("versions", versionsPill(c.versions))];
}

// closeMenus closes the menu of each cycle. A click out of a menu, and the key Escape, call it.
function closeMenus() {
  const open = Object.keys(CYCLES).map(menuKey).filter(k => S.open[k]);
  open.forEach(k => { S.open[k] = false; });
  if (open.length) render();
}

// checkNow sends the click "Check now" of the menu of a cycle. The observer starts the cycle, joins the one that runs, or says
// how long to wait. The pill shows the progress at once, and the page reads the observer again.
async function checkNow(name) {
  const c = D.cluster[name];
  if (!CYCLES[name] || S.now[name] || c.running) return;
  delete S.nowFailed[name];
  delete S.nowWait[name];
  S.now[name] = {sent: Date.now(), last: c[CYCLES[name].at]};
  render();
  let r = null, body = null;
  try {
    r = await fetch(`/v1/cycles/${name}`, {method: "POST", cache: "no-store"});
  } catch (err) {
    r = null;
  }
  if (r) {
    try { body = await r.json(); } catch (err) { body = null; }
  }
  if (r && r.status === 429) {
    delete S.now[name];
    S.nowWait[name] = Date.now() + 1000 * (body && body.retryAfter > 0 ? body.retryAfter : 60);
  } else if (!r || !r.ok) {
    delete S.now[name];
    S.nowFailed[name] = true;
  }
  await refresh();
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
    bars.push(h`<div class="alertbar a">No pipeline approved these records yet. <span>They hold what ${from ? h`the cluster <span class="m">${from}</span>` : "a cluster"} ran when the observer first saw it. Records that your pipeline publishes from Git, with a revision, take their place.</span></div>`);
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
  // The profiles with a newer published version of their own count apart from the packs. An observer of an older release gives no count.
  const versions = n.newerProfileVersions ? ` · ${n.newerProfileVersions} newer profile ${plural(n.newerProfileVersions, "version", "versions")}` : "";
  const newerSub = (n.newer === 0 ? (calmOk(true) ? `All ${n.packs} packs current` : "None listed") : `${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")} · ${fresh} this week`) + versions;
  const notCheckedSub = n.notChecked === 0 ? `${n.packsInFull} of ${n.packs} packs in full` : `${n.notChecked} ${plural(n.notChecked, "cause", "causes")} · ${n.packsInFull} of ${n.packs} packs in full`;
  // The tile "Image changes" is there only when the observer reads the pods. The other four fill the row.
  const images = () => { const [cls, value, sub] = imageTile(); return T(cls, "Image changes", value, sub, "images"); };
  return h`<section class="tiles" aria-label="Summary">${inOrder({
    findings: T(n.drift ? "r" : "g", "Drift", n.drift, driftSub, "findings"),
    images: podsOff() ? "" : images(),
    // Before the first read of the versions ends, the page knows of no newer version, and it says so: 0 would read as "all current".
    newer: versionsRead() ? T("b", "Newer versions", n.newer, newerSub, "newer") : T("y", "Newer versions", "-", D.cluster.versions.running ? "Reading the registries" : "Not read yet", "newer"),
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
    if (f.alsoOnCluster) text += " This cluster has the edit too.";
    else if (COPY_KEPT.includes(copyStatusOf(x))) text += " This cluster keeps its copy.";
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
  const version = profileVersionNews(x);
  switch (x.status) {
    case STATUS.DRIFT: {
      const lines = D.findings.filter(f => f.profile === subject).map(f => findingLine(f, x));
      if (ms.length) lines.push(`Also ${ms.length} newer ${plural(ms.length, "version", "versions")}.`);
      const text = lines.length ? lines.join(" ") : "Drift.";
      return version ? h`${text} ${version}` : text;
    }
    case STATUS.NEWER: {
      const packs = packNews(x, subject, ms);
      return version && packs ? h`${version} ${packs}` : version || packs;
    }
    case STATUS.NOT_ON_CLUSTER: return mut(NOT_ON_CLUSTER_WORDS);
    case STATUS.NOT_CHECKED: return notCheckedLine(x);
    case STATUS.NO_RECORD: return mut("No record.");
    default: return mut("No change.");
  }
}

// packNews tells the packs of a profile that have a newer version: the move of one pack, or a count. It is "" when no pack has one.
function packNews(x, subject, ms) {
  if (!ms.length) return "";
  if (ms.length === 1) {
    const row = newerRowOf(subject, ms[0]);
    return h`${move(ms[0].name, ms[0].version, ms[0].newer)}${newChip(row && row.new)}`;
  }
  const rows = D.newer.filter(r => r.profiles.includes(subject));
  const repave = rows.filter(r => r.impact === "repave").length, may = rows.filter(r => r.impact === "may-repave").length;
  return `${ms.length} of ${x.packs.length} packs have a newer version.${repave ? ` ${repave} repave.` : ""}${may ? ` ${may} can repave.` : ""}`;
}

// profileVersionNews tells the newest published version of the profile itself, when it is newer than the record. Its words differ
// from the words of a pack that has a newer version, so a person can see the difference. It is "" when the profile has none.
function profileVersionNews(x) {
  const n = x.newerVersion;
  if (!n) return "";
  return h`Newer profile version ${ref(x.name, n.version)}.${newChip(n.new)}`;
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

// PROFILE_KEYS sort the rows of the table of the profiles and of its fold.
const PROFILE_KEYS = {status: [x => SEVERITY.indexOf(x.status), "num"], profile: [x => x.name, "text"], type: [x => x.type, "text"], packs: [x => x.packs.length, "num"]};
// folded says that the row of the profile x is in the fold under the table: its status is not-on-this-cluster. A finding wins, so a
// record with a finding stays in the table with the status drift, also when the cluster does not run it.
const folded = x => x.status === STATUS.NOT_ON_CLUSTER;
// awayFold lists under the table the rows that folded gives: records of profile versions that the cluster does not run. No filter
// counts them, and the fold shows with the filter All only.
function awayFold(rows, head) {
  if (!rows.length) return "";
  return h`<details data-open="not-on-cluster" ${openAttr("not-on-cluster")}><summary>${rows.length} ${plural(rows.length, "record of a profile version", "records of profile versions")} that this cluster does not run</summary><div class="tw"><table>${head}<tbody>${rows}</tbody></table></div></details>`;
}

function profilesCard() {
  // A profile with a newer version of itself or of a pack shows under "Newer", also when its status is a drift.
  const has = (x, k) => k === "all" || (k === STATUS.OK ? x.status === STATUS.OK : k === STATUS.NEWER ? !!x.newerVersion || x.packs.some(p => p.newer) : asCount(x.status) === k || x.packs.some(p => p.status === k));
  const list = D.profiles.filter(x => !folded(x)).sort((a, b) => SEVERITY.indexOf(a.status) - SEVERITY.indexOf(b.status));
  const cnt = {};
  FILTERS.forEach(([k]) => { cnt[k] = list.filter(x => has(x, k)).length; });
  const q = S.q.toLowerCase();
  const named = x => !q || subjectOf(x).toLowerCase().includes(q);
  const row = x => {
    const subject = subjectOf(x);
    return h`<tr class="link" data-go="${profileHash(subject)}"><td class="w110">${statusBadge(x.status)}</td><td class="m nw"><a href="${profileHash(subject)}">${x.name}</a><span class="mut">@${x.version}</span></td><td class="mut narrow-hide">${x.type}</td><td class="narrow-hide nw">${hbar(x)}</td><td class="msg">${newsOf(x)}</td></tr>`;
  };
  const rows = sortBy("pt", list.filter(x => has(x, S.filter) && named(x)), PROFILE_KEYS).map(row);
  const what = S.q ? h`No profile matches <span class="m">${S.q}</span>.` : "No profile has this status.";
  const clear = h` <button type="button" class="lnk" data-clear="filter">Clear the filter</button>`;
  const empty = h`<tr><td colspan="5">${list.length ? inline(TONE.OFF, what, clear) : inline(TONE.OFF, "No profile yet.")}</td></tr>`;
  const head = h`<thead><tr>${sortTh("pt", "status", "Status")}${sortTh("pt", "profile", "Profile")}${sortTh("pt", "type", "Type", "narrow-hide")}${sortTh("pt", "packs", "Packs", "narrow-hide")}<th>Message</th></tr></thead>`;
  const fold = S.filter === "all" ? awayFold(sortBy("pt", D.profiles.filter(x => folded(x) && named(x)), PROFILE_KEYS).map(row), head) : "";
  return h`<section class="card"><div class="chead"><h2>Profiles <span class="sub">${neverChecked() ? "Not read yet" : `${D.cluster.counts.profiles} profiles, ${D.cluster.counts.packs} packs`}</span></h2>
    <div class="tb"><div class="seg" role="group" aria-label="Filter by status">${FILTERS.map(([k, l]) => h`<button data-filter="${k}" class="${S.filter === k ? "on" : ""}">${l} ${num(cnt[k])}</button>`)}</div>
    <input class="search" id="q" type="search" placeholder="Filter by name" aria-label="Filter by name" value="${S.q}"></div></div>
    <div class="tw"><table id="pt">${head}<tbody>${rows.length ? rows : empty}</tbody></table></div>${fold}</section>`;
}
