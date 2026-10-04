// Test driver for src/assets/opencode-plugin.js (plugin_replays_recorded_events).
// Runs under node or bun:
//   <runtime> opencode-plugin-driver.mjs <plugin.js> <hook-exe> <hang-exe> <log-dir> <events.jsonl>...
// It loads the plugin the way OpenCode does (an async factory returning hooks),
// replays each recorded event log through the `event` hook, one plugin instance
// per launch, and prints a JSON summary on stdout: the events fed to each
// instance, the composed sample configs, and any error the plugin let escape.
// The fake hook exe (written by the Rust test) logs each delivery to
// <log-dir>/<agent>.jsonl.
import { existsSync, readFileSync, statSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [pluginPath, hookExe, hangExe, logDir, ...eventFiles] = process.argv.slice(2);
const FALLBACK_MS = 100;
// A hook must return without waiting for its child: the hanging exe sleeps
// 10 s and its delivery is cut at 2 s here, so a hook that did wait would take
// 2 s or more. 1 s keeps that meaning while tolerating this process being descheduled
// for a few hundred ms in a fully loaded workspace test run.
const RETURNS_AT_ONCE_MS = 1000;
// Dispose is bounded by a 1 s chain wait plus a 1 s session-end spawn (2 s),
// plus slack for the same scheduling noise; far below the 10 s a hung exe runs.
const DISPOSE_LIMIT_MS = 4500;
// Session-end spawn limit for instances whose deliveries must complete: the
// 1 s product default is for hung exes (the `hang` instance keeps it).
const SESSION_END_LIMIT_MS = 10000;

// Per-delivery kill limit for instances whose deliveries must complete. The
// `hang` and `slow` instances cut deliveries at 2 s instead of the 10 s product
// default, to keep the run short; `patient` keeps the default.
const DELIVERY_LIMIT_MS = 10000;

const errors = [];
process.on("uncaughtException", (e) => errors.push(`uncaught: ${e}`));
process.on("unhandledRejection", (e) => errors.push(`unhandled rejection: ${e}`));

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const plugin = await import(pathToFileURL(pluginPath).href);
const factories = Object.values(plugin).filter((v) => typeof v === "function");
if (factories.length !== 1) errors.push(`expected one plugin export, got ${factories.length}`);
const factory = factories[0];

const ctx = {
  directory: "/work",
  worktree: "/",
  project: {},
  client: {},
  serverUrl: new URL("http://localhost:4096/"),
};

// `deliveryLimitMs` null leaves the product default in force.
async function instance(agent, exe, endLimitMs = SESSION_END_LIMIT_MS, deliveryLimitMs = DELIVERY_LIMIT_MS) {
  if (deliveryLimitMs === null) delete process.env.DUOHIVE_DELIVERY_LIMIT_MS;
  else process.env.DUOHIVE_DELIVERY_LIMIT_MS = String(deliveryLimitMs);
  process.env.DUOHIVE_SESSION_END_LIMIT_MS = String(endLimitMs);
  process.env.DUOHIVE_HOOK_EXE = exe;
  process.env.DUOHIVE_HOOK_AGENT = agent;
  process.env.DUOHIVE_START_FALLBACK_MS = String(FALLBACK_MS);
  return { agent, hooks: await factory(ctx) };
}

// Calls a hook, recording a throw or a call slower than `limitMs`.
async function timed(label, limitMs, fn) {
  const started = Date.now();
  try {
    await fn();
  } catch (e) {
    errors.push(`${label} threw: ${e}`);
  }
  const took = Date.now() - started;
  if (took > limitMs) errors.push(`${label} took ${took} ms (limit ${limitMs})`);
  return took;
}

// Waits until an agent's delivery log has not grown for 250 ms (at most 5 s),
// so dispose's own 1 s wait is not what keeps the order in a slow test run.
// `minLines` first waits for that many deliveries to be logged (a readiness
// signal, so a delivery slow to start is not mistaken for a quiet log).
async function quiet(agent, minLines = 0) {
  const file = `${logDir}/${agent}.jsonl`;
  const lines = () => (existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean).length : 0);
  const ready = Date.now() + 10000;
  while (lines() < minLines && Date.now() < ready) await sleep(25);
  const end = Date.now() + 5000;
  let size = -1;
  let stableSince = Date.now();
  while (Date.now() < end) {
    const now = existsSync(file) ? statSync(file).size : 0;
    if (now !== size) {
      size = now;
      stableSince = Date.now();
    } else if (Date.now() - stableSince >= 250) {
      return;
    }
    await sleep(25);
  }
}

