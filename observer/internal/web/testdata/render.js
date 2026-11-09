"use strict";
// Render each screen of the page with node, with no browser. web_test.go runs it and checks the markup.
//
//   node render.js ASSETS_DIR API_DIR STATE [poison]
//
// It loads app.js in a sandbox with a fake document, and it answers each fetch from the files of one
// set in API_DIR, as the server does. It starts at #overview and follows each link of each screen, so
// it shows each screen that a person can reach. On each screen, it also shows each tab, each filter,
// each folded part, and each side. It prints one JSON list: {hash, variant, html} for each screen, or
// {hash, variant, error} for a screen that threw.
//
// With "poison", each name, path, and value of the data has the text of a markup injection in it.
//
//   node render.js ASSETS_DIR API_DIR STATE ago SECONDS...
//
// With "ago", it prints no screen. It prints one JSON list: what ago() says for each age in seconds.
//
//   node render.js ASSETS_DIR API_DIR STATE retry HASH
//
// With "retry", the detail route answers 503 once. It shows the screen HASH two times, and it prints
// one JSON list of the two renders. The second render must ask the route again.
//
//   node render.js ASSETS_DIR API_DIR STATE show HASH...
//
// With "show", it follows no link. It shows the screens HASH..., and it prints one JSON list of their
// markup, in the same order.
//
//   node render.js ASSETS_DIR API_DIR STATE requests
//
// With "requests", it does the crawl of the default mode, and it prints one JSON list of each request that
// the page sent: {method, path, headers}. A page that nobody clicks sends no POST.
//
//   node render.js ASSETS_DIR API_DIR STATE click HASH STATUS CLICK...
//
// With "click", it shows the screen HASH. Each CLICK is a JSON object of data attributes ({"view":"split"} is
// a click on an element with data-view="split"). It clicks twice on each element, in order. A question to
// Palette gets its answer before the next click, and the question of the last click waits until the end. It
// prints one JSON object: the screen before the clicks, the screen while the last question is on its way,
// the screen after the answer, the hash at the end, each POST, and each element that the page scrolled to
// (jumps, by id). A click "Check now" waits for the gate too, and its answer says that the cycle starts.
// STATUS is 0 for the answer that the set gives, an HTTP status for a failed POST, or "network".
//
//   node render.js ASSETS_DIR API_DIR STATE asksdown HASH [first]
//
// With "asksdown", the route /v1/asks fails. It reads the routes two times, and it shows the screen HASH
// after the second read. With "first", the route fails from the first read on. The list of answers must
// stay as it was, and the page must not go offline.
//
//   node render.js ASSETS_DIR API_DIR STATE with HASH CODE
//
// With "with", it runs CODE in the page, for example S.side["id"] = "newer", and it shows the screen HASH. It prints the markup as one
// JSON string.
//
//   node render.js ASSETS_DIR API_DIR STATE offline HASH
//
// With "offline", every request fails when the page starts. It prints {before, after}: the screen HASH while the observer cannot be
// reached, and the screen after the requests work again and a click on "Try now" asked again.
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const [assetsDir, apiDir, state, mode, ...args] = process.argv.slice(2);
const EVIL = "<img src=x onerror=alert(1)>\"'&";

// read gives a file of the set, or of its base. With a fallback, a file that neither holds gives the fallback.
function read(name, fallback) {
  const own = path.join(apiDir, state, name);
  if (fs.existsSync(own)) return JSON.parse(fs.readFileSync(own, "utf8"));
  const basePath = path.join(apiDir, state, "base");
  const base = fs.existsSync(basePath) ? fs.readFileSync(basePath, "utf8").trim() : "";
  const inBase = base ? path.join(apiDir, base, name) : "";
  if (fallback !== undefined && !(inBase && fs.existsSync(inBase))) return fallback;
  return JSON.parse(fs.readFileSync(inBase, "utf8"));
}

const data = {
  cluster: read("cluster.json"), findings: read("findings.json"), profiles: read("profiles.json"), newer: read("newer.json"),
  notchecked: read("notchecked.json"), events: read("events.json"), diffs: read("diffs.json"), images: read("images.json"),
  asks: read("asks.json"), replies: read("replies.json", {asks: []}),
  // A set with no alerts.json is a page of an observer that started without --alerts.
  alerts: read("alerts.json", {set: false, channels: [], digestEvery: "", waiting: 0, rules: [], sample: ""}),
};

