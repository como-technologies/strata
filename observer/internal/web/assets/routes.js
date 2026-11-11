"use strict";
/* The routes, the render, and the events of the page. This script starts the page, so index.html loads it last. */

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

// put writes markup to one place of the page. It is the one write of markup in the scripts of the page.
// put writes markup into an element. In a browser it parses the markup in a template, and patches the nodes of the element in
// place: each node that stays, stays, so a read of the observer does not flicker, and the scroll, the focus, an open card, and the
// text of a filter stay. With no DOM, as in the tests with node, it writes the markup into the element itself.
function put(dest, markup) {
  const inPlace = typeof document.createElement === "function";
  const el = inPlace ? document.createElement("template") : dest;
  el.innerHTML = part(markup);
  if (inPlace) patch(dest, el.content);
}

// sameAttributes says that two elements carry the same attributes with the same values.
const sameAttributes = (x, y) => x.attributes.length === y.attributes.length && [...y.attributes].every(a => x.getAttribute(a.name) === a.value);

// patch makes the children of from equal to the children of to. A text that changed takes the new words. An element with the same
// name and the same attributes stays, and its children are patched. Any other node takes the place of the old one. The field that a
// person types in stays as it is.
function patch(from, to) {
  const now = [...to.childNodes];
  now.forEach((y, i) => {
    const x = from.childNodes[i];
    if (!x) { from.appendChild(y); return; }
    if (x.nodeType === 1 && x === document.activeElement && x.nodeName === "INPUT") return;
    if (x.nodeType !== y.nodeType || x.nodeName !== y.nodeName || (x.nodeType === 1 && !sameAttributes(x, y))) { from.replaceChild(y, x); return; }
    if (x.nodeType !== 1) { if (x.nodeValue !== y.nodeValue) x.nodeValue = y.nodeValue; return; }
    patch(x, y);
  });
  while (from.childNodes.length > now.length) from.removeChild(from.lastChild);
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
  if (!ev.target.closest(".cyclemenu")) closeMenus();
  const nw = ev.target.closest("[data-now]");
  if (nw) { checkNow(nw.dataset.now); return; }
  const t = ev.target.closest("[data-tab]");
  if (t) { S.tabs[t.dataset.tab] = t.dataset.k; render(); return; }
  const f = ev.target.closest("[data-filter]");
  if (f) { S.filter = f.dataset.filter; render(); return; }
  const v = ev.target.closest("[data-view]");
  if (v) { S.view = v.dataset.view === "split" ? "split" : "unified"; render(); return; }
  const nv = ev.target.closest("[data-nview]");
  if (nv) { S.newerView = nv.dataset.nview === "timeline" ? "timeline" : "table"; render(); return; }
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
    if (!k) return;
    S.open[k] = ev.target.open;
    // the menu of a cycle draws its panel when it opens, and one menu of a cycle is open at a time
    if (k.startsWith("cycle-")) {
      if (ev.target.open) Object.keys(CYCLES).map(menuKey).filter(o => o !== k).forEach(o => { S.open[o] = false; });
      render();
    }
  }, true);
  document.addEventListener("keydown", ev => { if (ev.key === "Escape") closeMenus(); });
  addEventListener("hashchange", () => { render(); scrollTo(0, 0); });
  // A turn of a phone, or a window made narrow, changes the layout of a diff.
  if (typeof matchMedia === "function") matchMedia(NARROW).addEventListener("change", render);
  render();
  refresh();
  setInterval(refresh, REFRESH_MS);
}

start();