// Waits (at most 10 s) for the fake exe to record its pid: proof that the
// delivery is really running, however long a loaded machine took to start it.
async function running(agent) {
  const file = `${logDir}/${agent}.pid`;
  const end = Date.now() + 10000;
  while (!existsSync(file) && Date.now() < end) await sleep(25);
}

const feed = [];

async function finish(inst) {
  await sleep(FALLBACK_MS * 3);
  // Every instance has started by now (an event or the fallback timer), so
  // session-start is on its way: wait until it is logged, not for a quiet
  // moment that a slow spawn could fake.
  await quiet(inst.agent, 1);
  feed.push({ agent: inst.agent, type: "dispose" });
  await timed(`${inst.agent} dispose`, DISPOSE_LIMIT_MS, () => inst.hooks.dispose());
}

// Replay. A `{"plugin":"init"}` line starts an instance; a `{"plugin":"dispose"}`
// line ends the newest one, and later events go to the one still running (the
// `opencode mcp list` run started and ended in the middle of a2's session).
// Other marker lines (`process.exit`, `config`) are skipped.
for (const [f, path] of eventFiles.entries()) {
  const open = [];
  let launches = 0;
  const lines = readFileSync(path, "utf8").split("\n").filter((l) => l.trim() !== "");
  for (const line of lines) {
    const event = JSON.parse(line);
    if (event.plugin === "init") {
      launches += 1;
      open.push(await instance(`f${f + 1}-l${launches}`, hookExe));
      continue;
    }
    if (event.plugin === "dispose") {
      const inst = open.pop();
      if (inst) await finish(inst);
      continue;
    }
    if (event.plugin !== undefined) continue;
    const inst = open[open.length - 1];
    if (!inst) continue;
    const item = { agent: inst.agent, type: event.type };
    if (event.type === "session.status") item.status = event.properties?.status?.type ?? null;
    if (event.type === "permission.asked") item.properties = event.properties;
    feed.push(item);
    await timed(`${inst.agent} ${event.type}`, RETURNS_AT_ONCE_MS, () => inst.hooks.event({ event }));
  }
  while (open.length > 0) await finish(open.pop());
}

// The `config` hook composes edit and bash (S2.11). What counts is the
// decision OpenCode reaches: its rules are the default ("*" allow), the
// agent's built-in rules (as `opencode agent list` showed them for 1.18.34),
// the top-level `permission`, then the agent's, each in key order, and the
// last rule whose permission and pattern match wins (`rules.findLast`).
const samples = [
  {},
  { permission: "allow" },
  { permission: "deny" },
  { permission: { "*": "ask", edit: "allow" } },
  {
    permission: {
      edit: "deny",
      bash: { "*": "allow", "rm *": "deny", "git push*": "ask" },
      webfetch: "allow",
    },
    agent: {
      build: { permission: { edit: "allow", bash: { "git commit*": "deny" } } },
      plan: { permission: "allow" },
      review: { model: "x" },
      docs: { permission: { edit: { "*": "allow", "docs/**": "allow" } } },
    },
  },
  // "*" after edit and bash: under last-match-wins it would decide for them.
  { permission: { edit: "allow", "*": "allow" } },
  { permission: { bash: { "git *": "allow" }, "*": "allow" } },
  { agent: { build: { permission: { bash: "allow", "*": "allow" } } } },
  // A pattern object without "*": its own patterns still decide.
  { permission: { edit: { "src/**": "deny" } } },
  // A stricter "*" after edit or bash still decides for them.
  { permission: { edit: "ask", "*": "deny" } },
  { permission: { bash: "allow", "*": "deny" } },
  { agent: { plan: { permission: { edit: "allow", "*": "deny" } } } },
  { permission: { edit: { "*": "ask" }, "*": "deny" } },
  // The user's own top-level allow overrides plan's built-in deny: it asks.
  { permission: { edit: "allow" } },
  // All-digit pattern keys come first in a JS object, whatever their place.
  { permission: { edit: { 5: "deny" } } },
  { permission: { edit: { "?": "allow" } }, agent: { build: { permission: { edit: { 5: "deny" } } } } },
];
const queries = [
  ["build", "edit", "a.txt"],
  ["build", "edit", "src/x.ts"],
  ["build", "bash", "ls"],
  ["build", "bash", "rm -rf x"],
  ["build", "bash", "git push"],
  ["build", "bash", "git commit -m x"],
  ["build", "webfetch", "x"],
  ["plan", "edit", "a.txt"],
  ["plan", "edit", ".opencode/plans/p.md"],
  ["plan", "bash", "rm -rf x"],
  ["docs", "edit", "a.txt"],
  ["docs", "edit", "docs/a.md"],
  ["explore", "edit", "a.txt"],
  ["explore", "bash", "ls"],
  ["general", "edit", "a.txt"],
  ["title", "bash", "ls"],
  ["build", "edit", "5"],
  ["plan", "edit", "5"],
];