// poison puts the injection after each name that the page shows, in each file. A name that appears in
// more than one file gets it in each, so the data stays consistent and the links still lead somewhere.
function poison(d) {
  const tokens = new Set([d.cluster.cluster.name]);
  d.profiles.profiles.forEach(p => { tokens.add(p.name); p.packs.forEach(k => { tokens.add(k.name); tokens.add(k.registry); }); });
  d.findings.findings.forEach(f => f.who.forEach(w => tokens.add(w.actor)));
  // An image reference is a token too, so that the finding of an image and its page keep the same reference.
  d.images.images.forEach(i => tokens.add(i.ref));
  const words = [...tokens].filter(Boolean).sort((a, b) => b.length - a.length).map(t => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const re = new RegExp(words.join("|"), "g");
  const walk = v => {
    if (typeof v === "string") return v.replace(re, m => m + EVIL);
    if (Array.isArray(v)) return v.map(walk);
    if (v && typeof v === "object") return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, walk(x)]));
    return v;
  };
  const out = walk(d);
  const tag = s => typeof s === "string" && s ? s + EVIL : s;
  const lead = s => typeof s === "string" && s ? EVIL + s : s;
  out.cluster.strataVersion = tag(out.cluster.strataVersion);
  out.cluster.records.revision = tag(out.cluster.records.revision);
  out.cluster.records.firstSeenFrom = tag(out.cluster.records.firstSeenFrom);
  out.cluster.content.every = tag(out.cluster.content.every);
  out.cluster.versions.every = tag(out.cluster.versions.every);
  out.findings.findings.forEach(f => { f.id = tag(f.id); });
  // The version of a profile gets the injection too, so its subject name@version changes. The rows of newer
  // versions, the diffs, and the answers of Palette name a profile by its subject, and they follow the change.
  const subjects = new Map(out.profiles.profiles.map(p => [`${p.name}@${p.version}`, `${p.name}@${tag(p.version)}`]));
  const follow = s => subjects.get(s) || s;
  out.profiles.profiles.forEach(p => {
    p.type = tag(p.type);
    p.scope = tag(p.scope);
    if (p.published) { p.published.rootRecorded = lead(p.published.rootRecorded); p.published.rootLive = lead(p.published.rootLive); }
    if (p.clusterCopy) p.clusterCopy.rootB = lead(p.clusterCopy.rootB);
    p.packs.forEach(k => { k.kind = tag(k.kind); k.upstream = lead(k.upstream); k.version = tag(k.version); k.newer = tag(k.newer); });
    p.version = tag(p.version);
  });
  out.newer.newer.forEach(r => { r.version = tag(r.version); r.newest = tag(r.newest); r.drops = r.drops.map(tag); r.profiles = r.profiles.map(follow); });
  [out.asks.asks, out.replies.asks].flat().forEach(a => {
    a.profile = follow(a.profile);
    a.newest = tag(a.newest);
    a.refusals.forEach(x => { x.pack = tag(x.pack); x.code = tag(x.code); x.message = tag(x.message); x.clusters = x.clusters.map(tag); });
  });
  out.events.events.forEach(e => { e.reason = tag(e.reason); });
  out.alerts.channels = out.alerts.channels.map(tag);
  out.alerts.digestEvery = tag(out.alerts.digestEvery);
  out.alerts.rules.forEach(r => { r.to = tag(r.to); r.muteUntil = tag(r.muteUntil); });
  out.images.images.forEach(i => i.digests.forEach(d => { d.digest = tag(d.digest); }));
  // Each line of a text gets the injection at its end. The text keeps its form, so the page can read it.
  const lines = s => typeof s === "string" ? s.replace(/\n/g, EVIL + "\n") : s;
  out.alerts.sample = lines(out.alerts.sample);
  out.diffs.forEach(df => {
    df.profile = follow(df.profile);
    df.values.unified = lines(df.values.unified);
    df.manifests.forEach(m => { m.unified = lines(m.unified); });
    const leaves = [df.values.settings, df.values.followsTheVersion, df.values.repeatsTheLists].flat();
    leaves.forEach(l => { l.path = tag(l.path); l.old = tag(l.old); l.new = tag(l.new); });
    [df.imageChanges, df.chartChanges].flat().forEach(c => { c.name = tag(c.name); c.old = tag(c.old); c.new = tag(c.new); });
    [df.manifests, df.paramChanges].flat().forEach(m => { m.name = tag(m.name); });
    df.masked = df.masked.map(tag);
  });
  return out;
}

