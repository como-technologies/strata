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
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const [assetsDir, apiDir, state, mode, ...ages] = process.argv.slice(2);
const EVIL = "<img src=x onerror=alert(1)>\"'&";

function read(name) {
  const own = path.join(apiDir, state, name);
  if (fs.existsSync(own)) return JSON.parse(fs.readFileSync(own, "utf8"));
  const base = fs.readFileSync(path.join(apiDir, state, "base"), "utf8").trim();
  return JSON.parse(fs.readFileSync(path.join(apiDir, base, name), "utf8"));
}

const data = {
  cluster: read("cluster.json"), findings: read("findings.json"), profiles: read("profiles.json"), newer: read("newer.json"),
  notchecked: read("notchecked.json"), events: read("events.json"), diffs: read("diffs.json"),
};

// poison puts the injection after each name that the page shows, in each file. A name that appears in
// more than one file gets it in each, so the data stays consistent and the links still lead somewhere.
function poison(d) {
  const tokens = new Set([d.cluster.cluster.name]);
  d.profiles.profiles.forEach(p => { tokens.add(p.name); p.packs.forEach(k => { tokens.add(k.name); tokens.add(k.registry); }); });
  d.findings.findings.forEach(f => f.who.forEach(w => tokens.add(w.actor)));
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
  out.profiles.profiles.forEach(p => {
    p.type = tag(p.type);
    p.scope = tag(p.scope);
    if (p.published) { p.published.rootRecorded = lead(p.published.rootRecorded); p.published.rootLive = lead(p.published.rootLive); }
    if (p.clusterCopy) p.clusterCopy.rootB = lead(p.clusterCopy.rootB);
    p.packs.forEach(k => { k.kind = tag(k.kind); k.upstream = lead(k.upstream); k.version = tag(k.version); k.newer = tag(k.newer); });
    p.version = tag(p.version);
  });
  out.newer.newer.forEach(r => { r.version = tag(r.version); r.newest = tag(r.newest); r.drops = r.drops.map(tag); });
  out.events.events.forEach(e => { e.reason = tag(e.reason); });
  out.diffs.forEach(df => {
    const leaves = [df.values.settings, df.values.followsTheVersion, df.values.repeatsTheLists].flat();
    leaves.forEach(l => { l.path = tag(l.path); l.old = tag(l.old); l.new = tag(l.new); });
    [df.imageChanges, df.chartChanges].flat().forEach(c => { c.name = tag(c.name); c.old = tag(c.old); c.new = tag(c.new); });
    [df.manifests, df.paramChanges].flat().forEach(m => { m.name = tag(m.name); });
    df.masked = df.masked.map(tag);
  });
  return out;
}

const served = mode === "poison" ? poison(data) : data;

const ROUTES = {"/v1/cluster": "cluster", "/v1/findings": "findings", "/v1/profiles": "profiles", "/v1/newer": "newer", "/v1/notchecked": "notchecked", "/v1/events": "events"};
async function fakeFetch(url) {
  const u = new URL(url, "http://127.0.0.1");
  let body = null;
  if (ROUTES[u.pathname]) {
    body = served[ROUTES[u.pathname]];
  } else {
    const m = /^\/v1\/packs\/([^/]+)\/([^/]+)\/([^/]+)\/diff$/.exec(u.pathname);
    if (m) {
      const [profile, layer, name] = m.slice(1).map(decodeURIComponent);
      body = served.diffs.find(d => d.profile === profile && d.key === `${layer}/${name}` && d.side === u.searchParams.get("side")) || null;
    }
  }
  return {ok: body !== null, status: body === null ? 404 : 200, json: async () => JSON.parse(JSON.stringify(body))};
}

const elements = {};
const element = id => elements[id] || (elements[id] = {id, innerHTML: "", dataset: {}, addEventListener() {}, focus() {}, setSelectionRange() {}});
const sandbox = vm.createContext({
  document: {getElementById: element, addEventListener() {}, activeElement: null},
  location: {hash: ""}, addEventListener() {}, scrollTo() {}, setInterval() {}, fetch: fakeFetch, console,
});

const unescape = s => s.replace(/&quot;/g, "\"").replace(/&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
const run = code => vm.runInContext(code, sandbox);
const settle = async () => { for (let i = 0; i < 5; i++) await new Promise(r => setImmediate(r)); };

async function show(hash) {
  sandbox.location.hash = hash;
  run("render()");
  await settle();
  return ["navl", "ctx", "top", "app"].map(id => String(element(id).innerHTML)).join("\n");
}

async function main() {
  run(fs.readFileSync(path.join(assetsDir, "app.js"), "utf8"));
  if (mode === "ago") {
    // The clock stops, so that each age is the number of seconds that the caller names.
    run("(() => { const now = Date.now(); Date.now = () => now; })()");
    process.stdout.write(JSON.stringify(ages.map(s => run(`ago(new Date(Date.now() - ${Number(s)} * 1000).toISOString())`))));
    return;
  }
  await run("refresh()");
  const screens = [];
  const seen = new Set();
  const queue = ["#overview"];
  const add = async (hash, variant, set) => {
    try {
      if (set) run(set);
      const html = await show(hash);
      screens.push({hash, variant, html});
      return html;
    } catch (err) {
      screens.push({hash, variant, error: String(err && err.stack || err)});
      return "";
    } finally {
      run("S.tabs = {}; S.filter = \"all\"; S.q = \"\"; S.open = {}; S.side = {};");
    }
  };
  while (queue.length) {
    const hash = queue.shift();
    if (seen.has(hash)) continue;
    seen.add(hash);
    const html = await add(hash, "", "");
    for (const m of html.matchAll(/(?:href|data-go)="(#[^"]*)"/g)) queue.push(unescape(m[1]));
    for (const m of html.matchAll(/data-tab="([^"]*)" data-k="([^"]*)"/g)) {
      await add(hash, `tab ${m[1]}`, `S.tabs[${JSON.stringify(unescape(m[1]))}] = ${JSON.stringify(unescape(m[2]))}`);
    }
    for (const m of html.matchAll(/data-filter="([^"]*)"/g)) await add(hash, `filter ${m[1]}`, `S.filter = ${JSON.stringify(unescape(m[1]))}`);
    await add(hash, "search", "S.q = \"e\"");
    await add(hash, "search none", "S.q = \"no profile has this name\"");
    const keys = [...html.matchAll(/data-open="([^"]*)"/g)].map(m => unescape(m[1]));
    if (keys.length) await add(hash, "open", `${JSON.stringify(keys)}.forEach(k => { S.open[k] = true; })`);
    for (const m of html.matchAll(/data-id="([^"]*)"/g)) {
      await add(hash, "side", `S.side[${JSON.stringify(unescape(m[1]))}] = "cluster"`);
    }
  }
  process.stdout.write(JSON.stringify(screens));
}

main().catch(err => { console.error(err); process.exit(1); });
