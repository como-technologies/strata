"use strict";
/* The comparison of a pack, and the card "Check this yourself". The head of app.js gives the order of the scripts. */

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
  return h`<div class="body">${inline(TONE.SAME, `No ${noun} changed${other ? " here" : ""}.`, link)}</div>`;
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
   record against the copy of this cluster, the pack against its newer version, and the record against the newest version of
   its profile. The card opens on the first side that holds a change. A side with no change sends no request, and it shows the
   facts of the three sides. */

// The commands of the card "Check this yourself" and of the splash for a text above the cap. record.json is the file that
// the first row of the card writes. Each value from the data goes through shq(), so a command pastes as the arguments it means.
const publishedCommand = subject => `strata compare ${RECORD_FILE} ${shq(subject)} --full`;
const clusterCommand = (x, full) => `strata compare ${RECORD_FILE} ${shq(`cluster:${clusterName()}/${x.name}`)}${full ? " --full" : ""}`;
// packRef gives a pack of a registry as one word of a shell.
const packRef = (row, version) => shq(`pack:${row.registryUid}/${row.name}@${version}`);
const moveCommand = row => `strata compare ${packRef(row, row.version)} ${packRef(row, row.newest)} --full`;
// newestCommand compares the record with the newest version of its profile, as the versions cycle does.
const newestCommand = x => `strata compare ${RECORD_FILE} ${shq(at(x.name, x.newerVersion.version))} --full`;
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
// page of the pack. A dim flag is no link. With no id, as for the move of a whole profile, no flag is a link.
function flagRow(d, id, href) {
  return h`<span class="flags">${FLAGS.map(f => {
    const lit = d.flags.includes(f), tab = lit && id ? flagTab(f, d) : "", w = FLAG_WORDS[f], cls = lit ? "b" : "y off";
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
      if (sameContent(data)) return {flags: flagRow(data, id, href), inner: h`${buildTable(data)}<div class="body">${inline(TONE.SAME, sameContentWords(data.impact.text))}</div>`};
      const panel = tab === "manifests" ? manifestsPanel(data, id, command) : tab === "images" ? changesPanel(data.imageChanges, "image", "Repository", id, data) : tab === "charts" ? changesPanel(data.chartChanges, "chart", "Chart", id, data) : valuesPanel(data, id, command);
      return {flags: flagRow(data, id, href), inner: h`${buildTable(data)}${tabBar(id, tabCounts(data), tab)}${panel}`};
    }
    default: return {flags: "", inner: retrySplash(d)};
  }
}

// sidesOf lists the comparisons that a pack can show: the record against the published version, against the copy of this cluster when
// strata read it, the pack against its newer version, and the record against the newest version of its profile when strata compared them.
function sidesOf(x, row) {
  const out = [[SIDE.PUBLISHED, "Record → Published now"]];
  const cp = x.clusterCopy && x.clusterCopy.status;
  if (COPY_READ.includes(cp)) out.push([SIDE.CLUSTER, "Record → This cluster's copy"]);
  if (row) out.push([SIDE.NEWER, `${at(row.name, row.version)} → ${at(row.name, row.newest)}`]);
  if (x.newerVersion && x.newerVersion.move) out.push([SIDE.NEWEST, "Record → Newest version"]);
  return out;
}

// changedSides says which sides of a pack hold a change. It reads the findings, the move of the pack, and the move of its profile, and
// it sends no request. A finding of the pods says nothing about the sides of a record.
const CLUSTER_KINDS = [KIND.CLUSTER, KIND.IMAGE_ID];
function changedSides(subject, x, p, row) {
  const fs = D.findings.filter(f => f.profile === subject && f.packs.includes(p.key) && !IMAGE_KINDS.includes(f.kind));
  return {
    [SIDE.PUBLISHED]: fs.some(f => !CLUSTER_KINDS.includes(f.kind)) || (p.status === STATUS.DRIFT && fs.length === 0),
    [SIDE.CLUSTER]: fs.some(f => CLUSTER_KINDS.includes(f.kind) || f.alsoOnCluster),
    [SIDE.NEWER]: !!row,
    [SIDE.NEWEST]: !!movePackOf(x, p.key),
  };
}

// publishedUnread says that strata could not read the published version, or could not verify it. No answer is not an identical copy.
const publishedUnread = x => !(x.published && (x.published.status === VERIFY.UNCHANGED || x.published.status === VERIFY.DRIFTED));
// quietTone is the tone of a place that says "no change": calm only when the check allows it.
const quietTone = () => calmOk() ? TONE.CALM : TONE.OFF;

// sideFacts gives the facts of a pack: the published version, the copy of this cluster, and the newer version. A pack that strata does
// not look for has no fact of a newer version.
function sideFacts(x, p, changed) {
  const copy = x.clusterCopy && x.clusterCopy.status;
  const readCopy = COPY_READ.includes(copy);
  const published = publishedUnread(x) ? ["Not read", "Published now"] : changed[SIDE.PUBLISHED] ? ["Changed", "Published now"] : ["Same", "Published now", "ok"];
  const cluster = readCopy ? (changed[SIDE.CLUSTER] ? ["Changed", "This cluster's copy"] : ["Same", "This cluster's copy", "ok"]) : copy === COPY.NOT_ON_CLUSTER ? ["Not here", "This cluster's copy"] : ["Not read", "This cluster's copy"];
  const newer = newerFact(x, p);
  return newer ? [published, cluster, newer] : [published, cluster];
}

// newerFact gives the fact of the newer version of the pack p of the profile x, or null for a pack that strata does not look for.
function newerFact(x, p) {
  switch (newerStateOf(x, p)) {
    case NEWER_STATE.NEWER: return [p.newer, "Newer version"];
    case NEWER_STATE.NOT_CHECKED: return ["Not checked", "Newer version"];
    case NEWER_STATE.NOT_READ: return ["Not read", "Newer version"];
    case NEWER_STATE.CURRENT: return ["Current", "No newer version", "ok"];
    default: return null;
  }
}

// undisturbed is the splash of a side with no change.
// undisturbed is the splash of a side that strata read and found the same as the record. That is a result of a check, so it takes the
// mark SAME wherever the rest of the cluster stands. The outline is for a place that is off, or that has nothing yet.
const UNDISTURBED_WORDS = {
  [SIDE.CLUSTER]: "The copy of this cluster holds what the record holds.",
  [SIDE.NEWEST]: "The newest version of the profile holds what the record holds for this pack.",
};
function undisturbed(side, list) {
  return splash(TONE.SAME, "Undisturbed", UNDISTURBED_WORDS[side] || "The published version holds what the record holds.", facts(list), SIZE.CARD);
}

// notCheckedSplash is the splash of a pack that strata did not check in full. It names the cause, and it points to the screen of the causes.
const notCheckedSplash = x => splash(TONE.WARN, "Part of this layer is hidden", h`This pack was not fully checked. ${notCheckedLine(x)} The content check runs every ${D.cluster.content.every}.`,
  next(h`<a href="#notchecked">Why, on the Not checked page</a>`), SIZE.CARD);

function comparePack(subject, x, p, row) {
  const sides = sidesOf(x, row), changed = changedSides(subject, x, p, row), id = "side-" + subject + "-" + p.key;
  const chosen = S.side[id], side = sides.some(([k]) => k === chosen) ? chosen : (sides.find(([k]) => changed[k]) || sides[0])[0];
  const pub = x.published, cp = x.clusterCopy;
  const unread = side === SIDE.PUBLISHED && publishedUnread(x);
  const select = h`<select id="side" class="search auto" data-id="${id}" aria-label="Compare">${sides.map(([k, l]) => h`<option value="${k}" ${k === side ? raw("selected") : ""}>${l}</option>`)}</select>`;
  const to = side === SIDE.CLUSTER ? cp && cp.rootB : pub && pub.rootLive;
  const roots = VERSION_SIDES.includes(side) ? "" : h`<span class="m mut">root ${root(pub && pub.rootRecorded)}${ARROW}${root(to)}</span>`;
  const tabId = side === SIDE.NEWER ? "move-" + subject + "-" + p.key : id + "-" + side;
  const command = side === SIDE.CLUSTER ? clusterCommand(x, true) : side === SIDE.NEWER ? moveCommand(row) : side === SIDE.NEWEST ? newestCommand(x) : publishedCommand(subject);
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
    inner = undisturbed(side, sideFacts(x, p, changed));
  } else {
    ({flags, inner} = comparePanel(tabId, d, packHash(subject, p), command));
    // A newer version is no drift: its side keeps the neutral badge of a newer version, as each other level does.
    const changedBadge = VERSION_SIDES.includes(side) ? statusBadge(STATUS.NEWER) : badge("r", "Changed");
    state = d.state === "ok" ? changedBadge : "";
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
  // The facts of the move sit in one row of labeled values. The panel "Ask Palette" sits beside them in its own box.
  const fact = (label, value) => h`<div><span>${label}</span>${value}</div>`;
  const facts = buildOnly ? [fact("Content", h`<b>The same</b>`), fact("Changes", "Only the build of the pack")]
    : [fact("Settings", unread ? DASH : h`<b>${f.settings}</b> of ${total} ${plural(total, "value", "values")}`), fact("Images", count(row.images)), fact("Charts", count(row.charts))];
  if (row.drops.length) facts.push(fact("Drops", row.drops.join(", ")));
  if (row.floating) facts.push(fact("Tag", "Floating"));
  const why = unread ? "Strata could not compare these versions now, so the counts are not known."
    : `${impactOf(row.impact)[1]} is a hint from strata's table${row.impactText ? `: ${row.impactText}` : ""}.`;
  return h`<section class="card"><div class="chead"><h2>Move to ${ref(row.name, row.newest)} <span class="sub">Listed ${day(row.listed)}</span></h2>${impactCell(subject, p, row)}</div>
    <div class="move"><div class="movefacts"><div class="kv">${facts}</div><div class="small mut">${why}${pinned ? ` The pinned version is ${pinned} in the registry.` : ""}</div></div>
    <div class="askbox"><div class="askh">Ask Palette</div>${askBlock(subject, p)}</div></div></section>`;
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
  // The page of a pack shows the move of its profile in the card of its comparison, so only the page of the profile has this row.
  if (!p && x.newerVersion) rows.push(["What does the move to the newest version change?", newestCommand(x), newestImpact(x.newerVersion)]);
  return rows;
}

// newestImpact gives the impact of the move of a profile to its newest version, or "Not read" when strata could not compare them.
const newestImpact = n => n.move ? impactBadge(n.move.impact) : badge("y", "Not read");

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
    <div class="pside">${packStack(subject, x, true, p.key)}</div></div></section>${row ? moveCard(subject, p, row) : ""}${comparePack(subject, x, p, row)}${checkCard(terminalRows(subject, x, p, row))}`};
}