const served = mode === "poison" ? poison(data) : data;


// requests lists each request that the page sent, in order. jumps lists the id of each element that the page scrolled to.
const requests = [];
const jumps = [];
// failDiffs counts the next requests for a diff that fail with 503. failAsks makes the route /v1/asks fail. postStatus is the answer of a POST that
// fails: an HTTP status, "network" for a request that throws, or 0 for the answer that the set gives. A POST
// waits for the gate, so that a test can look at the page while a question is on its way.
let failDiffs = 0;
let failAsks = false;
// fetchFails makes each request throw, as a page does when its port-forward has ended. copies holds each text that a click on Copy sent to the clipboard.
let fetchFails = false;
const copies = [];
let postStatus = 0;
let gate = Promise.resolve();
const ROUTES = {"/v1/cluster": "cluster", "/v1/findings": "findings", "/v1/profiles": "profiles", "/v1/newer": "newer", "/v1/notchecked": "notchecked", "/v1/events": "events", "/v1/images": "images", "/v1/asks": "asks", "/v1/alerts": "alerts"};
const fail = status => ({ok: false, status, json: async () => ({error: "failed"})});

// replyTo gives the answer of the dev server to a question: the answer that the set holds, else its reply.
function replyTo(profile, key) {
  const same = a => a.profile === profile && a.key === key;
  const held = served.asks.asks.find(same);
  if (held && held.verdict !== "no-answer") return held;
  return served.replies.asks.find(same) || held || null;
}

async function fakeFetch(url, opts = {}) {
  if (fetchFails) throw new TypeError("the network failed");
  const u = new URL(url, "http://127.0.0.1");
  const method = opts.method || "GET";
  requests.push({method, path: u.pathname + u.search, headers: Object.keys(opts.headers || {})});
  let body = null;
  if (failAsks && u.pathname === "/v1/asks") return fail(500);
  if (ROUTES[u.pathname]) {
    body = served[ROUTES[u.pathname]];
  } else {
    const m = /^\/v1\/packs\/([^/]+)\/([^/]+)\/([^/]+)\/diff$/.exec(u.pathname);
    if (m && failDiffs > 0) {
      failDiffs--;
      return fail(503);
    }
    if (m) {
      const [profile, layer, name] = m.slice(1).map(decodeURIComponent);
      body = served.diffs.find(d => d.profile === profile && d.key === `${layer}/${name}` && d.side === u.searchParams.get("side")) || null;
    }
    const ask = /^\/v1\/packs\/([^/]+)\/([^/]+)\/([^/]+)\/ask$/.exec(u.pathname);
    if (ask && method === "POST") {
      await gate;
      if (postStatus === "network") throw new TypeError("the network failed");
      if (postStatus) return fail(Number(postStatus));
      const [profile, layer, name] = ask.slice(1).map(decodeURIComponent);
      body = replyTo(profile, `${layer}/${name}`);
    }
    // the click "Check now": the observer starts the cycle, as the dev server says
    const cycle = /^\/v1\/cycles\/([^/]+)$/.exec(u.pathname);
    if (cycle && method === "POST") {
      await gate;
      if (postStatus === "network") throw new TypeError("the network failed");
      if (postStatus) return fail(Number(postStatus));
      return {ok: true, status: 202, json: async () => ({cycle: cycle[1], state: "started"})};
    }
  }
  return {ok: body !== null, status: body === null ? 404 : 200, json: async () => JSON.parse(JSON.stringify(body))};
}

