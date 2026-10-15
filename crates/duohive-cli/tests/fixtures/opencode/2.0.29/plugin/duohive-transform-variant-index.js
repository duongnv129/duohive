// DuoHive's OpenCode 2.0 plugin (spike skeleton, OpenCode 2.0.29).
// Directory plugin: <run>/opencode-plugin/index.js, loaded from the per-launch
// OPENCODE_CONFIG as {"plugins": [{"package": "file://<run>/opencode-plugin", "options": {...}}]}.
// Requires `opencode --standalone` (the shared background service never sees OPENCODE_CONFIG).
import { spawn, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const DELIVERY_LIMIT_MS = 10000;
const SESSION_END_LIMIT_MS = 1000;
const START_DELAY_MS = Number(process.env.DUOHIVE_START_DELAY_MS) || 1500;
const GUARDED = new Set(["edit", "shell"]); // 2.0 actions: write/edit/patch -> "edit", shell tool -> "shell"

async function setup(ctx) {
  const exe = process.env.DUOHIVE_HOOK_EXE || "";
  const agent = process.env.DUOHIVE_HOOK_AGENT || "";
  const enabled = exe !== "" && agent !== "";
  let instructions = "";
  try {
    if (ctx.options?.instructions) instructions = readFileSync(String(ctx.options.instructions), "utf8");
  } catch {}

  let chain = Promise.resolve();
  let started = false, busy = false, disposed = false;
  const children = new Set();
  const replies = new Map(); // sessionID -> { messageID, parts: Map(ordinal -> text) }

  function runOne(event, body) {
    return new Promise((resolve) => {
      let child;
      try { child = spawn(exe, ["hook", agent, event], { stdio: ["pipe", "ignore", "ignore"] }); } catch { resolve(); return; }
      const timer = setTimeout(() => { try { child.kill("SIGKILL"); } catch {} resolve(); }, DELIVERY_LIMIT_MS);
      const done = () => { clearTimeout(timer); resolve(); };
      try { child.on("error", done); child.on("exit", done); child.stdin?.on("error", () => {}); child.stdin?.end(body); } catch { done(); }
    });
  }
  function deliver(event, payload) {
    if (!enabled || disposed) return;
    let body = "{}";
    try { body = JSON.stringify(payload ?? {}); } catch {}
    chain = chain.then(() => (disposed ? undefined : runOne(event, body))).catch(() => {});
  }
  function start() { if (started || disposed) return; started = true; deliver("session-start", {}); }
  function markBusy(payload) { if (busy) return; busy = true; deliver("busy", payload); }
  function lastReply(sid) { const r = replies.get(sid); return r ? [...r.parts.values()].join("\n") : null; }

  function onEvent(ev) {
    const type = ev?.type; const d = ev?.data ?? {};
    if (typeof type !== "string") return;
    if (type === "plugin.updated" && !started) setTimeout(() => { try { start(); } catch {} }, START_DELAY_MS).unref?.();
    if (type === "session.created" && d.parentID) children.add(d.sessionID);
    if (d.sessionID && children.has(d.sessionID)) return; // subagent sessions are not the agent's turn
    switch (type) {
      case "session.execution.started":
        start(); replies.delete(d.sessionID); markBusy({ sessionID: d.sessionID }); break;
      case "session.text.ended": {
        let r = replies.get(d.sessionID);
        if (!r || r.messageID !== d.assistantMessageID) { r = { messageID: d.assistantMessageID, parts: new Map() }; replies.set(d.sessionID, r); }
        r.parts.set(d.ordinal, String(d.text ?? "")); break;
      }
      case "permission.asked":
      case "form.created":
        busy = false;
        deliver("needs-input", { type, properties: { id: d.id, sessionID: d.sessionID, action: d.action, resources: d.resources } });
        break;
      case "permission.replied":
      case "form.replied":
        if (d.reply !== "reject") markBusy({ type, sessionID: d.sessionID }); break;
      case "session.execution.succeeded":
      case "session.execution.failed":
      case "session.execution.interrupted":
        busy = false;
        deliver("turn-end", { type, sessionID: d.sessionID, error: d.error, reason: d.reason, duohive_last_reply: lastReply(d.sessionID) });
        break;
      default: break;
    }
  }

  (async () => {
    try { for await (const ev of ctx.event.subscribe()) { try { onEvent(ev); } catch {} } } catch {}
  })();

  try {
    await ctx.agent.transform((editor) => {
      for (const a of editor.list()) {
        editor.update(a.id, (draft) => {
          draft.permissions = [...(draft.permissions ?? []), { action: "edit", resource: "*", effect: "ask" }, { action: "shell", resource: "*", effect: "ask" }];
        });
      }
    });
  } catch {}
  if (instructions) {
    try {
      await ctx.session.hook("context", (input) => {
        try { if (!children.has(input.sessionID)) input.system.push({ type: "text", text: instructions }); } catch {}
      });
    } catch {}
  }

  return () => {
    try {
      if (disposed) return;
      disposed = true;
      if (enabled) spawnSync(exe, ["hook", agent, "session-end"], { input: "{}", stdio: ["pipe", "ignore", "ignore"], timeout: SESSION_END_LIMIT_MS, killSignal: "SIGKILL" });
    } catch {}
  };
}

export default { id: "duohive", setup };
