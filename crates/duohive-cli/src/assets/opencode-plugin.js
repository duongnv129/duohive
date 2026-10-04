// DuoHive's OpenCode plugin (spike S2, OpenCode 1.18.34).
//
// Written by DuoHive into <data_dir>/run/<agent>/ for each launch and loaded
// from the per-launch OPENCODE_CONFIG file as file://<run_dir>/opencode-plugin.js.
// It never touches the user's global configuration.
//
// It tells DuoHive what the agent is doing by running
//   $DUOHIVE_HOOK_EXE hook $DUOHIVE_HOOK_AGENT <event>
// as an argv (no shell) with a JSON payload on stdin, for these events (S2.3):
//   session-start  the first `catalog.updated` after load, or the first prompt's
//                  `session.created` / `chat.message`, or a fallback timer
//                  (3 s, or $DUOHIVE_START_FALLBACK_MS) if neither comes
//   busy           `session.status` busy, `chat.message`, `tool.execute.before`
//                  and `.after`, `permission.replied`; sent only on a change
//   needs-input    `permission.asked`
//   turn-end       `session.idle`, with `duohive_last_reply` added (S2.4)
//   session-end    `dispose`
//
// Its `config` hook makes `edit` and `bash` ask where the user's value would
// allow them (S2.11). It never answers a permission prompt itself (TD §2).
//
// It must never disturb OpenCode (plan Review Focus 1): every hook returns at
// once; deliveries run one at a time on an internal promise chain, in event
// order, never awaited by OpenCode; a spawn that throws, fails or exits
// non-zero is dropped; nothing is thrown into OpenCode; the child's output is
// ignored; a delivery still running after 2 s is killed; `dispose` waits for
// the chain for at most 1 s, then sends session-end with a synchronous spawn
// bounded at 1 s (an async child started during shutdown may not run).
//
// Only node:child_process is used, which Bun also provides, so the plugin runs
// under Bun inside OpenCode and under Node in DuoHive's tests.
import { spawn, spawnSync } from "node:child_process";

const DELIVERY_LIMIT_MS = 2000;
const DISPOSE_WAIT_MS = 1000;
const SESSION_END_LIMIT_MS = 1000;
const START_FALLBACK_MS = 3000;

const START_EVENTS = new Set(["catalog.updated", "session.created"]);
const GUARDED = ["edit", "bash"];