// listeners holds the handlers that app.js adds to the document, by event type.
const listeners = {};
const elements = {};
const element = id => elements[id] || (elements[id] = {id, innerHTML: "", dataset: {}, addEventListener() {}, focus() {}, setSelectionRange() {}, scrollIntoView() { jumps.push(id); }});
const sandbox = vm.createContext({
  document: {getElementById: element, addEventListener(type, f) { (listeners[type] = listeners[type] || []).push(f); }, activeElement: null},
  location: {hash: "", port: "18080"}, addEventListener() {}, scrollTo() {}, setInterval() {}, setTimeout() {}, fetch: fakeFetch, console,
  navigator: {clipboard: {writeText: async text => { copies.push(text); }}},
});

const unescape = s => s.replace(/&quot;/g, "\"").replace(/&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
const run = code => vm.runInContext(code, sandbox);
const settle = async () => { for (let i = 0; i < 5; i++) await new Promise(r => setImmediate(r)); };
const snapshot = () => ["navl", "ctx", "top", "app"].map(id => String(element(id).innerHTML)).join("\n");

async function show(hash) {
  sandbox.location.hash = hash;
  run("render()");
  await settle();
  return snapshot();
}

// clickOn sends a click to the handlers of the page. The target is a button with the data attributes of
// dataset, and its closest() answers as the browser does for an attribute selector.
function clickOn(dataset) {
  const target = {
    dataset,
    closest(selector) {
      const attribute = /^\[data-([a-z-]+)\]$/.exec(selector);
      if (attribute) return attribute[1].replace(/-([a-z])/g, (_, c) => c.toUpperCase()) in dataset ? target : null;
      return selector.split(",").includes("button") ? target : null;
    },
  };
  (listeners.click || []).forEach(f => f({target}));
}

// crawl shows the first screen, and it follows each link and each control of each screen.
async function crawl() {
  await run("refresh()");
  const screens = [];
  const seen = new Set();
  const shown = new Map();
  const queue = ["#overview"];
  const add = async (hash, variant, set) => {
    // A control can appear twice on a screen, as a tab and as a flag that opens the tab. The crawl shows it once.
    const once = `${hash}\n${variant}`;
    if (shown.has(once)) return shown.get(once);
    try {
      if (set) run(set);
      const html = await show(hash);
      screens.push({hash, variant, html});
      shown.set(once, html);
      return html;
    } catch (err) {
      screens.push({hash, variant, error: String(err && err.stack || err)});
      shown.set(once, "");
      return "";
    } finally {
      run("S.tabs = {}; S.filter = \"all\"; S.q = \"\"; S.open = {}; S.side = {}; S.view = \"unified\"; S.asking = {}; S.askFailed = {};");
    }
  };
  // tabsOf shows each tab of a screen, and each tab in the layout split. prefix names the variant, and base is the code that sets the
  // side of the screen before the tab.
  const tabsOf = async (hash, prefix, base, html) => {
    for (const m of html.matchAll(/data-tab="([^"]*)" data-k="([^"]*)"/g)) {
      const set = `${base ? base + "; " : ""}S.tabs[${JSON.stringify(unescape(m[1]))}] = ${JSON.stringify(unescape(m[2]))}`;
      const tab = await add(hash, `${prefix}tab ${m[1]} ${m[2]}`, set);
      if (tab.includes("data-view=\"split\"")) await add(hash, `${prefix}split tab ${m[1]} ${m[2]}`, `${set}; S.view = "split"`);
    }
  };
  while (queue.length) {
    const hash = queue.shift();
    if (seen.has(hash)) continue;
    seen.add(hash);
    const html = await add(hash, "", "");
    for (const m of html.matchAll(/(?:href|data-go)="(#[^"]*)"/g)) queue.push(unescape(m[1]));
    await tabsOf(hash, "", "", html);
    if (html.includes("data-view=\"split\"")) await add(hash, "split", "S.view = \"split\"");
    const asks = [...html.matchAll(/data-ask="([^"]*)" data-pack="([^"]*)"/g)].map(m => `${unescape(m[1])}|${unescape(m[2])}`);
    if (asks.length) {
      await add(hash, "asking", `${JSON.stringify(asks)}.forEach(k => { S.asking[k] = true; })`);
      await add(hash, "ask failed", `${JSON.stringify(asks)}.forEach(k => { S.askFailed[k] = true; })`);
    }
    for (const m of html.matchAll(/data-filter="([^"]*)"/g)) await add(hash, `filter ${m[1]}`, `S.filter = ${JSON.stringify(unescape(m[1]))}`);
    await add(hash, "search", "S.q = \"e\"");
    await add(hash, "search none", "S.q = \"no profile has this name\"");
    const keys = [...html.matchAll(/data-open="([^"]*)"/g)].map(m => unescape(m[1]));
    if (keys.length) await add(hash, "open", `${JSON.stringify(keys)}.forEach(k => { S.open[k] = true; })`);
    // A pack has one dropdown with up to three sides. Each side shows with its own tabs.
    const sides = [...html.matchAll(/<option value="([a-z]+)"/g)].map(m => m[1]);
    for (const m of html.matchAll(/data-id="([^"]*)"/g)) {
      for (const side of sides) {
        const set = `S.side[${JSON.stringify(unescape(m[1]))}] = ${JSON.stringify(side)}`;
        const sideHtml = await add(hash, `side ${side}`, set);
        await tabsOf(hash, `side ${side} `, set, sideHtml);
        if (sideHtml.includes("data-view=\"split\"")) await add(hash, `side ${side} split`, `${set}; S.view = "split"`);
      }
    }
  }
  return screens;
}

