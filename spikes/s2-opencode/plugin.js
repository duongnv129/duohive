// Throwaway OpenCode plugin for spike S2. Loaded per launch from the
// OPENCODE_CONFIG file's "plugin" list; it never touches global config.
//
// 1. Every bus event is appended to $SPIKE_RAW/events.jsonl (streaming deltas
//    included), so the spike sees what OpenCode 1.18.34 emits and in what order.
// 2. Events that could map to a DuoHive event, and the plugin hooks
//    `permission.ask`, `tool.execute.before`, `tool.execute.after` and
//    `chat.message`, are each sent as JSON on the stdin of
//      record-hook.sh <agent> <name>
//    the way the real plugin would run `duohive hook <agent> <event>`.
// The plugin never returns a permission decision (TD §2).
import { appendFileSync, mkdirSync } from "node:fs";

const RAW = process.env.SPIKE_RAW || "/tmp/duohive-spike/raw-oc";
const HOOK = process.env.SPIKE_HOOK;
const AGENT = process.env.SPIKE_AGENT || "unknown";

const FORWARD = new Set([
  "server.connected",
  "session.created",
  "session.updated",
  "session.status",
  "session.idle",
  "session.error",
  "session.deleted",
  "permission.asked",
  "permission.updated",
  "permission.replied",
  "question.asked",
  "question.replied",
  "question.rejected",
  "server.instance.disposed",
  "global.disposed",
]);

function send(name, payload) {
  if (!HOOK) return;
  try {
    const p = Bun.spawn([HOOK, AGENT, name], {
      stdin: new TextEncoder().encode(JSON.stringify(payload)),
      stdout: "ignore",
      stderr: "ignore",
    });
    p.unref?.();
  } catch (e) {
    appendFileSync(`${RAW}/plugin-errors.log`, `${name}: ${e}\n`);
  }
}

// Synchronous variant for shutdown paths, where an async child may not get to run.
function sendSync(name, payload) {
  if (!HOOK) return;
  try {
    Bun.spawnSync([HOOK, AGENT, name], {
      stdin: new TextEncoder().encode(JSON.stringify(payload)),
      stdout: "ignore",
      stderr: "ignore",
    });
  } catch (e) {
    appendFileSync(`${RAW}/plugin-errors.log`, `${name}: ${e}\n`);
  }
}

// Reply tracking from the event stream alone: the text parts of each assistant
// message, in order, kept once the part has ended.
const textParts = new Map(); // messageID -> Map(partID -> text)
let lastAssistant = new Map(); // sessionID -> messageID of the last assistant message with text

function replyFromParts(messageID) {
  const parts = textParts.get(messageID);
  return parts ? [...parts.values()].join("") : null;
}

// Reply from the server API: the last assistant message after the last user
// message that has any text part.
async function replyFromApi(client, sessionID) {
  const res = await client.session.messages({ path: { id: sessionID } });
  const msgs = res.data ?? res;
  let reply = null;
  for (const m of msgs) {
    if (m.info.role === "user") reply = null;
    if (m.info.role !== "assistant") continue;
    const text = m.parts.filter((p) => p.type === "text" && !p.synthetic).map((p) => p.text).join("");
    if (text) reply = text;
  }
  return reply;
}

// Review fix: compose "ask" for edit and bash only where the user's resolved value
// is "allow" or absent (global level), or "allow" (agent level); "ask", "deny" and
// every pattern other than "*" stay as the user wrote them. Enabled by SPIKE_COMPOSE=1.
function tighten(v, absentToAsk) {
  if (v === undefined) return absentToAsk ? "ask" : undefined;
  if (v === "allow") return "ask";
  if (typeof v === "string") return v;
  const out = { ...v };
  if (out["*"] === "allow" || (out["*"] === undefined && absentToAsk)) out["*"] = "ask";
  return out;
}
function tightenBlock(p, absentToAsk) {
  if (typeof p === "string") p = { "*": p };
  const out = { ...(p ?? {}) };
  for (const k of ["edit", "bash"]) {
    const v = tighten(out[k], absentToAsk);
    if (v !== undefined) out[k] = v;
  }
  return out;
}

