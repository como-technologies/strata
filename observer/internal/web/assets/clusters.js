"use strict";
/* The screens of a set of clusters: the clusters, the profile versions that they share, and the frame of a cluster of the set. The
   head of app.js gives the order of the scripts. With no --clusters, none of this shows, and the page is the page of one cluster. */

/* ---- The mode and the links ----
   SET is false for the observer of one cluster and true for a set. The page knows it at its first read: a set serves no route of one
   cluster at the top, so /v1/cluster answers 404. S.cluster is the uid of the cluster of the screen, and null on a screen of the set.
   S.project is the project of the choice of the project, and "" for each project. */
let SET = false;
S.cluster = null;
S.project = "";

// clusterOfHash reads the uid of the cluster of a hash #c-<uid>/<screen>. It gives null for a screen of the set, and for a page of one
// cluster.
function clusterOfHash(hash) {
  const m = /^#?c-([^/]*)/.exec(hash || "");
  if (!SET || !m) return null;
  try { return decodeURIComponent(m[1]); } catch (err) { return null; }
}

// link gives the hash of a screen of the cluster of the page: #<screen> for one cluster, and #c-<uid>/<screen> for a cluster of a set.
const link = k => SET && S.cluster !== null ? `#c-${enc(S.cluster)}/${k}` : `#${k}`;
// apiBase starts each route of the cluster of the page.
const apiBase = () => SET && S.cluster !== null ? `/v1/clusters/${enc(S.cluster)}` : "/v1";
// profileKey names a profile in a hash and in a route. A cluster of a set names it by its uid, because two projects can hold one name
// and version. One cluster names it by name@version. subjectOfKey reads the key back.
const profileKey = subject => { const p = SET ? profileOf(subject) : null; return p && p.uid ? p.uid : subject; };
const subjectOfKey = key => { const p = SET ? D.profiles.find(q => q.uid === key) : null; return p ? subjectOf(p) : key; };
const profileHash = subject => link(`p-${enc(profileKey(subject))}`);
const packHash = (subject, p) => link(`k-${enc(profileKey(subject))}/${enc(p.layer)}/${enc(p.name)}`);
const imageHash = ref => link(`i-${enc(ref)}`);
// clusterHash gives the first screen of one cluster of the set.
const clusterHash = uid => `#c-${enc(uid)}/overview`;

/* ---- The data of a set ---- */
// setData reads the summary of the set and the shared versions. On a screen of one cluster, it also reads the routes of that cluster.
// On a screen of the set, the frame of the set stands for the cluster. So the top bar shows the two clocks of the set. D.wanted is the
// cluster that the read was for.
async function setData() {
  const uid = clusterOfHash(location.hash);
  const [set, shared, alerts] = await Promise.all([getJSON("/v1/clusters"), getJSON("/v1/shared"), getJSON("/v1/alerts").catch(() => null)]);
  const known = uid !== null && set.clusters.some(c => c.uid === uid);
  const part = known ? await clusterData(`/v1/clusters/${enc(uid)}`) : setFrame(set, alerts);
  return {...part, set, shared: shared.shared, wanted: uid};
}

// setFrame gives the parts of the data that the frame of the page reads, for a screen of the set. They are the page, the records, the
// two clocks, and the counts. It names no cluster, and it holds no finding and no pod.
function setFrame(set, alerts) {
  const n = set.counts;
  const counts = {findings: n.findings, drift: n.drift, newer: n.newer, notChecked: n.notChecked, packs: 0, packsInFull: 0, profiles: 0, newerProfiles: 0, newerProfileVersions: 0};
  const cluster = {cluster: {uid: "", name: ""}, page: set.page, strataVersion: set.strataVersion, records: set.records, content: set.content, versions: set.versions, counts};
  const rules = alerts && Array.isArray(alerts.rules) ? alerts : D ? D.alerts : NO_ALERTS;
  return {cluster, findings: [], profiles: [], newer: [], notChecked: [], limits: {}, events: [], images: {pods: PODS.OFF, images: [], counts: {images: 0, matched: 0, changed: 0}}, asks: [], alerts: rules};
}

// wrongCluster says that the data is not for the cluster of the hash: a person opened another screen since the last read.
const wrongCluster = () => SET && D.wanted !== clusterOfHash(location.hash);
// rowOfCluster finds the row of a cluster of the set. here says that the observer runs on the cluster of the screen, so only it reads pods.
const rowOfCluster = uid => D.set.clusters.find(c => c.uid === uid);
const elsewhere = () => SET && S.cluster !== null && !(rowOfCluster(S.cluster) || {}).here;
// inProject says that the project of the choice of the project holds a cluster or a shared version.
const inProject = uid => !S.project || uid === S.project;