async function main() {
  // The page reads the routes when it starts, so a route that fails from the first read on fails before the script runs.
  failAsks = mode === "asksdown" && args[1] === "first";
  fetchFails = mode === "offline";
  run(fs.readFileSync(path.join(assetsDir, "app.js"), "utf8"));
  if (mode === "ago") {
    // The clock stops, so that each age is the number of seconds that the caller names.
    run("(() => { const now = Date.now(); Date.now = () => now; })()");
    process.stdout.write(JSON.stringify(args.map(s => run(`ago(new Date(Date.now() - ${Number(s)} * 1000).toISOString())`))));
    return;
  }
  if (mode === "retry") {
    failDiffs = 1;
    await run("refresh()");
    const first = await show(args[0]);
    const second = await show(args[0]);
    process.stdout.write(JSON.stringify([first, second]));
    return;
  }
  if (mode === "show") {
    await run("refresh()");
    const pages = [];
    for (const hash of args) pages.push(await show(hash));
    process.stdout.write(JSON.stringify(pages));
    return;
  }
  if (mode === "click") {
    const [hash, status, ...clicks] = args;
    postStatus = status === "network" ? status : Number(status);
    let open;
    gate = new Promise(r => { open = r; });
    await run("refresh()");
    const before = await show(hash);
    for (const [i, click] of clicks.entries()) {
      clickOn(JSON.parse(click));
      clickOn(JSON.parse(click));
      await settle();
      // Each question but the last one gets its answer before the next click.
      if (i < clicks.length - 1) {
        open();
        await settle();
        gate = new Promise(r => { open = r; });
      }
    }
    const during = snapshot();
    open();
    await settle();
    process.stdout.write(JSON.stringify({before, during, after: snapshot(), hash: sandbox.location.hash, posts: requests.filter(r => r.method === "POST"), jumps, copies}));
    return;
  }
  if (mode === "with") {
    await run("refresh()");
    run(args[1]);
    process.stdout.write(JSON.stringify(await show(args[0])));
    return;
  }
  if (mode === "offline") {
    // The page starts while the observer cannot be reached. A click on Try now, after the observer is back, must show the data.
    fetchFails = true;
    await run("refresh()");
    const before = await show(args[0]);
    fetchFails = false;
    clickOn({retry: "page"});
    await settle();
    process.stdout.write(JSON.stringify({before, after: await show(args[0])}));
    return;
  }
  if (mode === "asksdown") {
    await run("refresh()");
    failAsks = true;
    await run("refresh()");
    process.stdout.write(JSON.stringify(await show(args[0])));
    return;
  }
  const screens = await crawl();
  process.stdout.write(JSON.stringify(mode === "requests" ? requests : screens));
}

main().catch(err => { console.error(err); process.exit(1); });