const DENY_ALL = [{ permission: "*", pattern: "*", action: "deny" }];
const BUILTIN = {
  plan: [
    { permission: "edit", pattern: "*", action: "deny" },
    { permission: "edit", pattern: ".opencode/plans/*.md", action: "allow" },
  ],
  explore: [...DENY_ALL, { permission: "bash", pattern: "*", action: "allow" }],
  compaction: DENY_ALL,
  summary: DENY_ALL,
  title: DENY_ALL,
};

function wildcard(pattern, text) {
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped.replace(/\*/g, ".*").replace(/\?/g, ".")}$`, "s").test(text);
}

function rules(block) {
  if (block === undefined) return [];
  if (typeof block === "string") return [{ permission: "*", pattern: "*", action: block }];
  return Object.entries(block).flatMap(([permission, value]) =>
    typeof value === "string"
      ? [{ permission, pattern: "*", action: value }]
      : Object.entries(value).map(([pattern, action]) => ({ permission, pattern, action })),
  );
}

function decide(cfg, agent, permission, pattern) {
  const all = [
    { permission: "*", pattern: "*", action: "allow" },
    ...(BUILTIN[agent] ?? []),
    ...rules(cfg.permission),
    ...rules(cfg.agent?.[agent]?.permission),
  ];
  const hit = all.findLast((r) => wildcard(r.permission, permission) && wildcard(r.pattern, pattern));
  return hit?.action;
}

const composer = await instance("config", "");
const decisions = [];
const before = [];
const decideAll = (cfg) => queries.map(([agent, permission, pattern]) => decide(cfg, agent, permission, pattern));
for (const sample of samples) {
  const cfg = JSON.parse(JSON.stringify(sample));
  before.push(decideAll(cfg));
  await timed("config", RETURNS_AT_ONCE_MS, () => composer.hooks.config(cfg));
  decisions.push(decideAll(cfg));
}
await timed("config dispose", DISPOSE_LIMIT_MS, () => composer.hooks.dispose());

// A missing hook exe: nothing throws, nothing is rejected.
const missing = await instance("missing", `${logDir}/no-such-duohive`);
const h = missing.hooks;
const sample = (type, properties) => ({ event: { type, properties } });
const calls = [
  () => h.event(sample("catalog.updated", {})),
  () => h["chat.message"]({ sessionID: "s" }, { parts: [] }),
  () => h.event(sample("session.status", { sessionID: "s", status: { type: "busy" } })),
  () => h["tool.execute.before"]({ tool: "write" }, { args: {} }),
  () => h.event(sample("permission.asked", { permission: "edit", patterns: ["a"] })),
  () => h.event(sample("permission.replied", { reply: "once" })),
  () => h["tool.execute.after"]({ tool: "write" }, {}),
  () => h.event(sample("session.idle", { sessionID: "s" })),
  () => h.event({ event: null }),
  () => h.event(undefined),
  () => h.config(undefined),
];
for (const [i, call] of calls.entries()) await timed(`missing exe call ${i}`, RETURNS_AT_ONCE_MS, call);
await sleep(300);
await timed("missing exe dispose", DISPOSE_LIMIT_MS, () => h.dispose());

// A hook exe that hangs: hooks still return at once, and dispose is bounded
// (1 s for the chain, then a session-end spawn bounded at 1 s).
const hang = await instance("hang", hangExe, 1000, 2000);
await timed("hang event", RETURNS_AT_ONCE_MS, () =>
  hang.hooks.event(sample("catalog.updated", {})),
);
await timed("hang busy", RETURNS_AT_ONCE_MS, () =>
  hang.hooks.event(sample("session.status", { sessionID: "s", status: { type: "busy" } })),
);
const hangDispose = await timed("hang dispose", DISPOSE_LIMIT_MS, () => hang.hooks.dispose());

// Dispose kills a delivery still running, so session-end is the last one
// (its session-start would be logged 1.5 s in, after session-end, if it lived).
const inflight = await instance("inflight", hookExe);
await timed("inflight event", RETURNS_AT_ONCE_MS, () =>
  inflight.hooks.event(sample("catalog.updated", {})),
);
await running("inflight");
await timed("inflight dispose", DISPOSE_LIMIT_MS, () => inflight.hooks.dispose());

// A delivery that hangs is killed at its (shortened) 2 s limit and the chain goes on.
const slow = await instance("slow", hookExe, SESSION_END_LIMIT_MS, 2000);
await timed("slow event", RETURNS_AT_ONCE_MS, () => slow.hooks.event(sample("catalog.updated", {})));
await timed("slow busy", RETURNS_AT_ONCE_MS, () =>
  slow.hooks.event(sample("session.status", { sessionID: "s", status: { type: "busy" } })),
);
await running("slow");
await sleep(2600);
await quiet("slow", 1);
await timed("slow dispose", DISPOSE_LIMIT_MS, () => slow.hooks.dispose());

// A delivery slower than 2 s (a loaded machine) still lands under the
// product default limit.
const patient = await instance("patient", hookExe, SESSION_END_LIMIT_MS, null);
await timed("patient event", RETURNS_AT_ONCE_MS, () =>
  patient.hooks.event(sample("catalog.updated", {})),
);
await quiet("patient", 1);
await timed("patient dispose", DISPOSE_LIMIT_MS, () => patient.hooks.dispose());

// Subagent sessions (with a parentID) neither busy nor end the agent's turn.
const sub = await instance("subagent", hookExe);
const subEvents = [
  sample("catalog.updated", {}),
  sample("session.created", { info: { id: "c", parentID: "p" } }),
  sample("session.status", { sessionID: "c", status: { type: "busy" } }),
  sample("session.idle", { sessionID: "c" }),
  sample("session.status", { sessionID: "p", status: { type: "busy" } }),
  sample("session.updated", { info: { id: "c2", parentID: "p" } }),
  sample("session.idle", { sessionID: "c2" }),
  sample("session.idle", { sessionID: "p" }),
];
for (const e of subEvents) await timed("subagent event", RETURNS_AT_ONCE_MS, () => sub.hooks.event(e));
await quiet("subagent", 3);
await timed("subagent dispose", DISPOSE_LIMIT_MS, () => sub.hooks.dispose());

// Busy payloads carry only what DuoHive reads, never a tool's full output.
const trim = await instance("trim", hookExe);
const big = "x".repeat(100000);
await timed("trim tool", RETURNS_AT_ONCE_MS, () =>
  trim.hooks["tool.execute.after"](
    { tool: "write", sessionID: "s", callID: "c" },
    { title: "t", output: big, metadata: { diff: big } },
  ),
);
await timed("trim asked", RETURNS_AT_ONCE_MS, () =>
  trim.hooks.event(
    sample("permission.asked", {
      id: "per_1",
      sessionID: "s",
      permission: "edit",
      patterns: ["a.txt"],
      metadata: { filepath: "/w/a.txt", diff: big },
      always: ["*"],
    }),
  ),
);
await timed("trim chat", RETURNS_AT_ONCE_MS, () =>
  trim.hooks["chat.message"]({ sessionID: "s", agent: "build" }, { parts: [{ type: "text", text: big }] }),
);
await quiet("trim", 4);
await timed("trim dispose", DISPOSE_LIMIT_MS, () => trim.hooks.dispose());

await sleep(200);
process.stdout.write(JSON.stringify({ feed, decisions, before, queries, errors, hangDispose }));
process.exit(0);
