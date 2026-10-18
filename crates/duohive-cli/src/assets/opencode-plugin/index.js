// DuoHive's OpenCode plugin (spike S2 addendum, OpenCode 2.0.29).
//
// Written by DuoHive as <data_dir>/run/<agent>/opencode-plugin/index.js for
// each launch (OpenCode 2.0 loads a plugin directory, not a single file) and
// named in the per-launch OPENCODE_CONFIG file as
//   {"plugins": [{"package": "file://<run_dir>/opencode-plugin",
//                 "options": {"instructions": "<run_dir>/instructions.md"}}]}
// DuoHive launches `opencode --standalone`, so the plugin runs in that
// launch's own server child; without `--standalone` the TUI talks to the
// user's shared background service, which never reads OPENCODE_CONFIG. It
// never touches the user's global configuration.
//
// It tells DuoHive what the agent is doing by running
//   $DUOHIVE_HOOK_EXE hook $DUOHIVE_HOOK_AGENT <event>
// as an argv (no shell) with a JSON payload on stdin, from the events of
// `ctx.event.subscribe()` (each `{type, data}`):
//   session-start  1.5 s ($DUOHIVE_START_DELAY_MS) after the first
//                  `plugin.updated` (the input box draws about 0.7 s after it,
//                  and a paste sent at it was lost), or the first
//                  `session.execution.started`, or a fallback timer (3 s, or
//                  $DUOHIVE_START_FALLBACK_MS) if neither comes; and always
//                  before any other delivery
//   busy           `session.execution.started`, `permission.replied` (not a
//                  reject), `form.replied`; sent only on a change
//   needs-input    `permission.asked`, `form.created` (the question tool)
//   turn-end       `session.execution.succeeded`, `.failed` (with its error)
//                  and `.interrupted`, with `duohive_last_reply`: the text
//                  parts (`session.text.ended`) of the turn's last assistant
//                  message, in ordinal order
//   session-end    the cleanup function `setup` returns (it runs at shutdown,
//                  also after a kill -9 of the TUI)
//
// Its `evaluate` permission hook raises the final effect from "allow" to
// "ask" for the `edit` and `shell` actions, and loosens nothing but DuoHive's
// own tools ("ask" to "allow", P3-D1; a "deny" stays), so
// the PRD's rule that writes and commands wait for the user holds on
// OpenCode's defaults (everything allowed). A resource the user approved
// with "Always allow" (the request's `save` patterns) is not raised again.
// It never answers a permission prompt itself (TD §2).
//
// Its `context` session hook adds the stacked instructions (the file named in
// the plugin's `options.instructions`) to each of the agent's own sessions.
//
// It must never disturb OpenCode: nothing is thrown into OpenCode; hooks
// return at once; deliveries run one at a time on an internal promise chain,
// in event order, never awaited by OpenCode; a spawn that throws, fails or
// exits non-zero is dropped; the child's output is ignored; a delivery still
// running after 10 s is killed (a loaded machine can take seconds to start
// one; deliveries stay serial); payloads carry only what DuoHive reads, so a
// diff or a tool's output never fills the hook's stdin. The cleanup kills a
// delivery still running and drops the queued ones, then sends session-end
// with a synchronous spawn bounded at 1 s (an async child started during
// shutdown may not run).
//
// Child sessions (`session.created` with a parentID: subagents) are ignored:
// only the agent's own session reports its turn.
//
// Only node:child_process and node:fs are used, which Bun also provides, so
// the plugin runs under Bun inside OpenCode and under Node in DuoHive's tests.
import { spawn, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const DELIVERY_LIMIT_MS = 10000;
const SESSION_END_LIMIT_MS = 1000;
const START_DELAY_MS = 1500;
const START_FALLBACK_MS = 3000;
// The actions OpenCode 2.0 asks about for file writes (write, edit, patch)
// and for the shell tool.
const GUARDED = new Set(["edit", "shell"]);
// DuoHive's own MCP tools, `duohive_<tool>`, run without a prompt (P3-D1).
const DUOHIVE_TOOL = /^duohive_[a-z_]+$/;
const TURN_ENDS = new Set([
  "session.execution.succeeded",
  "session.execution.failed",
  "session.execution.interrupted",
]);
// Bounds on what a needs-input or turn-end payload carries.
const MAX_RESOURCES = 10;
const MAX_TEXT = 500;

function isObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function envMs(name, fallback) {
  const n = Number(process.env[name]);
  return Number.isFinite(n) && n > 0 ? n : fallback;
}

function cut(value) {
  return typeof value === "string" ? value.slice(0, MAX_TEXT) : undefined;
}

function wildcard(pattern, text) {
  const escaped = String(pattern).replace(/[.+^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped.replace(/\*/g, ".*").replace(/\?/g, ".")}$`, "s").test(String(text));
}

function payloadText(payload) {
  try {
    return JSON.stringify(payload ?? {}) ?? "{}";
  } catch {
    return "{}";
  }
}

function strings(list) {
  return Array.isArray(list) ? list.filter((s) => typeof s === "string") : [];
}

function readInstructions(options) {
  try {
    const path = isObject(options) ? options.instructions : undefined;
    return typeof path === "string" && path !== "" ? readFileSync(path, "utf8") : "";
  } catch {
    return "";
  }
}

async function setup(ctx) {
  const exe = process.env.DUOHIVE_HOOK_EXE || "";
  const agent = process.env.DUOHIVE_HOOK_AGENT || "";
  const enabled = exe !== "" && agent !== "";
  // Test hooks: the defaults are the product's; tests shorten or lengthen them.
  const startDelayMs = envMs("DUOHIVE_START_DELAY_MS", START_DELAY_MS);
  const fallbackMs = envMs("DUOHIVE_START_FALLBACK_MS", START_FALLBACK_MS);
  const deliveryLimitMs = envMs("DUOHIVE_DELIVERY_LIMIT_MS", DELIVERY_LIMIT_MS);
  const endLimitMs = envMs("DUOHIVE_SESSION_END_LIMIT_MS", SESSION_END_LIMIT_MS);
  const instructions = readInstructions(ctx?.options);

  let chain = Promise.resolve();
  let started = false;
  let busy = false;
  let disposed = false;
  let inFlight;
  let delayTimer;
  // Subagent sessions: their execution, text, approvals and forms are not the agent's.
  const children = new Set();
  // Per session: the last assistant message with text, and its parts by ordinal.
  const replies = new Map();
  // Open approval requests, by id: their action and "Always allow" patterns.
  const asked = new Map();
  // What the user approved with "Always allow": { action, patterns }.
  const always = [];

  function runOne(event, body) {
    return new Promise((resolve) => {
      let child;
      try {
        child = spawn(exe, ["hook", agent, event], { stdio: ["pipe", "ignore", "ignore"] });
      } catch {
        resolve();
        return;
      }
      const timer = setTimeout(() => {
        try {
          child.kill("SIGKILL");
        } catch {}
        resolve();
      }, deliveryLimitMs);
      inFlight = child;
      const done = () => {
        clearTimeout(timer);
        if (inFlight === child) inFlight = undefined;
        resolve();
      };
      try {
        child.on("error", done);
        child.on("exit", done);
        child.stdin?.on("error", () => {});
        child.stdin?.end(body);
      } catch {
        done();
      }
    });
  }

  function send(event, payload) {
    if (!enabled || disposed) return;
    const body = payloadText(payload);
    chain = chain.then(() => (disposed ? undefined : runOne(event, body))).catch(() => {});
  }

  function start() {
    if (started || disposed) return;
    started = true;
    clearTimeout(startTimer);
    clearTimeout(delayTimer);
    send("session-start", {});
  }

  // Every other delivery follows session-start.
  function deliver(event, payload) {
    start();
    send(event, payload);
  }

  function markBusy(payload) {
    if (busy) return;
    busy = true;
    deliver("busy", payload);
  }

  function lastReply(sessionID) {
    const last = replies.get(sessionID);
    if (!last) return null;
    return [...last.parts.entries()]
      .sort(([a], [b]) => a - b)
      .map(([, text]) => text)
      .join("\n");
  }

  function trackText(d) {
    let last = replies.get(d.sessionID);
    if (!last || last.messageID !== d.assistantMessageID) {
      last = { messageID: d.assistantMessageID, parts: new Map() };
      replies.set(d.sessionID, last);
    }
    const ordinal = Number(d.ordinal);
    last.parts.set(Number.isFinite(ordinal) ? ordinal : last.parts.size, String(d.text ?? ""));
  }

  function turnEnd(type, d) {
    busy = false;
    const payload = { type, sessionID: d.sessionID, duohive_last_reply: lastReply(d.sessionID) };
    if (type === "session.execution.failed") {
      const e = isObject(d.error) ? d.error : {};
      payload.error = {
        type: cut(e.type),
        message: cut(e.message),
        status: typeof e.status === "number" ? e.status : undefined,
      };
    }
    if (typeof d.reason === "string") payload.reason = cut(d.reason);
    deliver("turn-end", payload);
  }

  function onEvent(ev) {
    if (!isObject(ev) || typeof ev.type !== "string") return;
    const type = ev.type;
    const d = isObject(ev.data) ? ev.data : {};
    if (type === "plugin.updated" && !started && delayTimer === undefined) {
      delayTimer = setTimeout(() => {
        try {
          start();
        } catch {}
      }, startDelayMs);
      delayTimer.unref?.();
      return;
    }
    if (type === "session.created" && typeof d.parentID === "string") {
      children.add(d.sessionID);
      return;
    }
    const form = isObject(d.form) ? d.form : {};
    const sessionID = d.sessionID ?? form.sessionID;
    if (children.has(sessionID)) return;
    switch (type) {
      case "session.execution.started":
        replies.delete(sessionID);
        markBusy({ type, sessionID });
        break;
      case "session.text.ended":
        trackText(d);
        break;
      case "permission.asked":
        asked.set(d.id, { action: d.action, save: strings(d.save) });
        busy = false;
        deliver("needs-input", {
          type,
          properties: {
            id: d.id,
            sessionID,
            action: d.action,
            resources: strings(d.resources).slice(0, MAX_RESOURCES).map(cut),
          },
        });
        break;
      case "permission.replied": {
        const request = asked.get(d.requestID);
        asked.delete(d.requestID);
        if (d.reply === "always" && request && request.save.length > 0) {
          always.push({ action: request.action, patterns: request.save });
        }
        if (d.reply !== "reject") markBusy({ type, sessionID });
        break;
      }
      case "form.created": {
        const field = Array.isArray(form.fields) && isObject(form.fields[0]) ? form.fields[0] : {};
        busy = false;
        deliver("needs-input", {
          type,
          properties: {
            id: form.id,
            sessionID,
            title: cut(form.title),
            question: cut(field.description ?? field.title),
          },
        });
        break;
      }
      case "form.replied":
        markBusy({ type, sessionID });
        break;
      default:
        if (TURN_ENDS.has(type)) turnEnd(type, d);
        break;
    }
  }

  function alwaysAllowed(action, resources) {
    return (
      resources.length > 0 &&
      always.some(
        (rule) =>
          rule.action === action &&
          resources.every((res) => rule.patterns.some((p) => wildcard(p, res))),
      )
    );
  }

  function evaluate(input) {
    if (!isObject(input)) return;
    if (typeof input.action === "string" && DUOHIVE_TOOL.test(input.action)) {
      // Only an "ask" is loosened: a user's "deny" stays.
      if (input.effect === "ask") input.effect = "allow";
      return;
    }
    if (!GUARDED.has(input.action) || input.effect !== "allow") return;
    if (alwaysAllowed(input.action, strings(input.resources))) return;
    input.effect = "ask";
  }

  function addInstructions(input) {
    if (!isObject(input) || !Array.isArray(input.system) || children.has(input.sessionID)) return;
    input.system.push({ type: "text", text: instructions });
  }

  const startTimer = setTimeout(() => {
    try {
      start();
    } catch {}
  }, fallbackMs);
  startTimer.unref?.();

  (async () => {
    try {
      for await (const ev of ctx.event.subscribe()) {
        if (disposed) break;
        try {
          onEvent(ev);
        } catch {}
      }
    } catch {}
  })().catch(() => {});

  try {
    await ctx.permission.hook("evaluate", (input) => {
      try {
        evaluate(input);
      } catch {}
    });
  } catch {}
  if (instructions !== "") {
    try {
      await ctx.session.hook("context", (input) => {
        try {
          addInstructions(input);
        } catch {}
      });
    } catch {}
  }

  return () => {
    try {
      if (disposed) return;
      disposed = true;
      clearTimeout(startTimer);
      clearTimeout(delayTimer);
      // A delivery still running would land after session-end.
      try {
        inFlight?.kill("SIGKILL");
      } catch {}
      if (!enabled) return;
      spawnSync(exe, ["hook", agent, "session-end"], {
        input: "{}",
        stdio: ["pipe", "ignore", "ignore"],
        timeout: endLimitMs,
        killSignal: "SIGKILL",
      });
    } catch {}
  };
}

export default { id: "duohive", setup };