/* ---- The frame of a set ---- */
const ROW = {DRIFT: "drift", DIFFERS: "differs", NOT_CHECKED: "not-checked", OK: "ok"};
const rowBadge = r => r.status === ROW.DRIFT ? badge("r", `Drift ${r.drift}`) : r.status === ROW.DIFFERS ? badge("r", "Differs")
  : r.status === ROW.NOT_CHECKED ? badge("y", "Not checked") : r.status === ROW.OK ? badge("g", "OK") : badge("y", r.status);
const ELSEWHERE_WORDS = "The observer reads the pods of its own cluster only.";

// setNavLinks gives the nav of a screen of the set: the clusters, the shared versions, and the alerts.
function setNavLinks(cur) {
  const rows = D.set.clusters.filter(c => inProject(c.project.uid));
  const items = [["clusters", "Clusters", h`<span class="cnt">${known(rows.length)}</span>`], ["shared", "Shared versions", h`<span class="cnt">${known(D.shared.length)}</span>`],
    ["alerts", "Alerts", alertsCount()]];
  return items.map(([k, l, cnt]) => h`<a href="#${k}" class="${k === cur ? "on" : ""}"><span>${l}</span>${cnt}</a>`);
}

// clustersLink is the first item of the nav of a cluster of the set: the way back to the screen of the clusters.
const clustersLink = () => h`<a href="#clusters"><span>Clusters</span><span class="cnt">${D.set.counts.clusters}</span></a>`;

// clusterCrumbs puts the set and the cluster before the crumbs of a screen of one cluster: Clusters / cluster / screen.
function clusterCrumbs(cr) {
  const r = rowOfCluster(S.cluster), name = r ? r.name || r.uid : S.cluster;
  return h`<a href="#clusters">Clusters</a> <span>/</span> <a href="${clusterHash(S.cluster)}">${name}</a> <span>/</span> ${cr}`;
}

// projectChoice filters the screens of the set by project. It lists each project that has a key.
function projectChoice() {
  const p = D.set.projects;
  return h`<select class="search" id="project" aria-label="Project"><option value=""${S.project ? "" : raw(" selected")}>All projects</option>${p.map(x => h`<option value="${x.uid}"${S.project === x.uid ? raw(" selected") : ""}>${x.name || x.uid}</option>`)}</select>`;
}

// selectorBars tells each selector that selects no cluster, and each selector whose search failed. Neither state is calm.
function selectorBars() {
  return D.set.selectors.filter(s => s.unread || !s.clusters).map(s => s.unread
    ? h`<div class="alertbar a">The search of <span class="m">${s.selector}</span> failed in a project. <span>The clusters of that project are the ones of its last good search.</span></div>`
    : h`<div class="alertbar a">No cluster matches <span class="m">${s.selector}</span>. <span>Check the label on the clusters, or the saved filter in Palette.</span></div>`);
}

// setCalm says that the screen of the clusters can read calm. The checks can speak, each selector selects a cluster, and each cluster is OK
// with no cause of Not checked. A cluster that is not checked keeps the calm away, so the state of the set is never better than its worst
// cluster. A cause of Not checked keeps it away too, because it limits what a check says.
const setCalm = rows => calmOk() && rows.length > 0 && rows.every(r => r.status === ROW.OK && !r.notChecked) && !D.set.selectors.some(s => s.unread || !s.clusters);

/* ---- The screen of the clusters ---- */
function viewClusters() {
  return withChecks("clusters", crumbs("Clusters"), () => {
    const rows = D.set.clusters.filter(c => inProject(c.project.uid));
    return h`${selectorBars()}${setTiles(rows)}${clustersCard(rows)}`;
  });
}

function setTiles(rows) {
  const T = (cls, label, value, sub) => h`<a class="tile ${cls}" href="#clusters"><div class="l">${label}</div><div class="v">${value}</div><div class="s">${sub}</div></a>`;
  const drift = rows.filter(r => r.status === ROW.DRIFT || r.status === ROW.DIFFERS).length, newer = rows.filter(r => r.newer > 0).length;
  const unchecked = rows.filter(r => r.status === ROW.NOT_CHECKED).length, projects = new Set(rows.map(r => r.project.uid)).size;
  return h`<section class="tiles" aria-label="Summary">
    ${T("", "Clusters", rows.length, `in ${projects} ${plural(projects, "project", "projects")}`)}
    ${T(drift ? "r" : "g", "Drift", drift, drift ? `${drift} ${plural(drift, "cluster", "clusters")}` : setCalm(rows) ? "Nothing changed" : "None in what was checked")}
    ${versionsRead() ? T("b", "Newer versions", newer, `${newer} ${plural(newer, "cluster", "clusters")}`) : T("y", "Newer versions", "-", "Not read yet")}
    ${T("y", "Not checked", unchecked, `${unchecked} ${plural(unchecked, "cluster", "clusters")}`)}
  </section>`;
}

