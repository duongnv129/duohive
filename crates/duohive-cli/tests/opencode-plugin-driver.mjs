// Test driver for src/assets/opencode-plugin/index.js (plugin_replays_recorded_events).
// Runs under node or bun:
//   <runtime> opencode-plugin-driver.mjs <index.mjs> <hook-exe> <hang-exe> <log-dir> <instructions> <probe.jsonl>...
// It loads the plugin the way OpenCode 2.0 does (a default export
// `{ id, setup(ctx) }`, `setup` returning a cleanup function) with a fake
// `ctx`: `event.subscribe()` (an async iterator the driver feeds),
// `permission.hook`, `session.hook` and `options`. It replays each recorded
// probe log (spike S2.14: one OpenCode launch each) through the event stream
// and the `evaluate` hook, one plugin instance per log, then runs the
// scenarios below, and prints a JSON summary on stdout: the `evaluate`
// results, the scenario results, and any error the plugin let escape. The
// fake hook exe (written by the Rust test) logs each delivery to
// <log-dir>/<agent>.jsonl.
import { existsSync, readFileSync, statSync } from "node:fs";
import { pathToFileURL } from "node:url";

const [pluginPath, hookExe, hangExe, logDir, instructionsPath, ...probeFiles] =
  process.argv.slice(2);
const FALLBACK_MS = 100;
const START_DELAY_MS = 50;
// `setup` and every hook must return without waiting for a child: the
// hanging exe sleeps 10 s and its delivery is cut at 2 s here, so a call
// that did wait would take 2 s or more. 1 s keeps that meaning while
// tolerating this process being descheduled in a fully loaded test run.
const RETURNS_AT_ONCE_MS = 1000;
// The cleanup is one session-end spawn bounded at 1 s, plus slack for the same
// scheduling noise; far below the 10 s a hung exe runs.
const CLEANUP_LIMIT_MS = 3000;
// Session-end spawn limit for instances whose deliveries must complete: the
// 1 s product default is for hung exes (the `hang` instance keeps it).
const SESSION_END_LIMIT_MS = 10000;
// Per-delivery kill limit for instances whose deliveries must complete. The
// `hang` and `slow` instances cut deliveries at 2 s instead, to keep the run
// short; `patient` keeps the 10 s product default.
const DELIVERY_LIMIT_MS = 10000;

const errors = [];
process.on("uncaughtException", (e) => errors.push(`uncaught: ${e}`));
process.on("unhandledRejection", (e) => errors.push(`unhandled rejection: ${e}`));

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const tick = () => new Promise((resolve) => setImmediate(resolve));

const plugin = (await import(pathToFileURL(pluginPath).href)).default;
if (plugin?.id !== "duohive" || typeof plugin?.setup !== "function") {
  errors.push(`expected a default export { id: "duohive", setup }, got ${JSON.stringify(plugin)}`);
}

// A fake OpenCode 2.0 plugin context.
function fakeCtx(options) {
  const queue = [];
  let waiting = null;
  let closed = false;
  const hooks = {};
  const register = async (name, fn) => {
    (hooks[name] ??= []).push(fn);
  };
  const iterator = {
    next() {
      if (queue.length > 0) return Promise.resolve({ value: queue.shift(), done: false });
      if (closed) return Promise.resolve({ value: undefined, done: true });
      return new Promise((resolve) => {
        waiting = resolve;
      });
    },
    return() {
      closed = true;
      return Promise.resolve({ value: undefined, done: true });
    },
  };
  return {
    ctx: {
      options,
      event: { subscribe: () => ({ [Symbol.asyncIterator]: () => iterator }) },
      permission: { hook: register },
      session: { hook: register },
    },
    hooks,
    push(ev) {
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: ev, done: false });
      } else {
        queue.push(ev);
      }
    },
    // Resolves once the plugin has taken every queued event and waits for the next.
    async drained() {
      for (let i = 0; i < 2000 && (queue.length > 0 || !waiting); i += 1) await tick();
    },
    close() {
      closed = true;
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: undefined, done: true });
      }
    },
  };
}

// Calls `fn`, recording a throw or a call slower than `limitMs`.
async function timed(label, limitMs, fn) {
  const started = Date.now();
  let value;
  try {
    value = await fn();
  } catch (e) {
    errors.push(`${label} threw: ${e}`);
  }
  const took = Date.now() - started;
  if (took > limitMs) errors.push(`${label} took ${took} ms (limit ${limitMs})`);
  return value;
}

