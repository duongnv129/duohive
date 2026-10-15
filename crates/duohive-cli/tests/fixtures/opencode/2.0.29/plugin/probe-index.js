// DuoHive OpenCode 2.0 spike probe plugin: logs everything to $PROBE_LOG (JSONL).
import { appendFileSync } from "node:fs";
import { spawnSync, spawn } from "node:child_process";

const LOG = process.env.PROBE_LOG || "/tmp/oc2probe.jsonl";
const t0 = Date.now();
function log(kind, data) {
  try {
    appendFileSync(LOG, JSON.stringify({ t: Date.now(), dt: Date.now() - t0, pid: process.pid, kind, data }) + "\n");
  } catch {}
}
function safe(v, depth = 0) {
  try { return JSON.parse(JSON.stringify(v, (k, x) => typeof x === "function" ? "[fn]" : (typeof x === "bigint" ? String(x) : x))); } catch (e) { return String(e); }
}

log("module.loaded", { argv: process.argv, ppid: process.ppid, cwd: process.cwd(),
  env: Object.fromEntries(Object.entries(process.env).filter(([k]) => /^(DUOHIVE|PROBE|OPENCODE)/.test(k) && !/PASSWORD|KEY|TOKEN/.test(k))) });

async function setup(ctx) {
  log("setup", { keys: Object.keys(ctx), location: safe(ctx.location), options: safe(ctx.options),
    eventKeys: Object.keys(ctx.event || {}), subscribeSrc: String(ctx.event?.subscribe).slice(0, 600),
    sessionKeys: Object.keys(ctx.session || {}), permissionKeys: Object.keys(ctx.permission || {}),
    agentKeys: Object.keys(ctx.agent || {}), appKeys: Object.keys(ctx.app || {}) });
  // spawn test
  try {
    const r = spawnSync("/bin/sh", ["-c", "echo spawned-$PPID; cat"], { input: "stdin-ok", encoding: "utf8", timeout: 2000 });
    log("spawnSync", { status: r.status, stdout: r.stdout, error: r.error && String(r.error) });
  } catch (e) { log("spawnSync.throw", String(e)); }
  // events
  try {
    const sub = ctx.event.subscribe((ev) => log("event.cb", safe(ev)));
    log("subscribe.returned", { type: typeof sub, isPromise: sub instanceof Promise, asyncIter: !!(sub && sub[Symbol.asyncIterator]) });
    Promise.resolve(sub).then(async (s) => {
      log("subscribe.resolved", { type: typeof s, keys: s && Object.keys(s), asyncIter: !!(s && s[Symbol.asyncIterator]) });
      if (s && s[Symbol.asyncIterator]) { for await (const ev of s) log("event.iter", safe(ev)); }
      else if (s && s.stream && s.stream[Symbol.asyncIterator]) { for await (const ev of s.stream) log("event.stream", safe(ev)); }
    }).catch((e) => log("subscribe.err", String(e && e.stack || e)));
  } catch (e) { log("subscribe.throw", String(e && e.stack || e)); }
  // hooks
  for (const name of ["prompt", "context", "retry", "model.request"]) {
    try {
      await ctx.session.hook(name, (input) => {
        const s = safe(input);
        if (s && s.messages) s.messages = s.messages.slice(-2);
        if (s && s.system) {
          const txt = JSON.stringify(input.system);
          s.system = { count: input.system.length, shapes: input.system.map((x) => Object.keys(x)), hasSentinel: txt.includes("HIVE-SENTINEL"), head: input.system.map((x) => String(x.text ?? x.content ?? "").slice(0, 80)) };
          if (name === "context" && process.env.PROBE_INJECT) { input.system.push({ type: "text", text: process.env.PROBE_INJECT }); s.injected = true; }
        }
        if (s && s.tools) s.tools = Object.keys(s.tools);
        log("session.hook." + name, s);
      });
    } catch (e) { log("session.hook.err." + name, String(e && e.stack || e)); }
  }
  try {
    await ctx.permission.hook("evaluate", (input) => {
      const before = safe(input);
      if (["edit", "shell", "bash", "write", "patch"].includes(input.action) && input.effect === "allow") input.effect = "ask";
      log("permission.evaluate", { before, after: input.effect });
    });
  } catch (e) { log("permission.hook.err", String(e && e.stack || e)); }
  try {
    await ctx.agent.transform((editor) => {
      const list = editor.list();
      log("agent.transform", list.map((a) => ({ id: a.id, model: a.model, permissions: a.permissions })));
    });
  } catch (e) { log("agent.transform.err", String(e && e.stack || e)); }
  return () => {
    log("cleanup", {});
    try {
      const r = spawnSync("/bin/sh", ["-c", "echo cleanup-spawn >> \"$PROBE_LOG.cleanup\""], { timeout: 1000 });
      log("cleanup.spawnSync", { status: r.status, error: r.error && String(r.error) });
    } catch (e) { log("cleanup.spawn.throw", String(e)); }
  };
}

export default { id: "duohive-probe", setup };
