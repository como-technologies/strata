"use strict";
/* The screens of the page, and the alerts. The head of app.js gives the order of the scripts. */

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

// newerCard draws the newer versions: four tiles, then one card with a table or a timeline, or the splash of an empty list.
const NEWER_KEYS = {pack: [r => r.name, "text"], move: [r => r.newest, "version"], impact: [r => rank(Object.keys(IMPACT))(r.impact), "num"],
  settings: [r => r.fold.settings, "num"], images: [r => r.images, "num"], charts: [r => r.charts, "num"], listed: [r => when(r.listed) || 0, "num"]};
const NEWER_VIEWS = [["table", "Table"], ["timeline", "Timeline"]];
const REPAVES = ["repave", "may-repave"];
// stepsOf gives the newer versions of a row from the oldest to the newest. An observer of an older release gives the newest alone.
const stepsOf = r => r.steps && r.steps.length ? r.steps : [{version: r.newest, listed: r.listed}];
// buildOnly says that a move changes no value, manifest, image, or chart: only the build of the pack moves.
const buildOnly = r => r.fold.settings + r.fold.followsTheVersion + r.fold.repeatsTheLists === 0 && !r.images && !r.charts && !(r.flags || []).includes("AUTHORED");

// newerGroups groups the rows by profile, and keeps what the filter matches: a whole group when its profile matches, and else the
// rows whose pack matches.
function newerGroups(rows) {
  const groups = [];
  rows.forEach(r => r.profiles.forEach(s => {
    let g = groups.find(q => q.subject === s);
    if (!g) { g = {subject: s, rows: []}; groups.push(g); }
    g.rows.push(r);
  }));
  groups.sort((a, b) => a.subject.localeCompare(b.subject));
  return groups.map(g => matches("newer", g.subject) ? g : {...g, rows: g.rows.filter(r => matches("newer", r.name))}).filter(g => g.rows.length);
}

function newerTiles(rows) {
  const n = D.cluster.counts, others = packsWithoutMove();
  const repave = rows.filter(r => REPAVES.includes(r.impact)).length, fresh = rows.filter(r => r.new).length;
  const behind = rows.reduce((sum, r) => sum + stepsOf(r).length, 0);
  const T = (cls, label, value, sub) => h`<div class="tile ${cls}"><div class="l">${label}</div><div class="v">${value}</div><div class="s">${sub}</div></div>`;
  return h`<section class="tiles" aria-label="Newer versions in numbers">
    ${T("b", "Packs behind", rows.length, `in ${n.newerProfiles} of ${n.profiles} ${plural(n.profiles, "profile", "profiles")}`)}
    ${T(repave ? "a" : "g", "Moves that repave", repave, repave ? "The move replaces the nodes" : "No move replaces the nodes")}
    ${T("b", "New this week", fresh, "Listed in the last 7 days")}
    ${T("y", "Versions behind", behind, `${others.current} other ${plural(others.current, "pack is", "packs are")} current`)}
  </section>`;
}

function newerGroupHead(g, cols) {
  const p = profileOf(g.subject), total = p ? p.packs.length : "";
  const count = h`<span class="mut">· ${g.rows.length}${total !== "" ? h` of ${total}` : ""} ${plural(total === "" ? g.rows.length : total, "pack", "packs")}</span>`;
  return cols ? h`<tr class="g"><td colspan="${cols}"><a class="m" href="${profileHash(g.subject)}">${subjectRef(g.subject)}</a> ${count}</td></tr>`
    : h`<div class="lane-g"><a class="m" href="${profileHash(g.subject)}">${subjectRef(g.subject)}</a> ${count}</div>`;
}

function newerTable(groups) {
  const none = h`<tr><td colspan="8">${inline(TONE.OFF, h`No pack or profile matches <span class="m">${S.qs.newer}</span>.`)}</td></tr>`;
  return h`<div class="tw"><table class="tree"><thead><tr>${sortTh("nt", "pack", "Pack")}${sortTh("nt", "move", "Move")}${sortTh("nt", "impact", "Impact")}${sortTh("nt", "settings", "Settings", "n")}${sortTh("nt", "images", "Images", "n narrow-hide")}${sortTh("nt", "charts", "Charts", "n narrow-hide")}${sortTh("nt", "listed", "Listed", "narrow-hide")}<th class="narrow-hide" title="${ASK_WHAT}">Palette</th></tr></thead><tbody>${groups.length ? groups.map(g => {
    const p = profileOf(g.subject);
    return [newerGroupHead(g, 8), sortBy("nt", g.rows, NEWER_KEYS).map(r => {
      const k = packOfRow(p, r), href = k ? packHash(g.subject, k) : profileHash(g.subject);
      const unread = moveUnread(g.subject, k), same = !unread && buildOnly(r);
      const count = v => unread ? DASH : same ? mut(DASH) : v;
      const settings = same ? h`<span class="mut" title="Only the build of the pack changes: the values stay the same">same</span>` : h`<b>${count(r.fold.settings)}</b>`;
      return h`<tr class="link" data-go="${href}"><td class="m"><a href="${href}">${r.name}</a></td><td class="m nw">${r.version}${ARROW}${r.newest}</td><td>${impactCell(g.subject, k, r)}</td><td class="n">${settings}</td><td class="n narrow-hide">${count(r.images)}</td><td class="n narrow-hide">${count(r.charts)}</td><td class="m mut narrow-hide nw">${day(r.listed)}${newChip(r.new)}</td><td class="narrow-hide">${k ? askCell(g.subject, k) : ""}</td></tr>`;
    })];
  }) : none}</tbody></table></div>`;
}