// `deliveryLimitMs` null leaves the product default in force.
async function instance(agent, exe, opts = {}) {
  const {
    options = {},
    endLimitMs = SESSION_END_LIMIT_MS,
    deliveryLimitMs = DELIVERY_LIMIT_MS,
    startDelayMs = START_DELAY_MS,
    fallbackMs = FALLBACK_MS,
  } = opts;
  if (deliveryLimitMs === null) delete process.env.DUOHIVE_DELIVERY_LIMIT_MS;
  else process.env.DUOHIVE_DELIVERY_LIMIT_MS = String(deliveryLimitMs);
  process.env.DUOHIVE_SESSION_END_LIMIT_MS = String(endLimitMs);
  process.env.DUOHIVE_START_DELAY_MS = String(startDelayMs);
  process.env.DUOHIVE_START_FALLBACK_MS = String(fallbackMs);
  process.env.DUOHIVE_HOOK_EXE = exe;
  process.env.DUOHIVE_HOOK_AGENT = agent;
  const fake = fakeCtx(options);
  const cleanup = await timed(`${agent} setup`, RETURNS_AT_ONCE_MS, () => plugin.setup(fake.ctx));
  if (typeof cleanup !== "function") errors.push(`${agent} setup returned no cleanup`);
  return {
    agent,
    fake,
    async event(type, data) {
      fake.push({ id: "evt", type, data });
      await fake.drained();
    },
    evaluate(input) {
      for (const fn of fake.hooks.evaluate ?? []) fn(input);
      return input.effect;
    },
    async cleanup() {
      await timed(`${agent} cleanup`, CLEANUP_LIMIT_MS, () => cleanup?.());
      fake.close();
    },
  };
}

function lines(agent) {
  const file = `${logDir}/${agent}.jsonl`;
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean).length : 0;
}