export const DuohiveSpike = async (ctx) => {
  mkdirSync(RAW, { recursive: true });
  process.on("exit", (code) => {
    appendFileSync(`${RAW}/events.jsonl`, JSON.stringify({ t: Date.now(), plugin: "process.exit", code }) + "\n");
    sendSync("process.exit", { code });
  });
  for (const sig of ["SIGTERM", "SIGHUP", "SIGINT"]) {
    process.on(sig, () => {
      appendFileSync(`${RAW}/events.jsonl`, JSON.stringify({ t: Date.now(), plugin: `signal.${sig}` }) + "\n");
      sendSync(`process.${sig}`, {});
    });
  }
  appendFileSync(
    `${RAW}/events.jsonl`,
    JSON.stringify({ t: Date.now(), plugin: "init", directory: ctx.directory, serverUrl: String(ctx.serverUrl) }) + "\n",
  );
  send("plugin.init", { directory: ctx.directory, worktree: ctx.worktree });
  return {
    config: async (cfg) => {
      if (process.env.SPIKE_COMPOSE !== "1") return;
      cfg.permission = tightenBlock(cfg.permission, true);
      for (const a of Object.values(cfg.agent ?? {})) {
        if (a && a.permission !== undefined) a.permission = tightenBlock(a.permission, false);
      }
      appendFileSync(`${RAW}/events.jsonl`, JSON.stringify({ t: Date.now(), plugin: "config", permission: cfg.permission }) + "\n");
    },
    dispose: async () => {
      appendFileSync(`${RAW}/events.jsonl`, JSON.stringify({ t: Date.now(), plugin: "dispose" }) + "\n");
      sendSync("hook.dispose", {});
    },
    event: async ({ event }) => {
      appendFileSync(`${RAW}/events.jsonl`, JSON.stringify({ t: Date.now(), ...event }) + "\n");
      const p = event.properties ?? {};
      if (event.type === "message.part.updated" && p.part?.type === "text" && p.part.time?.end && !p.part.synthetic) {
        const m = textParts.get(p.part.messageID) ?? new Map();
        m.set(p.part.id, p.part.text);
        textParts.set(p.part.messageID, m);
        lastAssistant.set(p.part.sessionID, p.part.messageID);
      }
      if (event.type === "message.updated" && p.info?.role === "user") {
        lastAssistant.delete(p.info.sessionID); // a new turn: no reply yet
      }
      if (event.type === "session.idle") {
        const fromParts = lastAssistant.has(p.sessionID) ? replyFromParts(lastAssistant.get(p.sessionID)) : null;
        let fromApi = null;
        let apiError = null;
        try {
          fromApi = await replyFromApi(ctx.client, p.sessionID);
        } catch (e) {
          apiError = String(e);
        }
        send("session.idle.reply", { ...event, duohive_last_reply: fromParts, api_last_reply: fromApi, api_error: apiError });
        return;
      }
      if (FORWARD.has(event.type)) send(event.type, event);
      // A finished assistant message carries the reply's metadata; record those too.
      if (event.type === "message.updated" && event.properties?.info?.time?.completed) {
        send("message.updated.completed", event);
      }
    },
    "permission.ask": async (input, output) => {
      send("hook.permission.ask", { input, output });
      // no decision: output.status is left as OpenCode set it
    },
    "tool.execute.before": async (input, output) => {
      send("hook.tool.execute.before", { input, output });
    },
    "tool.execute.after": async (input, output) => {
      send("hook.tool.execute.after", { input, output });
    },
    "chat.message": async (input, output) => {
      send("hook.chat.message", { input, output });
    },
  };
};