// newerTimeline draws one lane for each pack: the pinned version, each version that the registry listed after it, and the newest,
// with the day of each. The lane links to the page of the pack.
function newerTimeline(groups) {
  if (!groups.length) return h`<div class="body">${inline(TONE.OFF, h`No pack or profile matches <span class="m">${S.qs.newer}</span>.`)}</div>`;
  return h`<div class="lanes">${groups.map(g => {
    const p = profileOf(g.subject);
    return [newerGroupHead(g, 0), sortBy("nt", g.rows, NEWER_KEYS).map(r => {
      const k = packOfRow(p, r), href = k ? packHash(g.subject, k) : profileHash(g.subject), steps = stepsOf(r);
      const step = (cls, version, when, word) => h`<span class="step ${cls}"><i></i><b>${version}</b><span>${when}${word ? h` · ${word}` : ""}</span></span>`;
      return h`<a class="lane" href="${href}"><span class="lname m">${r.name}</span><span class="steps">${step("pin", r.version, "pinned", "")}${steps.map((x, i) => step(i === steps.length - 1 ? "newest" : "", x.version, day(x.listed), i === steps.length - 1 && r.new ? "new" : ""))}</span><span class="limp">${impactBadge(r.impact)}</span></a>`;
    })];
  })}</div>`;
}

function newerCard() {
  const n = D.cluster.counts, rows = D.newer;
  if (!rows.length) return h`<section class="card"><div class="chead"><h2>Newer versions <span class="sub">${num(rows.length)}</span></h2></div>${noNewer(SIZE.CARD)}</section>`;
  const groups = newerGroups(rows), view = S.newerView || "table";
  const seg = h`<span class="seg" role="group" aria-label="View">${NEWER_VIEWS.map(([k, l]) => h`<button data-nview="${k}" aria-pressed="${view === k ? "true" : "false"}" class="${view === k ? "on" : ""}">${l}</button>`)}</span>`;
  const sub = h`<span class="sub">${rows.length} in ${n.newerProfiles} ${plural(n.newerProfiles, "profile", "profiles")}</span>`;
  const note = view === "table" ? h`<div class="body small mut pb9"><b>Settings</b> counts what a move really changes. Impact is a hint from strata's table, so read the two together.</div>`
    : h`<div class="body small mut pb9">Each lane runs from the pinned version to the newest one, with the day that the registry listed each version.</div>`;
  return h`${newerTiles(rows)}<section class="card"><div class="chead"><h2>Newer versions ${sub}</h2><div class="tb">${seg}${filterBox("newer", "Filter by pack or profile")}</div></div>${note}${view === "table" ? newerTable(groups) : newerTimeline(groups)}</section>`;
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
      const why = r.cause === CAUSE.OLD_COPY && r.registry ? `Current against an older copy of ${r.registry}. A newer version can be missing here.`
        : r.cause === CAUSE.NO_RECORD && D.cluster.records.baseline === BASELINE.FIRST_SEEN ? NO_RECORD_FIRST_SEEN : (CAUSE_TEXT[r.cause] || r.cause);
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
  return h`<div class="tw"><table><thead><tr>${sortTh("it", "status", "Status")}${sortTh("it", "image", "Image")}${sortTh("it", "pack", "Pack", "narrow-hide")}${sortTh("it", "pods", "Pods", "n narrow-hide")}<th>Digest</th></tr></thead><tbody>${shown.length ? shown.map(imageRow) : none}</tbody></table></div>`;
}

function viewImages() {
  const o = D.images, n = o.counts, rest = n.images - n.matched;
  const sub = podsRead() ? h`${n.images} ${plural(n.images, "image", "images")}. ${n.matched} ${plural(n.matched, "matches", "match")} a pack. ${rest} ${plural(rest, "does", "do")} not.` : podsOff() ? "Pods not read" : "Not read yet";
  const note = podsRead() && o.images.length ? h`<div class="note small">This page says so when what this cluster pulled differs from what it first pulled, or when two pods of one tag differ. It cannot say whether a tag moved at its source.</div>` : "";
  return {nav: "images", crumbs: crumbs("Images"), body: h`${alertBars()}<section class="card"><div class="chead"><h2>Images <span class="sub">${sub}</span></h2>${podsRead() && o.images.length ? h`<div class="tb">${filterBox("images", "Filter by image or pack")}</div>` : ""}</div>${imagesBody()}</section>${note}`};
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