// causesLine counts the causes of Not checked of a cluster that the observer checked. They limit its check, and they change no status.
const causesLine = r => r.status !== ROW.NOT_CHECKED && r.notChecked > 0
  ? h`<div class="small mut">${r.notChecked} ${plural(r.notChecked, "cause", "causes")} in Not checked</div>` : "";

function clustersCard(rows) {
  const head = h`<thead><tr><th>Status</th><th>Cluster</th><th class="narrow-hide">Project</th><th class="n narrow-hide">Profiles</th><th>Checked</th></tr></thead>`;
  const row = r => {
    const href = clusterHash(r.uid), err = r.lastError ? h`<div class="small mut">${errorWords(r.lastError)}</div>` : "";
    const here = r.here ? h` <span class="mut small">runs the observer</span>` : "";
    return h`<tr class="link" data-go="${href}"><td class="w110">${rowBadge(r)}</td><td class="m"><a href="${href}">${r.name || r.uid}</a>${here}${err}${causesLine(r)}</td><td class="narrow-hide">${r.project.name || r.project.uid}</td><td class="n narrow-hide">${r.profiles}</td><td class="mut nw">${r.checkedAt ? `${ago(r.checkedAt)} ago` : "not yet"}</td></tr>`;
  };
  const empty = h`<tr><td colspan="5">${inline(TONE.OFF, "No cluster is in the set.")}</td></tr>`;
  return h`<section class="card"><div class="chead"><h2>Clusters <span class="sub">${rows.length}, the worst first</span></h2></div><div class="tw"><table>${head}<tbody>${rows.length ? rows.map(row) : empty}</tbody></table></div></section>`;
}

/* ---- The screen of the shared versions ---- */
function viewShared() {
  return withChecks("shared", crumbs("Shared versions"), sharedCard);
}

// copiesCell tells how many copies of a profile version match its record, with a link to each cluster whose copy differs.
function copiesCell(s) {
  const differ = s.copies.differ.map(c => h` <a class="m" href="#c-${enc(c.uid)}/p-${enc(s.uid)}">${c.name || c.uid}</a>`);
  const unread = s.copies.notRead ? h`<div class="small mut">${s.copies.notRead} not read</div>` : "";
  return h`${s.copies.same} of ${s.copies.of} same${differ.length ? h`<div class="small">differs on${differ}</div>` : ""}${unread}`;
}

// SCOPE names the scope of a profile that the screen of the shared versions tells apart.
const SCOPE = {PROJECT: "project"};
// projectName gives the name of a project of the set, by its uid.
const projectName = uid => { const p = D.set.projects.find(x => x.uid === uid); return p ? p.name || p.uid : uid; };
// scopeCell names the scope of a profile version. A row of project scope also names its project, because two projects can hold
// one name and version.
const scopeCell = s => s.scope === SCOPE.PROJECT ? `${s.scope} · ${s.projects.map(projectName).join(", ")}` : s.scope || "-";

function sharedCard() {
  const rows = D.shared.filter(s => !S.project || s.projects.includes(S.project));
  const head = h`<thead><tr><th>Profile version</th><th class="narrow-hide">Scope</th><th>Clusters</th><th>Published</th><th>Copies</th></tr></thead>`;
  const row = s => h`<tr><td class="m">${ref(s.name, s.version)}</td><td class="narrow-hide">${scopeCell(s)}</td><td>${s.clusters} ${plural(s.clusters, "cluster", "clusters")} in ${s.projects.length} ${plural(s.projects.length, "project", "projects")}</td><td>${s.published ? publishedBadge(s.published) : badge("y", "No record")}</td><td>${copiesCell(s)}</td></tr>`;
  const empty = h`<tr><td colspan="5">${inline(TONE.OFF, "No cluster of the set runs a profile version yet.")}</td></tr>`;
  return h`<section class="card"><div class="chead"><h2>Shared versions <span class="sub">${rows.length}. Each profile version that a cluster of the set runs.</span></h2></div><div class="tw"><table>${head}<tbody>${rows.length ? rows.map(row) : empty}</tbody></table></div></section>`;
}

// setRoute routes a hash of a set. It gives the view of a screen of the set, or the hash of a screen of one cluster, after its prefix.
function setRoute(hash) {
  if (hash === "shared") return viewShared();
  if (hash === "alerts") return viewAlerts();
  const uid = clusterOfHash(hash);
  if (uid === null) return viewClusters();
  if (!rowOfCluster(uid)) {
    return {nav: "clusters", crumbs: crumbs(h`<a href="#clusters">Clusters</a>`, uid), body: missingSplash({title: "This cluster is not in the set", name: uid, why: "is in no selector now. A new search of the selectors can remove it.", home: ["#clusters", "All clusters"]})};
  }
  const i = hash.indexOf("/");
  return i < 0 ? "overview" : hash.slice(i + 1) || "overview";
}
