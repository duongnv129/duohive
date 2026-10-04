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
// Its `config` hook makes `edit` and `bash` ask where the user's rules would
// allow them (S2.11), and never loosens anything. OpenCode puts each agent's
// built-in rules before the user's, then the user's top level, then the
// user's agent block, and the last matching rule wins (`rules.findLast`), in
// key order. So the hook leaves the top level alone and, for each agent,
// rewrites the user's rules for `edit` and `bash` into one key placed last in
// that agent's block: the same patterns and decisions, with "*" made "ask"
// where it allows. Where an agent's built-in rules restrict a key (plan's
// edit, explore's edit, the hidden agents' everything; `opencode agent list`,
// 1.18.34), no "*" is added, so the built-in still decides. All-digit
// patterns, which a JS object moves first, are kept by raising the pattern
// that would otherwise decide for them. Only the agents named here or in the
// user's config are composed; DuoHive launches `build` (`--agent=build`).
// It never answers a permission prompt itself (TD §2).
//
// It must never disturb OpenCode (plan Review Focus 1): every hook returns at
// once; deliveries run one at a time on an internal promise chain, in event
// order, never awaited by OpenCode; a spawn that throws, fails or exits
// non-zero is dropped; nothing is thrown into OpenCode; the child's output is
// ignored; a delivery still running after 2 s is killed; `dispose` waits for
// the chain for at most 1 s, kills a delivery still running and drops the
// queued ones, then sends session-end with a synchronous spawn bounded at
// 1 s (an async child started during shutdown may not run).
//
// Sessions with a parentID (subagents, unmeasured in spike S2) are ignored
// for busy and turn-end: only the agent's own session reports its turn.
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

// OpenCode 1.18.34's agents, and the guarded keys their built-in rules
// restrict (`opencode agent list`): adding "*": "ask" there would loosen them.
const BUILTIN_AGENTS = ["build", "plan", "general", "explore", "compaction", "summary", "title"];
const RESTRICTED = {
  plan: ["edit"],
  explore: ["edit"],
  compaction: GUARDED,
  summary: GUARDED,
  title: GUARDED,
};