// Waits until an agent's delivery log has not grown for `stableMs` (at most
// 10 s), first waiting for `minLines` deliveries (a readiness signal, so a
// delivery slow to start is not mistaken for a quiet log). The cleanup drops
// queued deliveries, so a replay waits for its log to settle first.
async function quiet(agent, minLines = 0, stableMs = 750) {
  const file = `${logDir}/${agent}.jsonl`;
  const ready = Date.now() + 10000;
  while (lines(agent) < minLines && Date.now() < ready) await sleep(25);
  const end = Date.now() + 10000;
  let size = -1;
  let stableSince = Date.now();
  while (Date.now() < end) {
    const now = existsSync(file) ? statSync(file).size : 0;
    if (now !== size) {
      size = now;
      stableSince = Date.now();
    } else if (Date.now() - stableSince >= stableMs) {
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

// Replay: each probe log is one launch, agent p<n>. Its `event.iter` lines are
// the events OpenCode sent, its `permission.evaluate` lines the evaluate calls.
const evaluations = [];
for (const [i, path] of probeFiles.entries()) {
  const agent = `p${i + 1}`;
  const inst = await instance(agent, hookExe);
  for (const line of readFileSync(path, "utf8").split("\n")) {
    if (line.trim() === "") continue;
    const record = JSON.parse(line);
    if (record.kind === "event.iter") {
      inst.fake.push(record.data);
      await inst.fake.drained();
    } else if (record.kind === "permission.evaluate") {
      const input = JSON.parse(JSON.stringify(record.data.before));
      await timed(`${agent} evaluate`, RETURNS_AT_ONCE_MS, () => inst.evaluate(input));
      evaluations.push({ agent, action: input.action, before: record.data.before.effect, after: input.effect });
    }
  }
  await quiet(agent, 1);
  await inst.cleanup();
}

const results = {};

// A missing hook exe: nothing throws, nothing is rejected, garbage is ignored.
{
  const m = await instance("missing", `${logDir}/no-such-duohive`, {
    options: { instructions: instructionsPath },
  });
  const calls = [
    () => m.event("plugin.updated", {}),
    () => m.event("session.execution.started", { sessionID: "s" }),
    () => m.event("permission.asked", { id: "p", sessionID: "s", action: "edit", resources: ["a"] }),
    () => m.event("permission.replied", { sessionID: "s", requestID: "p", reply: "once" }),
    () => m.event("form.created", { form: { id: "f", sessionID: "s", fields: [null] } }),
    () => m.event("form.replied", { id: "f", sessionID: "s" }),
    () => m.event("session.text.ended", { sessionID: "s", ordinal: "x", text: null }),
    () => m.event("session.execution.failed", { sessionID: "s", error: "garbage" }),
    () => m.event("session.execution.succeeded", null),
    () => m.event(undefined, undefined),
    async () => {
      m.fake.push(null);
      m.fake.push(42);
      m.fake.push({ type: 7 });
      await m.fake.drained();
    },
    () => m.evaluate({ action: "edit", effect: "allow", resources: 5 }),
    () => {
      for (const fn of m.fake.hooks.evaluate ?? []) {
        fn(null);
        fn(undefined);
      }
    },
    () => {
      for (const fn of m.fake.hooks.context ?? []) {
        fn(null);
        fn({ sessionID: "s" });
        fn({ sessionID: "s", system: "not a list" });
      }
    },
  ];
  for (const [i, call] of calls.entries()) await timed(`missing exe call ${i}`, RETURNS_AT_ONCE_MS, call);
  await sleep(300);
  await m.cleanup();
  await m.cleanup();
}

// A context whose hooks throw, and one with no event stream: setup still returns.
{
  process.env.DUOHIVE_HOOK_EXE = `${logDir}/no-such-duohive`;
  process.env.DUOHIVE_HOOK_AGENT = "broken";
  const throwing = async () => {
    throw new Error("boom");
  };
  const ctxs = [
    {
      options: { instructions: instructionsPath },
      event: {
        subscribe: () => {
          throw new Error("boom");
        },
      },
      permission: { hook: throwing },
      session: { hook: throwing },
    },
    {},
    undefined,
  ];
  for (const [i, ctx] of ctxs.entries()) {
    const cleanup = await timed(`broken ctx ${i} setup`, RETURNS_AT_ONCE_MS, () => plugin.setup(ctx));
    await timed(`broken ctx ${i} cleanup`, CLEANUP_LIMIT_MS, () => cleanup?.());
  }
}

// A hook exe that hangs: hooks still return at once, and the cleanup is
// bounded (a session-end spawn bounded at 1 s).
{
  const hang = await instance("hang", hangExe, { endLimitMs: 1000, deliveryLimitMs: 2000 });
  await timed("hang event", RETURNS_AT_ONCE_MS, () => hang.event("session.execution.started", { sessionID: "s" }));
  const started = Date.now();
  await hang.cleanup();
  results.hangCleanupMs = Date.now() - started;
}

// The cleanup kills a delivery still running, so session-end is the last one
// (its session-start would be logged 1.5 s in, after session-end, if it lived).
{
  const inflight = await instance("inflight", hookExe);
  await inflight.event("session.execution.started", { sessionID: "s" });
  await running("inflight");
  await inflight.cleanup();
}

// A delivery that hangs is killed at its (shortened) 2 s limit and the chain goes on.
{
  const slow = await instance("slow", hookExe, { deliveryLimitMs: 2000 });
  await slow.event("session.execution.started", { sessionID: "s" });
  await running("slow");
  await sleep(2600);
  await quiet("slow", 1);
  await slow.cleanup();
}

// A delivery slower than 2 s (a loaded machine) still lands under the
// product default limit.
{
  const patient = await instance("patient", hookExe, { deliveryLimitMs: null });
  await patient.event("plugin.updated", {});
  await quiet("patient", 1);
  await patient.cleanup();
}

// session-start comes the start delay after `plugin.updated`, not at it.
{
  const delayed = await instance("delayed", hookExe, { startDelayMs: 600, fallbackMs: 60000 });
  await delayed.event("plugin.updated", {});
  await delayed.event("plugin.updated", {});
  await sleep(150);
  results.delayedEarly = lines("delayed");
  await quiet("delayed", 1);
  await delayed.cleanup();
}

// With no event at all, the fallback timer sends session-start.
{
  const fallback = await instance("fallback", hookExe, { fallbackMs: 100 });
  await quiet("fallback", 1);
  await fallback.cleanup();
}

// Child sessions (subagents) neither busy, ask, reply nor end the agent's turn.
{
  const sub = await instance("subagent", hookExe);
  await sub.event("session.execution.started", { sessionID: "p" });
  await sub.event("session.created", { sessionID: "c", parentID: "p" });
  await sub.event("session.execution.started", { sessionID: "c" });
  await sub.event("session.text.ended", { sessionID: "c", assistantMessageID: "mc", ordinal: 0, text: "child" });
  await sub.event("permission.asked", { id: "pc", sessionID: "c", action: "edit", resources: ["x"] });
  await sub.event("permission.replied", { requestID: "pc", sessionID: "c", reply: "once" });
  await sub.event("form.created", { form: { id: "fc", sessionID: "c", title: "Questions" } });
  await sub.event("session.execution.succeeded", { sessionID: "c" });
  await sub.event("session.execution.failed", { sessionID: "c", error: { message: "child failed" } });
  await sub.event("session.text.ended", { sessionID: "p", assistantMessageID: "m1", ordinal: 1, text: "two" });
  await sub.event("session.text.ended", { sessionID: "p", assistantMessageID: "m1", ordinal: 0, text: "one" });
  await sub.event("session.execution.succeeded", { sessionID: "p" });
  await quiet("subagent", 3);
  await sub.cleanup();
}

// The reply is the last assistant message's text parts; a new turn starts with none.
{
  const reply = await instance("reply", hookExe);
  await reply.event("session.execution.started", { sessionID: "s" });
  await reply.event("session.text.ended", { sessionID: "s", assistantMessageID: "m1", ordinal: 0, text: "first message" });
  await reply.event("session.text.ended", { sessionID: "s", assistantMessageID: "m2", ordinal: 0, text: "last" });
  await reply.event("session.text.ended", { sessionID: "s", assistantMessageID: "m2", ordinal: 1, text: "message" });
  await reply.event("session.execution.interrupted", { sessionID: "s", reason: "user" });
  await reply.event("session.execution.started", { sessionID: "s" });
  await reply.event("session.execution.succeeded", { sessionID: "s" });
  await quiet("reply", 5);
  await reply.cleanup();
}

// Payloads carry only what DuoHive reads, never a diff, a tool's output or an
// error's response body, and long text is cut.
{
  const trim = await instance("trim", hookExe);
  const big = "x".repeat(100000);
  await trim.event("permission.asked", {
    id: "per_1",
    sessionID: "s",
    action: "shell",
    resources: Array.from({ length: 20 }, (_, i) => `${i} ${big}`),
    save: ["*"],
    metadata: { diff: big },
    source: { type: "tool" },
  });
  await trim.event("permission.replied", { requestID: "per_1", sessionID: "s", reply: "reject" });
  await trim.event("form.created", {
    form: { id: "f", sessionID: "s", title: "Questions", fields: [{ key: "q0", description: big, options: [big] }] },
  });
  await trim.event("form.replied", { id: "f", sessionID: "s", answer: { q0: big } });
  await trim.event("session.execution.failed", {
    sessionID: "s",
    error: { type: "provider.internal", message: big, status: 500, response: { body: big } },
  });
  await quiet("trim", 5);
  await trim.cleanup();
}

// `evaluate` raises only allow → ask, only for edit and shell, and keeps an
// "Always allow" answer's save patterns from asking again.
{
  const ev = await instance("evaluate", "");
  const query = (action, effect, resources) => ev.evaluate({ sessionID: "s", agent: "build", action, effect, resources });
  const actions = ["edit", "shell", "read", "grep", "webfetch", "question", "subagent", "bash", "write"];
  results.evaluateTable = actions.map((action) =>
    ["allow", "ask", "deny"].map((effect) => query(action, effect, ["a.txt"])),
  );
  await ev.event("permission.asked", { id: "p1", sessionID: "s", action: "shell", resources: ["ls -la"], save: ["ls *"] });
  await ev.event("permission.replied", { requestID: "p1", sessionID: "s", reply: "always" });
  await ev.event("permission.asked", { id: "p2", sessionID: "s", action: "edit", resources: ["b.txt"], save: ["b.txt"] });
  await ev.event("permission.replied", { requestID: "p2", sessionID: "s", reply: "once" });
  await ev.event("permission.asked", { id: "p3", sessionID: "s", action: "edit", resources: ["c.txt"], save: [] });
  await ev.event("permission.replied", { requestID: "p3", sessionID: "s", reply: "always" });
  await ev.event("permission.replied", { requestID: "unknown", sessionID: "s", reply: "always" });
  results.evaluateAlways = [
    query("shell", "allow", ["ls -la"]),
    query("shell", "allow", ["ls src", "ls -l"]),
    query("shell", "allow", ["ls -la", "rm -rf x"]),
    query("shell", "allow", []),
    query("shell", "deny", ["ls -la"]),
    query("edit", "allow", ["ls -la"]),
    query("edit", "allow", ["b.txt"]),
    query("edit", "allow", ["c.txt"]),
  ];
  await ev.cleanup();
}

// `context` adds the instructions to the agent's own sessions only; with no
// instructions file (or an unreadable one) no context hook is registered.
{
  const withFile = await instance("context", "", { options: { instructions: instructionsPath } });
  await withFile.event("session.created", { sessionID: "c", parentID: "p" });
  const run = (sessionID) => {
    const input = { sessionID, agent: "build", system: [{ type: "text", text: "base" }] };
    for (const fn of withFile.fake.hooks.context ?? []) fn(input);
    return input.system;
  };
  results.context = { own: run("p"), child: run("c") };
  await withFile.cleanup();
  const registered = [];
  for (const options of [{}, { instructions: `${logDir}/no-such-file.md` }, { instructions: 5 }, undefined]) {
    const none = await instance("context-none", "", { options });
    registered.push((none.fake.hooks.context ?? []).length);
    await none.cleanup();
  }
  results.contextWithout = registered;
}

await sleep(200);
process.stdout.write(JSON.stringify({ evaluations, results, errors }));
process.exit(0);