function isObject(v) {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

// One permission value (a string or a pattern object) for edit or bash, made
// "ask" where it would allow. `fallbackAllows`: what an absent value (or an
// absent "*" in a pattern object) falls back to allows.
function tightenValue(value, fallbackAllows) {
  if (value === undefined) return fallbackAllows ? "ask" : undefined;
  if (value === "allow") return "ask";
  if (!isObject(value)) return value;
  const star = value["*"];
  if (star === "allow" || (star === undefined && fallbackAllows)) return { ...value, "*": "ask" };
  return value;
}

// A `permission` block, top level or an agent's. A string shorthand "allow"
// is read as {"*": "allow"}; any other string, and every "deny", "ask" and
// pattern other than "*", is kept as the user wrote it.
function tightenBlock(block, topLevel) {
  const b = block === "allow" ? { "*": "allow" } : block;
  if (b === undefined && !topLevel) return undefined;
  if (b !== undefined && !isObject(b)) return b;
  const current = b ?? {};
  const star = current["*"];
  const fallbackAllows = star === "allow" || (star === undefined && topLevel);
  const out = { ...current };
  for (const key of GUARDED) {
    const tightened = tightenValue(current[key], fallbackAllows);
    if (tightened !== undefined) out[key] = tightened;
  }
  return out;
}

function composePermissions(cfg) {
  if (!isObject(cfg)) return;
  cfg.permission = tightenBlock(cfg.permission, true);
  if (!isObject(cfg.agent)) return;
  for (const agent of Object.values(cfg.agent)) {
    if (isObject(agent) && agent.permission !== undefined) {
      agent.permission = tightenBlock(agent.permission, false);
    }
  }
}

function payloadText(payload) {
  try {
    return JSON.stringify(payload ?? {}) ?? "{}";
  } catch {
    return "{}";
  }
}

export const DuohivePlugin = async () => {
  const exe = process.env.DUOHIVE_HOOK_EXE || "";
  const agent = process.env.DUOHIVE_HOOK_AGENT || "";
  const fallback = Number(process.env.DUOHIVE_START_FALLBACK_MS);
  const fallbackMs = Number.isFinite(fallback) && fallback > 0 ? fallback : START_FALLBACK_MS;
  const enabled = exe !== "" && agent !== "";

  let chain = Promise.resolve();
  let started = false;
  let busy = false;
  let disposing = false;
  let disposed = false;
  // Per session: the last assistant message that had text, and its text parts.
  const replies = new Map();

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
      }, DELIVERY_LIMIT_MS);
      const done = () => {
        clearTimeout(timer);
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

  function deliver(event, payload) {
    if (!enabled || disposed) return;
    const body = payloadText(payload);
    chain = chain
      .then(() => (disposed ? undefined : runOne(event, body)))
      .catch(() => {});
  }

  function start() {
    if (started || disposed) return;
    started = true;
    clearTimeout(startTimer);
    deliver("session-start", {});
  }

  function markBusy(payload) {
    if (busy) return;
    busy = true;
    deliver("busy", payload);
  }

  // S2.4: text parts with an end time and not synthetic, by message and part;
  // a user message starts a new turn with no reply yet.
  function track(event) {
    const p = event.properties ?? {};
    if (event.type === "message.part.updated") {
      const part = p.part ?? {};
      if (part.type !== "text" || !part.time?.end || part.synthetic) return;
      const last = replies.get(part.sessionID);
      const current =
        last && last.messageID === part.messageID
          ? last
          : { messageID: part.messageID, parts: new Map() };
      current.parts.set(part.id, String(part.text ?? ""));
      replies.set(part.sessionID, current);
    } else if (event.type === "message.updated" && p.info?.role === "user") {
      replies.delete(p.info.sessionID);
    }
  }

  function lastReply(sessionID) {
    const last = replies.get(sessionID);
    return last ? [...last.parts.values()].join("\n") : null;
  }

  function onEvent(event) {
    if (!isObject(event) || typeof event.type !== "string") return;
    track(event);
    const p = event.properties ?? {};
    if (START_EVENTS.has(event.type)) start();
    switch (event.type) {
      case "session.status":
        if (p.status?.type === "busy") markBusy(event);
        break;
      case "permission.replied":
        markBusy(event);
        break;
      case "permission.asked":
        busy = false;
        deliver("needs-input", event);
        break;
      case "session.idle":
        busy = false;
        deliver("turn-end", { ...event, duohive_last_reply: lastReply(p.sessionID) });
        break;
      default:
        break;
    }
  }

  function sendSessionEnd() {
    if (!enabled) return;
    try {
      spawnSync(exe, ["hook", agent, "session-end"], {
        input: "{}",
        stdio: ["pipe", "ignore", "ignore"],
        timeout: SESSION_END_LIMIT_MS,
        killSignal: "SIGKILL",
      });
    } catch {}
  }

  const startTimer = setTimeout(() => {
    try {
      start();
    } catch {}
  }, fallbackMs);
  startTimer.unref?.();

  return {
    config: async (cfg) => {
      try {
        composePermissions(cfg);
      } catch {}
    },
    event: async (input) => {
      try {
        onEvent(input?.event);
      } catch {}
    },
    "chat.message": async (input, output) => {
      try {
        start();
        markBusy({ input, output });
      } catch {}
    },
    "tool.execute.before": async (input, output) => {
      try {
        markBusy({ input, output });
      } catch {}
    },
    "tool.execute.after": async (input, output) => {
      try {
        markBusy({ input, output });
      } catch {}
    },
    dispose: async () => {
      try {
        if (disposing) return;
        disposing = true;
        clearTimeout(startTimer);
        let waitTimer;
        const waited = new Promise((resolve) => {
          waitTimer = setTimeout(resolve, DISPOSE_WAIT_MS);
        });
        await Promise.race([chain, waited]);
        clearTimeout(waitTimer);
        disposed = true;
        sendSessionEnd();
      } catch {}
    },
  };
};