function wildcard(pattern, text) {
  const escaped = String(pattern).replace(/[.+^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped.replace(/\*/g, ".*").replace(/\?/g, ".")}$`, "s").test(text);
}

// A `permission` block as OpenCode's ordered rules.
function rulesOf(block) {
  if (typeof block === "string") return [{ permission: "*", pattern: "*", action: block }];
  if (!isObject(block)) return [];
  return Object.entries(block).flatMap(([permission, value]) => {
    if (typeof value === "string") return [{ permission, pattern: "*", action: value }];
    if (!isObject(value)) return [];
    return Object.entries(value)
      .filter(([, action]) => typeof action === "string")
      .map(([pattern, action]) => ({ permission, pattern, action }));
  });
}

// The user's rules that match `key`, as one pattern object with the same
// decisions (a pattern seen twice keeps its last place and action), "*"
// raised from "allow" to "ask", and "*": "ask" added first when absent
// unless the agent's built-in restricts the key.
function composeKey(key, rules, restricted) {
  const patterns = new Map();
  for (const rule of rules) {
    if (!wildcard(rule.permission, key)) continue;
    patterns.delete(rule.pattern);
    patterns.set(rule.pattern, rule.action);
  }
  if (patterns.get("*") === "allow") patterns.set("*", "ask");
  const ordered =
    !patterns.has("*") && !restricted ? [["*", "ask"], ...patterns] : [...patterns];
  return ordered.length > 0 ? keepIndexPatterns(ordered) : undefined;
}

const RANK = { allow: 0, ask: 1, deny: 2 };

function lastMatch(entries, target) {
  let action;
  for (const [pattern, a] of entries) if (wildcard(pattern, target)) action = a;
  return action;
}

// A JS object puts array-index keys ("5", "2024") first, ascending, whatever
// their place, so a later pattern could decide for one. For each such
// pattern, the string pattern that would now decide is raised to the
// decision the intended order gives, so nothing gets less strict.
function keepIndexPatterns(ordered) {
  const out = Object.fromEntries(ordered);
  for (const [pattern] of ordered) {
    if (!/^(0|[1-9]\d*)$/.test(pattern) || Number(pattern) >= 2 ** 32 - 1) continue;
    const want = lastMatch(ordered, pattern);
    const actual = Object.entries(out);
    if ((RANK[lastMatch(actual, pattern)] ?? 2) >= (RANK[want] ?? 2)) continue;
    const decider = actual.filter(([p]) => p !== pattern && wildcard(p, pattern)).pop();
    if (decider) out[decider[0]] = want;
  }
  return out;
}

function composePermissions(cfg) {
  if (!isObject(cfg)) return;
  const agents = isObject(cfg.agent) ? cfg.agent : {};
  const top = rulesOf(cfg.permission);
  const out = { ...agents };
  for (const name of new Set([...BUILTIN_AGENTS, ...Object.keys(agents)])) {
    const agent = agents[name] === undefined ? {} : agents[name];
    if (!isObject(agent)) continue;
    const own = agent.permission;
    if (own !== undefined && typeof own !== "string" && !isObject(own)) continue;
    const restricted = RESTRICTED[name] ?? [];
    const composed = {};
    for (const key of GUARDED) {
      const value = composeKey(key, [...top, ...rulesOf(own)], restricted.includes(key));
      if (value !== undefined) composed[key] = value;
    }
    if (Object.keys(composed).length === 0) continue;
    const block = typeof own === "string" ? { "*": own } : (own ?? {});
    const rest = Object.fromEntries(Object.entries(block).filter(([key]) => !GUARDED.includes(key)));
    out[name] = { ...agent, permission: { ...rest, ...composed } };
  }
  cfg.agent = out;
}

// Payloads carry only what DuoHive reads, so a tool's output or a diff never
// fills the hook's stdin.
function toolPayload(input) {
  return { input: { tool: input?.tool, sessionID: input?.sessionID } };
}

function askedPayload(event) {
  const p = event.properties ?? {};
  return {
    type: event.type,
    properties: {
      id: p.id,
      sessionID: p.sessionID,
      permission: p.permission,
      patterns: p.patterns,
      metadata: { filepath: p.metadata?.filepath },
    },
  };
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
  // Test hook, like DUOHIVE_START_FALLBACK_MS: a loaded machine can take longer
  // than 1 s to start even a trivial hook exe, which the replay test must not
  // mistake for a lost session-end.
  const endLimit = Number(process.env.DUOHIVE_SESSION_END_LIMIT_MS);
  const endLimitMs = Number.isFinite(endLimit) && endLimit > 0 ? endLimit : SESSION_END_LIMIT_MS;
  // Test hook, same reason: under a fully loaded machine a first exec of a
  // fresh fake exe can take over 2 s, which would drop session-start.
  const limit = Number(process.env.DUOHIVE_DELIVERY_LIMIT_MS);
  const deliveryLimitMs = Number.isFinite(limit) && limit > 0 ? limit : DELIVERY_LIMIT_MS;
  const enabled = exe !== "" && agent !== "";

  let chain = Promise.resolve();
  let started = false;
  let busy = false;
  let disposing = false;
  let disposed = false;
  // Per session: the last assistant message that had text, and its text parts.
  const replies = new Map();
  // Subagent sessions (with a parentID): their status and idle are not the agent's.
  const children = new Set();
  let inFlight;

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
    if ((event.type === "session.created" || event.type === "session.updated") && p.info?.parentID) {
      children.add(p.info.id);
    }
    if ((event.type === "session.status" || event.type === "session.idle") && children.has(p.sessionID)) {
      return;
    }
    switch (event.type) {
      case "session.status":
        if (p.status?.type === "busy") markBusy(event);
        break;
      case "permission.replied":
        markBusy(event);
        break;
      case "permission.asked":
        busy = false;
        deliver("needs-input", askedPayload(event));
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
        timeout: endLimitMs,
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
    "chat.message": async (input) => {
      try {
        start();
        markBusy({ input: { sessionID: input?.sessionID } });
      } catch {}
    },
    "tool.execute.before": async (input) => {
      try {
        markBusy(toolPayload(input));
      } catch {}
    },
    "tool.execute.after": async (input) => {
      try {
        markBusy(toolPayload(input));
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
        // A delivery still running would land after session-end.
        try {
          inFlight?.kill("SIGKILL");
        } catch {}
        sendSessionEnd();
      } catch {}
    },
  };
};
