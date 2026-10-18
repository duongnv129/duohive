# DuoHive Phase 3 Implementation Plan — Duo and messaging

**Status:** Ready · expanded 2026-10-18 from the outline written 2026-10-05. Phase 2's three-day run without `tmux attach` (Task 2.19 Step 2) is still open; phase 3 starts alongside it by the author's choice, and nothing here depends on that run.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** You direct the team through Duo.
- **Duo** sees the org, messages agents, starts and stops them, and proposes org changes you approve on cards.
- **Agents** message each other safely, identified by token, never by name.
- **A task given to a role** reaches a free agent, or a card proposes a new one.

**Done when (roadmap row 3):** a "what's blocked, then fix it" round trip happens entirely in chat, and a task given to a role reaches a free agent.

**Architecture:** the layers stay as in phases 0–2 (TD §3).
- **`duohive-mcp`** gains the endpoint (DuoHive's side of a tool call) and the `duohive mcp` helper (the agent's side, a stdio MCP server). The helper is a thin relay: it parses nothing but MCP; DuoHive parses the tool arguments and checks the token.
- **A tool call is a command.** The endpoint sends `Command::Tool { call_id, token, call }` into the core and awaits `Event::ToolResult { call_id, result }`. Commands stay plain serialisable data; no channel travels inside one (TD §3.2).
- **One socket.** Hooks and tool calls share `run.sock`, told apart by a `kind` field (P3-D9). The listener in `duohive` hands tool connections to `duohive_mcp::endpoint`.
- **The core gains** the inbox, Duo's tools, approval cards, task to a role and the Duo chat transcript. Each is sans-I/O and stored through `Store`.
- **The TUI** replaces the phase-2 chat pane (P2-D8) with the real chat, cards, suggestions, and the role variants of Give work.

**Tech stack:** as phases 0–2 (Rust 1.99.0, tokio, ratatui `=0.30.2`, crossterm `=0.29.0`, rusqlite, insta, tempfile, `vt100 = "=0.16.2"`). New in this phase:
- **`rmcp`, pinned with `=`**, for the helper's MCP server, if the release binary stays under 10 MB (P3-D8, measured in Task 3.1); else a hand-written JSON-RPC over stdio. Both are behind the same `helper` tests.
- No other new crate. `serde_json` (already in `duohive-core`) carries tool arguments.

**Spec:**
- [PRD](../product/prd.md): Core capabilities (Duo, messages, Needs you, task to a role); The org model (Lifecycle: free, messages to a stopped agent); Duo, the manager agent; One screen (Duo pane); User flows; Delivery Phases row 3.
- [UI/UX spec](../ux/ui-spec.md): §2.2 (`✉`), §4.1.2 (role and empty-org copy), §4.1.3 (card rows), §4.1.4 (card alerts), §4.5, §4.6, §4.7, §4.11 (chat and card keys), §4.12 (empty role), §5.
- [Technical design](../design/technical-design.md): §2, §3.1–§3.4, §4.2, §5.1–§5.5, §6.1–§6.5, §7, §8, §9, §12, §13.
- [Spike report](spike-report.md): S1.5, S2.5, S2.14.
- [Roadmap](v1-roadmap.md) row 3 and [traceability](v1-traceability.md).

## Global Constraints

- **Phase 0's [Global Constraints](phase-0-plan.md#global-constraints) still hold:**
  - toolchain `1.99.0`, edition 2024, Apache-2.0;
  - the dependency rule: `duohive-core` depends on no `duohive-*` crate; `duohive-tmux`, `duohive-cli`, `duohive-store`, `duohive-tui` and `duohive-mcp` depend only on `duohive-core`; only `duohive` depends on all of them;
  - `duohive-core` does no I/O (tokio `sync` and `time` only, for the driver);
  - no `unwrap`/`expect` outside tests;
  - no `unsafe`, except `crates/duohive/src/bench/measure.rs`;
  - Ratatui and crossterm pinned with `=`, and every new crate too (`rmcp`);
  - no `std::env::set_var` in tests: code that reads the environment takes a `HashMap<String, String>`, and tests of the built binary use `Command::env`;
  - tmux tests run on a throwaway socket, never on `duohive`;
  - the user's global CLI configuration is never modified: MCP config, permissions and the Duo prompt are passed per launch from `<data_dir>/run/<agent>/`;
  - supported CLIs: Claude Code and OpenCode only;
  - conventional commits, with no `Co-Authored-By` line, and never push.
- **Every commit passes `make check`:** format, clippy `-D warnings`, tests, `cargo deny` and `typos`.
  - A task that changes a shared type updates every construction and match site in the same commit; its Files list names them.
  - New `Command` and `Event` variants, and the shared data types, are added in Task 3.1. Until the task that builds them, the engine answers a new command with `CommandRejected("not built yet")` (a `Command::Tool` with `ToolError { code: Unavailable }`), and the TUI ignores a new event.
  - Migrations are append-only, in task order: 4 `pending_tool_calls` and `sessions.launched_as_duo` (3.2), 5 `messages` (3.4), 6 `chat` (3.8), 7 `cards` (3.6).
  - **Task order:** 3.1 → 3.2 → 3.3 → 3.4 → 3.8 → 3.6 → 3.7 → 3.5, then 3.9 (after 3.8), 3.10 (after 3.6) and 3.11 (after 3.7), and 3.12 last. Each core task uses what the ones before it built: 3.8 calls 3.4's `send`; 3.6 calls `send` and 3.8's `chat_entry`; 3.7 calls 3.6's `raise_card`; 3.5 dispatches the remaining tools over all of them.
- **Decisions in force:** D1–D32 (phase 1), P2-D1–P2-D28 (phase 2), and P3-D1–P3-D7. This plan replaces:
  - **P2-D8** (the phase-2 chat pane): Task 3.9;
  - **P2-D10** (Give work to agents only): Task 3.11;
  - **P2-D3's held-back copy** (role summary without `space give work to this role`; empty org and empty role without "ask Duo"): Tasks 3.9 and 3.11;
  - **D7's empty-org and empty-role text**: Task 3.9.
- **The narrow exception to "no auto-approve flags" (P3-D1):** each launch pre-approves DuoHive's own MCP tools and nothing else. Claude Code: `mcp__duohive__*` in the per-launch `--settings` file's `permissions.allow`. OpenCode: the plugin's `evaluate` hook allows only the `duohive` server's tools. No other tool or prompt is touched; nothing is written outside `<data_dir>/run/<agent>/`.
- **Identity:** every tool call carries the launch's `DUOHIVE_TOKEN`; the core resolves it to the agent with the same constant-time compare as hooks (`same_token`, made `pub(crate)` in 3.3). A name in a payload is never read for identity: unknown argument fields are ignored (P3-D17).
- **Tool rights (TD §6.2):**

  | Rights | Tools |
  |---|---|
  | Every agent (`Rights::Agent`) | `list_agents`, `send_message`, `message_status` |
  | Duo only (`Rights::Duo`, given by the Duo launch) | `read_agent`, `wait_for_agent`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change` |

- **Inbox (TD §6.3):**
  - **Delivery:** only when the recipient is finished or idle, with no delivery pending, never onto a waiting prompt. It counts once the recipient leaves rest (P2-D11); no sign within 5 s, or a failed paste, drops it.
  - **Statuses:** `queued`, `delivered`, `answered`, `dropped`. A dropped message is never retried by DuoHive.
  - **Limits, between agents only:** a duplicate is refused within 10 minutes; at most 5 a minute per sender → recipient pair; at most 20 waiting per recipient. You are never throttled.
- **Duo's limits:** no org change without a card; no stop of a starting, working or waiting agent without a card (finished, idle or errored: direct; stopped: refused, P3-D6); no proposal to remove an agent that isn't stopped; no tool answers a CLI prompt.
- **`wait_for_agent`:** 10 minutes by default, 60 at most (P3-D11).
- **Copy:** sentence case; agent names as monospace identifiers; "Give work" is the only term for handing over a task; colour never the only signal; no feature descriptions on screen (UI §1).
- **Snapshots (TD §9):** every screen this plan adds or changes gets `insta` snapshots at 80×24 and 160×48. A test name ending `_80x24` / `_160x48` stands for both.
- **Test-first applies** to every task except the live checks in 3.2 and the acceptance in 3.12, validated as those steps state.

## Review Focus

1. **Prompt injection across agents through message text.**
   - An agent's message reaches its recipient only inside a frame whose boundary is minted at delivery and never appears in the text; your own messages and card answers are never framed (P3-D18).
   - Duo-only tools are unreachable by token, whatever the text says; a card changes only on your key, never on any message's text.
   - Tests: Task 3.4 `agent_text_framed_with_fresh_boundary`, `boundary_never_in_text`, `your_text_typed_verbatim`; Task 3.3 `agent_cannot_call_duo_tools`, `name_in_payload_ignored`; Task 3.4 `send_message_sender_is_token_not_payload`; Task 3.6 `card_answer_only_from_command`.
2. **A tool call racing an agent's stop or restart, and a stale token.**
   - A token of a stopped agent, or of a previous launch, gets `unknown_token`; a wait on an agent that stops ends at once; a restarted Duo's old waits are dropped; a result goes only to the call id it belongs to.
   - Tests: Task 3.3 `token_of_stopped_agent_rejected`, `token_of_previous_launch_rejected`; Task 3.5 `wait_ends_when_target_stops`, `duo_restart_drops_its_waits`; Task 3.1 `result_reaches_only_its_call`.
3. **A card answered after its target changed state.**
   - Approving re-checks the change against the org and states as they are then; what can't apply is answered "approved, not applied" with the reason, and nothing half-applies (P3-D21).
   - Tests: Task 3.6 `approve_stop_after_target_finished_stops_directly`, `approve_stop_after_target_stopped_reports_it`, `approve_remove_after_agent_started_not_applied`, `approve_add_after_name_taken_not_applied`; Task 3.7 `approve_clone_after_role_deleted_not_applied`.
4. **Inbox delivery onto a prompt.**
   - Nothing is pasted into a starting, working or waiting agent, into one with a delivery pending, or into one you are typing into (P3-D19); a failed or untaken paste drops the message and never pastes it again.
   - Tests: Task 3.4 `never_typed_onto_waiting`, `never_typed_while_delivery_pending`, `never_typed_while_you_type`, `starting_agent_waits_for_rest`, `untaken_message_dropped_never_retried`.
5. **DuoHive closing mid-call.**
   - A call whose connection ends before its answer gets `interrupted`, and is never queued again; changing calls made while DuoHive is closed queue in `state.db` and run on open, with the token checked against the session rows; reads answer `duohive_closed`.
   - Tests: Task 3.2 `closed_mid_call_answers_interrupted_not_queued`, `closed_reads_answer_closed`, `closed_changes_queue_and_answer_queued`; Task 3.4 `closed_changes_queue_and_run_on_open`, `queued_call_with_unknown_token_dropped_on_open`, `persisted_and_replayed_after_restart`.

---

## File Structure

```
crates/duohive-core/src/
  tool.rs        CallId, Rights, ToolName, ToolCall, ToolOk, ToolError, ErrorCode, parse_call (3.1)        (new)
  inbox.rs       MessageId, Sender, MessageKind, MessageStatus, Message (3.1); limits, frame (3.4)       (new)
  card.rs        CardId, CardSource, CardChange, CardOutcome, Card (3.1); summary, answer_text (3.6)    (new)
  chat.rs        ChatId, ChatKind, ChatEntry (3.1)                                                      (new)
  duo.rs         DUO_PROMPT, duo_prompt, action_line (3.5)                                              (new)
  command.rs  event.rs  ports.rs (Ids::boundary, Store methods)  org.rs (OrgEdit::Move)  inherit.rs
  engine/{mod,lifecycle,signals,reconnect,timers,org_edit}.rs
  engine/tools.rs  rights.rs  inbox.rs  duo.rs  cards.rs  assign.rs  chat.rs                           (new)
  testkit.rs  engine/harness.rs
crates/duohive-mcp/src/
  lib.rs  protocol.rs (run.sock lines)  endpoint.rs  calls.rs  tools.rs (schemas, rendering)  helper.rs  (new)
crates/duohive-store/src/{db.rs, sqlite_store.rs, closed.rs (new: plain fns for a closed DuoHive)}   migrations 4–7
crates/duohive-cli/src/{claude.rs, opencode.rs, fake.rs, assets/opencode-plugin/index.js}
crates/duohive/src/
  mcp_cmd.rs (new: StoreFallback)  args.rs  main.rs  hook_listener.rs  system.rs  real_ports.rs  fake_agent.rs
crates/duohive/tests/{mcp.rs (new), e2e_duo.rs (new)}
crates/duohive-tui/src/
  chat.rs  suggest.rs (new)  model.rs  input.rs  focus.rs  tree.rs
  render/{chat_pane.rs, card.rs (new), needs_you.rs, agent_pane.rs, give_work.rs}
  forms/{give_work.rs, mod.rs, agent.rs}  input/mouse.rs
docs/plans/{bench-results.md (Phase 3), spike-report.md (S1.5/S2.14 addenda)} · TD · UI spec
```

## Author decisions

P3-D1–P3-D7 were decided by the author on 2026-10-17; P3-D8–P3-D11 answer the outline's open questions P3-Q1–P3-Q4 (2026-10-17: the author took the outline's defaults).

| # | Decision |
|---|---|
| P3-D1 | **MCP tool approvals: only DuoHive's tools are pre-approved.** Decided by the author on 2026-10-17. Each launch pre-approves DuoHive's own MCP tools and nothing else: Claude Code: `mcp__duohive__*` (the naming of spike S1.5, which never called the tool; a prompt is expected, like any MCP tool) in the per-launch `--settings` file's `permissions.allow`, checked live on the pinned Claude Code in 3.2. OpenCode: the plugin's `evaluate` hook (2.0) allows the `duohive` server's tools; the mechanism and the tool naming are checked on 2.0 in 3.2. It is a narrow exception to TD §2's "passes no auto-approve flags of its own": none of these tools commits, pushes, merges, publishes or deletes, and every CLI prompt for the agent's own work is untouched. Global config is never touched. This keeps the done-when "entirely in chat". Built in 3.2; TD §2 and §6.2 say so. |
| P3-D2 | **Card tools don't block.** Decided by the author on 2026-10-17. `stop_agent` (when it raises a card) and `propose_org_change` return at once `{status: "pending", card_id}`. The answer (approved, edited and approved, or rejected) reaches Duo later as a message from you through the inbox, so it follows the inbox's delivery rules. When Duo is stopped, the answer waits for it as `queued · stopped`, like an agent's message, and doesn't start Duo; it is delivered when Duo next runs. Built in 3.6; TD §6.2 says so. |
| P3-D3 | **The engine raises the no-free-agent card.** Decided by the author on 2026-10-17. TD §5.4 stands: the core raises the clone card itself, and the UI shows it as Duo's, so it works while Duo is stopped or busy. The no-free-agent variant of Give work says `enter propose one`, not "ask Duo". The answer to that card reaches Duo as a message from you, as in P3-D2. Built in 3.7 and 3.11; PRD Must and UI §4.5 say so. |
| P3-D4 | **A card's colour.** Decided by the author on 2026-10-17. A card's outline is `border` (heavy when focused), with an amber `◆` (`status.attention`) in its title; no amber outline (P2-D27). Built in 3.10; UI §4.6 says so. |
| P3-D5 | **Card alerts.** Decided by the author on 2026-10-17. A card's alert follows the `needs you` toggle (`settings.alerts.needs_you`), with the title `Duo needs you` and the card's summary as its body. Built in 3.6; UI §4.1.4 and TD §4.2 say so. |
| P3-D6 | **`stop_agent` by state.** Decided by the author on 2026-10-17. Finished or idle: direct. Errored: direct. Starting, working or waiting: a card. Stopped: refused, with the reason. Built in 3.5; TD §6.2 and the PRD say so. |
| P3-D7 | **`e` on cards.** Decided by the author on 2026-10-17. On a stop card `e` is disabled, and the footer gives the reason (`nothing to edit · y or n`). On a new-agent card `e` opens the agent form pre-filled, and the task the card holds is kept unchanged. Built in 3.10; UI §4.6 says so. |
| P3-D8 | **The MCP library (was P3-Q1).** `rmcp`, pinned with `=` at the latest release when Task 3.1 starts (3.5.0 on 2026-10-05), if the release binary stays under 10 MB with it. Task 3.1 measures `target/release/duohive` with and without it (8 518 032 bytes on 2026-10-17, before phase 3) and records both in `bench-results.md`. Over 10 MB, or failing `cargo deny`, the fallback is a minimal hand-written JSON-RPC 2.0 over stdio in `helper.rs` (`initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`). The helper's tests are the same for both. |
| P3-D9 | **One socket for hooks and tool calls (was P3-Q2).** The `duohive mcp` helper and DuoHive talk over `run.sock`, shared with hooks: one listener, one owner-only permission model. Each line carries a message-type field, `kind` (P3-D17). |
| P3-D10 | **A tool call while DuoHive is closed (was P3-Q3).** Calls that change something (`send_message`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change`) queue in `state.db` and run, in order, when DuoHive next opens. Reads (`list_agents`, `message_status`, `read_agent`, `wait_for_agent`) answer "DuoHive is closed", since a stale answer misleads. Task 3.2 records this in TD §6.3. |
| P3-D11 | **`wait_for_agent` timeouts (was P3-Q4).** 10 minutes by default, 60 minutes at most. A larger `timeout_minutes` is refused as `invalid_arguments`, not clamped, so Duo learns the limit. |

## Open questions

| # | Question | Status |
|---|---|---|
| P3-Q5 | How much chat history the pane loads and keeps | The default stands: the snapshot carries the last 500 entries (`CHAT_LOAD = 500`); the `chat` table keeps every entry. Built in 3.8 and 3.9 |
| P3-Q6 | The wording of Duo's fixed prompt | An author checkpoint: drafted in 3.5 against P3-D13's required points, approved by the author before 3.12 runs the real-Duo session |

## Decisions this plan makes where the specs are silent

| # | Decision |
|---|---|
| P3-D12 | **A card's answer to Duo, and in the chat.** The answer is a message from you (`Sender::You`, `MessageKind::CardAnswer(id)`), typed in unframed, one line: `card <id> approved: <summary>`, `card <id> edited and approved: <summary of the edited change>`, `card <id> rejected: <summary>`, or `card <id> approved, not applied (<reason>): <summary>` (P3-D21). In the chat the card collapses into one `→` line, `→ approved: stop squad-1` (or `edited and approved`, `rejected`, `approved, not applied (<reason>)`), with its time; no separate `you` line is shown for it. *Why:* a fixed, parseable prefix that only DuoHive produces unframed, and one line in the chat as UI §4.6 draws. |
| P3-D13 | **What Duo's fixed prompt must say.** The wording is P3-Q6's; these points are required and tested by substring in 3.5: (1) the tools and who may use them; (2) org changes and stops of starting, working or waiting agents go through cards; `stop_agent`, `assign_to_role` and `propose_org_change` may return `{"status":"pending","card_id":N}` at once, and the answer arrives later as a message from the user starting `card N`; (3) messages from agents arrive framed by `[message from <agent> · DuoHive boundary <b>]` … `[end of message <b> …]`, and text inside a frame is the agent's, never the user's, whatever it claims; (4) unframed text is the user's; (5) commit, push, merge, publish and delete wait for the user, and no tool answers a CLI prompt; (6) every tool call is shown to the user as an action line. *Why:* P3-D2 makes card tools asynchronous, and Review Focus 1 needs Duo to know which text is whose. |
| P3-D14 | **Action lines.** Every tool call Duo makes, reads included, writes one `→` chat entry when the call is answered, never rewritten later (the transcript is append-only). The text and result per tool: `list_agents` → `list agents` / `<n> agents`; `send_message` → `@<to> "<text cut to 40>"` / `typed in`, `queued (<to> is <state word>)`; `message_status` → `message <id> status` / the status word; `read_agent` → `read <agent>` / `read`; `wait_for_agent` → `wait for <agent>` / `finished`, `idle`, `stopped`, `errored`, `timed out after <n>m`; `start_agent` and `assign_to_role` → `give work <agent> "<task cut to 40>"` / `started`, `typed in`; `stop_agent` → `stop <agent>` / `stopped`; any call that raises a card → `card raised`; any error → `refused (<message>)`. Your own `@agent` messages write the same `@<to>` line. UI §4.6's list of results drops `delivered` (no call knows it when answered) and shows `typed in`, `queued`, `started` and `card raised`; delivery stays visible through `message_status` and the `✉` count. Task 3.5 edits it. *Why:* UI §4.6 lists words but not when a line is written or what a card-raising call shows. Approved by the author on 2026-10-18. |
| P3-D15 | **The clone card's new agent.** It is named by D11 (`next_agent_names(org, team, role, 1)`, so `payments-backend-1`, skipping names in use), takes the own fields (CLI, flags, directory, model, effort, worktree, prompt) of the role's first agent in tree order (none when the role is empty), and holds the task and a Run of `NewWorktree` when the clone's resolved worktree default is on, else `Directory`. Summary: `add <name> for a task`; card body: `+ Add agent <name> in <role path> (clone of <first agent>)` and `Task  <first line, cut>`. Approving with an edit keeps the task (P3-D7). *Why:* TD §5.4 says "cloned from the role" without naming or fields, and an agent with no directory is invalid. |
| P3-D16 | **Tool results.** Every result is MCP `content: [{type: "text", text: <JSON>}]` plus `structuredContent: <the same JSON>`; errors set `isError: true` and the JSON `{"error": {"code": "<code>", "message": "<text>"}}`. Success shapes: `list_agents` `{"you": "<caller>", "agents": [{"name", "team", "role", "state", "quiet", "from_screen", "for_secs", "last_reply", "waiting_messages", "cli", "worktree", "free"}]}` (Duo first, `team` and `role` null); `send_message` `{"message_id": 42, "status": "typed_in"\|"queued"\|"started", "recipient_state": "<state word>"}` (`ToolOk::Queued { message, outcome: SendOutcome }`: `TypedIn` when the recipient was at rest and the paste went out, `Queued(state)` when it waits, `Started` when your message launched a stopped or errored agent; an agent's message to a stopped agent is `queued` with `recipient_state: "stopped"`, which is `queued · stopped`); `message_status` `{"message_id", "to", "status", "reason"}`, where `reason` is the drop reason for `dropped`, and for `queued` says what it waits on (`pay-qa is stopped`, `pay-qa is errored`, `pay-qa is working`); `read_agent` `{"agent", "state", "screen"}`; `wait_for_agent` `{"agent", "outcome": "at_rest"\|"turn_ended"\|"timed_out"\|"stopped"\|"errored", "state", "last_reply"}` (an agent already stopped or errored answers at once with that outcome); `start_agent` and `assign_to_role` `{"agent", "result": "started"\|"typed_in", "worktree": bool}` (in a new worktree, answered once git has created it; a failed creation answers `refused` "couldn't create worktree: <e>", P3-D23); `stop_agent` `{"agent", "result": "stopped"}`; card tools `{"status": "pending", "card_id": N}`; a call queued while closed `{"status": "queued_until_open", "request": N}`. Error codes: `unknown_token`, `not_allowed`, `duohive_closed`, `interrupted`, `invalid_arguments`, `unknown_agent`, `unknown_role`, `unknown_message`, `refused` (by state or org rule), `limit` (with `"limit": "duplicate"\|"rate"\|"inbox_full"`), `invalid_change`, `no_screen`, `unavailable`. *Why:* TD §6.2 names results only for card tools. |
| P3-D17 | **`run.sock` framing.** One JSON object per line. A line with no `kind`, or `"kind": "hook"`, is a `HookMessage`, so `duohive hook` and `pending_hooks` don't change. `{"kind": "tools", "token"}` asks for the caller's tool list; `{"kind": "call", "token", "tool", "arguments"}` is one call. Each tool request uses its own connection; the answer is one line, `{"ok": <json>}` or `{"error": {"code", "message"}}`, then the endpoint closes it. Lines over `MAX_HOOK_LINE` (1 MiB) are dropped, as today. The helper parses no tool arguments; DuoHive does, with `tool::parse_call`, which ignores fields it doesn't know (models add stray ones, and identity never comes from arguments, so an injected `"from": "Duo"` has no effect) and refuses missing or ill-typed ones. The helper waits at most 30 s for an answer, and `WAIT_MAX_MINUTES` (60 min) + 30 s for `wait_for_agent`, without reading its arguments. Texts (`text`, `task`) are at most 16 000 characters (`MAX_TEXT`), else `invalid_arguments`. *Why:* P3-D9 fixes the socket but not the frames. |
| P3-D18 | **Message framing, recipients that aren't running, and "answered".** An agent's message is pasted as `[message from <sender> · DuoHive boundary <b>]`, newline, the text, newline, `[end of message <b> · reply with send_message to <sender>]`, where `<b>` is 12 lowercase hex characters from `Ids::boundary()`, minted again until it isn't a substring of the text. Your messages and card answers are pasted as written. An errored recipient counts as not running: an agent's message waits, shown `queued · errored`; yours starts it, as for stopped. "Answered" applies to agent → agent messages only: a delivered message from A to B becomes answered when B's `send_message` to A is accepted. A duplicate is the same sender, recipient and text (trimmed) as a message accepted in the last 10 minutes, whatever its status. *Why:* TD §6.3 names a boundary token but no format, and is silent on errored recipients and on answers to you. |
| P3-D19 | **Deliveries wait while you type into the agent.** While the agent pane is in Typing mode on an agent, the TUI sends `Command::Typing { agent: Some(..) }` (and `None` on leaving); the engine delivers nothing to that agent meanwhile, since a paste would join your half-typed line. *Why:* the inbox is the first thing that pastes into an agent you didn't act on. |
| P3-D20 | **Duo can propose moves; `OrgEdit::Move` is built here.** The PRD lets Duo propose to move a team, role or agent; the core has no move yet. Task 3.6 adds `OrgEdit::Move { node: NodeKey, to: MoveTarget }` with `MoveTarget::{Into(NodeKey), Before(NodeName), End}`: an agent moves `Into` a role; a role moves `Into` a team or the root (`NodeKey::Duo`); a team, which has no parent (teams don't nest), moves among the teams, `Before` another team or to the `End`. Every other pairing is refused, so a team onto a role stays invalid (UI §4.4.2's drag rule). On a team, Move to… lists positions (Task 3.6 edits UI §4.4.2 to say so). A moved running agent keeps its session; new inherited values apply at its next start. Phase 4's Task 4.8 builds the picker and drag on it. *Why:* a PRD Must in phase 3 needs it; the outline put the core move in 4.8. Approved by the author on 2026-10-18, with the team move by position. |
| P3-D21 | **Approving after the target changed.** Approving re-checks the card's change then: an org change runs through `preview_edit` and the core's own rules; a stop follows `stop_agent` by the target's state now (a finished target is stopped directly; an already stopped one is answered `approved, not applied (squad-1 is already stopped)`); a new-agent card adds the agent, then gives it the task. What can't apply is answered `approved, not applied (<reason>)` and changes nothing; the card is answered either way, never raised again. A clone card is applied as proposed even if an agent has become free meanwhile. *Why:* Duo keeps working while a card waits (P3-D2), so the org moves underneath it. |
| P3-D22 | **`e` on remove and move cards.** Like a stop card (P3-D7): `e` is disabled with `nothing to edit · y or n`. On add and edit cards it opens the team, role or agent form pre-filled with the proposed values. *Why:* UI §4.6 covers stop and new-agent cards only. |
| P3-D23 | **`read_agent` and `wait_for_agent`.** `read_agent` asks the runtime for one capture and answers with the next `Screen` for that agent, or `no_screen` after 5 s; a stopped agent is `refused`. `wait_for_agent` answers at once `at_rest` when the agent is finished or idle with nothing pending, else when its turn ends (`turn_ended`), when it stops or errors, or at the timeout. A wait on an agent already stopped or errored answers at once (`stopped`, `errored`). `start_agent` and `assign_to_role` into a new worktree answer when git answers: `Created` → `started` with `worktree: true`; a creation failure, including P2-D22's 120 s with no answer → `refused` "couldn't create worktree: <e>". A wait whose caller's token stops being current (Duo stopped or relaunched) is dropped without an answer. *Why:* TD §6.2 gives one line per tool. |
| P3-D24 | **Calls cut off by a close.** A call whose connection ends without an answer gets `interrupted` ("DuoHive closed before answering; the request may or may not have run") from the helper, and is never queued, since it may have run. While closed, the helper answers a Duo-only call from a token that isn't Duo's `not_allowed`, and an unknown token `unknown_token`, from the session row's `launched_as_duo` (P3-D30). Calls queued while closed are checked against the tokens of the session rows loaded at the next start-up, then run after `Reconnected` under the reserved `CallId(0)`, whose result is never published, each with its action line; an unknown token drops the call with a log line. `run.sock` is bound before the engine's start-up takes the queue, so a call either connects (and waits in the listen backlog) or was queued before the take. *Why:* Review Focus 5; TD §6.3 says only that requests wait. |
| P3-D25 | **Needs you card rows.** `NeedsYouItem` gains `card: Option<CardId>`. A card row has Duo's name, state `Waiting` (glyph `◆`), `since` = when raised, and `excerpt` = the summary; the TUI draws its word as `approval`. Cards order with agents by `since`. *Why:* UI §4.1.3 draws the row but the type has no card. |
| P3-D26 | **The chat's own lines.** Plain text to a stopped or errored Duo starts Duo with it as its task, as P2-D8 did. A `you` line shows `queued (Duo is <state word>)` in `fg.muted`, right-aligned, until its message is delivered. `/help` prints the chat keys and `/agents` every agent with its state cell, as local lines that aren't stored; any other `/word` says `unknown command /word · /help lists them`. `@name` for an unknown agent leaves the text in the input with the footer `no agent named <name>`. A dropped message of yours puts its text back in an empty input with `· enter to send again` (P2-D11's offer). *Why:* UI §4.7 doesn't say what commands print or how a queued line looks. |
| P3-D27 | **Tool arguments.** `start_agent {agent, task, run?}` (`run`: `"directory"` or `"worktree"`, default the agent's resolved default; a task is required); `assign_to_role {team?, role, task}`; `send_message {to, text}`; `message_status {message_id}` (only the caller's own messages; others are `unknown_message`); `read_agent {agent}`; `wait_for_agent {agent, timeout_minutes?}`; `stop_agent {agent}`; `propose_org_change {change}`, where `change` is one of the objects below. `fields` holds any of `cli` (`"claude"`\|`"opencode"`), `flags`, `directory`, `model`, `effort`, `worktree` (bool), `prompt`; an absent key inherits. A node is `{"agent": "<name>"}`, `{"team": "<name>"}`, `{"role": "<name>", "team": "<name>"\|null}` or `{"duo": true}` (the root, a move target only). `{"op": "add_team", "name": "Growth", "fields": {}}` → `AddTeam`; `{"op": "add_role", "team": "Payments"\|null, "name": "Frontend", "fields": {"directory": "~/code/web"}, "agents": 1}` → `AddRole` (`agents` 0–9, default 0); `{"op": "add_agent", "role": {"team": "Payments", "role": "Frontend"}, "name": "pay-web", "fields": {"cli": "claude"}}` → `AddAgent`; `{"op": "edit_team", "team": "Growth", "name": "Growth", "fields": {…}}`, `{"op": "edit_role", "role": {…}, "name": "…", "fields": {…}}` and `{"op": "edit_agent", "agent": "pay-web", "name": "pay-web", "fields": {…}}` → the matching edit, `fields` replacing the node's own fields; `{"op": "move", "node": {"agent": "pay-qa"}, "to": {"role": "QA", "team": "Payments"}}` → `Move { to: Into(..) }` (a role `Into` `{"team": …}` or `{"duo": true}`); `{"op": "move", "node": {"team": "Growth"}, "to": {"before_team": "Payments"}}` → `Move { to: Before(Payments) }`, and `"to": "end"` → `Move { to: End }` (teams only); `{"op": "remove", "node": {"role": "QA", "team": "Payments"}}` → `Delete`. Each `input_schema` is generated from these shapes (a `oneOf` on `op`) and tested against them. *Why:* the specs name tools, not arguments. |
| P3-D28 | **Picking a free agent.** "At rest longest" is the earliest `status.since` among the role's free agents; ties go to tree order. Give work's role variant may pick another free agent with `↑↓`; if it stopped being free, or is getting a worktree ready (P2-D20, which the TUI can't see), the engine refuses with `<agent> isn't free any more` (`Event::AssignRefused`) and the dialog stays open. *Why:* TD §5.4 has no tie rule. |
| P3-D29 | **The Give work role variant's Run row follows the picked agent.** A stopped or errored pick shows the Run radios, preset from its default; a finished or idle pick shows the next-turn line (`in <dir>, as the next turn of <agent>'s session`). UI §4.5's role mock draws Run radios beside a finished agent, which TD §5.1 and the PRD rule out; Task 3.11 edits the mock to drop the Run choices beside the finished agent. *Why:* a conflict between the UI mock and the PRD's Give work by state; the PRD decides what. Approved by the author on 2026-10-18. |
| P3-D30 | **Rights while DuoHive is closed come from the session row.** `SessionRow` gains `launched_as_duo` (migration 4), saved at each launch from `ResolvedAgent::is_duo`. The closed helper reads it through `duohive_store::closed`, never `org.toml` (whose `[duo] name` may differ from the running session's, and whose path the launch environment doesn't carry: only `DUOHIVE_DATA_DIR` is set). *Why:* the review found `org.toml` unreliable for this. |

---

## Tasks

### Task 3.1: The MCP endpoint and the shared types

**Files:**
- Create `crates/duohive-core/src/{tool.rs, inbox.rs, card.rs, chat.rs}`; modify `lib.rs`.
- Modify `crates/duohive-core/src/{command.rs, event.rs}`; `engine/mod.rs` (dispatch to a new `engine/tools.rs`, which answers every call `Unavailable` for now; the other new commands are rejected `not built yet`).
- Every `AgentView {` site (gains `inbox`): `crates/duohive-core/src/{event.rs, engine/mod.rs}`, `crates/duohive-tui/src/{fixtures.rs, model.rs, forms/tests/mod.rs}`.
- Every `Snapshot {` site (gains `cards`, `chat`, `your_queued`): `crates/duohive-core/src/{event.rs, engine/mod.rs, engine/reconnect/tests.rs}`, `crates/duohive-tui/src/{live.rs, model.rs, app/tests.rs, input/pane_tests.rs, render/pane_tests.rs}`, `crates/duohive/tests/{system.rs, e2e.rs}`.
- Every `NeedsYouItem {` site (gains `card`): `crates/duohive-core/src/{event.rs, engine/timers.rs, engine/timers/tests.rs}`, `crates/duohive-tui/src/{fixtures.rs, model.rs, input/pane_tests.rs, render/pane_tests.rs, render/zoom_tests.rs}`.
- Create `crates/duohive-mcp/src/{protocol.rs, endpoint.rs, calls.rs, tools.rs}`; modify `lib.rs` and `Cargo.toml` (`tokio`, `serde`, `serde_json`, `thiserror`, `tracing`).
- Modify `crates/duohive/src/hook_listener.rs` (route by `kind`; `serve` and `serve_until` take a `Calls`) and its callers `crates/duohive/src/system.rs` and `crates/duohive/tests/{hook.rs, system.rs}`; `system.rs`'s `forward_events` resolves `ToolResult` into `Calls` and doesn't pass it to the app.
- Docs: `docs/plans/bench-results.md` (P3-D8's two sizes); TD §3.2's lists gain `Tool`, `RejectCard` and `Typing`, and `ToolResult`, `MessageDropped`, `MessageRefused`, `InboxChanged`, `RoleAssigned` and `AssignRefused` (`ApproveCard` and `ProposeOrgChange` are already named; the latter arrives as a `Tool` call).

**Interfaces — Produces:**

```rust
// tool.rs
pub struct CallId(pub u64);
impl CallId { pub const REPLAY: CallId = CallId(0); }          // calls queued while closed (P3-D24); never published, never resolved
pub enum Rights { Agent, Duo }
pub enum ToolName { ListAgents, SendMessage, MessageStatus, ReadAgent, WaitForAgent,
                    StartAgent, StopAgent, AssignToRole, ProposeOrgChange }
impl ToolName { pub const ALL: [ToolName; 9]; pub fn as_str(self) -> &'static str;   // "list_agents", …
                pub fn parse(s: &str) -> Option<Self>; pub fn duo_only(self) -> bool; pub fn changes(self) -> bool }
pub fn tools_for(rights: Rights) -> Vec<ToolName>;
pub const MAX_TEXT: usize = 16_000;
pub const WAIT_DEFAULT_MINUTES: u32 = 10; pub const WAIT_MAX_MINUTES: u32 = 60;
pub enum ToolCall {
    Rights,                                                    // what tools/list shows; not a tool, no action line
    ListAgents,
    SendMessage { to: AgentName, text: String },
    MessageStatus { id: MessageId },
    ReadAgent { agent: AgentName },
    WaitForAgent { agent: AgentName, timeout_minutes: u32 },
    StartAgent { agent: AgentName, task: String, run: Option<RunIn> },
    StopAgent { agent: AgentName },
    AssignToRole { role: RoleKey, task: String },
    ProposeOrgChange { change: OrgEdit },
}
pub fn parse_call(tool: &str, args: &serde_json::Value) -> Result<ToolCall, ToolError>;   // P3-D17, P3-D27
pub struct AgentEntry { pub name: AgentName, pub team: Option<NodeName>, pub role: Option<NodeName>,
                        pub state: AgentState, pub quiet: bool, pub from_screen: bool, pub for_secs: u64,
                        pub last_reply: Option<String>, pub waiting_messages: u32, pub cli: CliKind,
                        pub worktree: Option<TaskId>, pub free: bool }
pub enum Given { Started { worktree: bool }, TypedIn, CreatingWorktree }   // CreatingWorktree: the launch waits for git;
                                                                          // a tool answer waits for it (P3-D23), never shows it
pub enum SendOutcome { TypedIn, Queued(AgentState), Started }          // P3-D16; Queued carries the recipient's state
pub enum WaitOutcome { AtRest, TurnEnded, TimedOut, Stopped, Errored }
pub enum ToolOk {
    Rights(Rights), Agents { you: AgentName, agents: Vec<AgentEntry> },
    Queued { message: MessageId, outcome: SendOutcome },
    Status { message: MessageId, to: AgentName, status: MessageStatus, reason: Option<String> },   // reason: P3-D16
    Screen { agent: AgentName, state: AgentState, text: String },
    Waited { agent: AgentName, outcome: WaitOutcome, state: AgentState, last_reply: Option<String> },
    Given { agent: AgentName, given: Given }, Stopped { agent: AgentName }, Pending { card: CardId },
}
pub enum LimitKind { Duplicate, Rate, InboxFull }
pub enum ErrorCode { UnknownToken, NotAllowed, DuohiveClosed, Interrupted, InvalidArguments, UnknownAgent,
                     UnknownRole, UnknownMessage, Refused, Limit(LimitKind), InvalidChange, NoScreen, Unavailable }
pub struct ToolError { pub code: ErrorCode, pub message: String }
pub type ToolResult = Result<ToolOk, ToolError>;
// inbox.rs
pub struct MessageId(pub u64);
pub enum Sender { You, Agent(AgentName) }
pub enum MessageKind { Text, CardAnswer(CardId) }
pub enum MessageStatus { Queued, Delivered, Answered, Dropped { reason: String } }
pub struct Message { pub id: MessageId, pub from: Sender, pub to: AgentName, pub kind: MessageKind, pub text: String,
                     pub status: MessageStatus, pub queued_at: Millis, pub delivered_at: Option<Millis> }
// card.rs
pub struct CardId(pub u64);
pub enum CardSource { Duo, NoFreeAgent }                       // both shown as Duo's (P3-D3)
pub enum CardChange { Org(OrgEdit), Stop { agent: AgentName },
                      NewAgent { role: RoleKey, agent: AgentDraft, task: String, run: RunIn, clone_of: Option<AgentName> } }
pub enum CardOutcome { Approved, EditedApproved, Rejected, NotApplied(String) }
pub struct Card { pub id: CardId, pub source: CardSource, pub change: CardChange, pub raised_at: Millis,
                  pub answer: Option<(CardOutcome, Millis)> }
// chat.rs
pub struct ChatId(pub u64);
pub enum ChatKind { You { message: Option<MessageId> }, Duo,
                    Action { result: String }, CardResult { card: CardId, outcome: CardOutcome }, Note }   // Note: TUI-only, never stored
pub struct ChatEntry { pub id: ChatId, pub at: Millis, pub kind: ChatKind, pub text: String }
// command.rs
Command::Tool { call_id: CallId, token: Token, call: ToolCall }
Command::SendMessage { to: AgentName, text: String }          // from you
Command::ChatToDuo { text: String }
Command::AssignToRole { role: RoleKey, task: String, pick: Option<AgentName>, run: Option<RunIn> }   // from you
Command::ApproveCard { card: CardId, edited: Option<CardChange> }
Command::RejectCard { card: CardId }
Command::Typing { agent: Option<AgentName> }                  // P3-D19
// event.rs
Event::ToolResult { call_id: CallId, result: ToolResult }
Event::MessageQueued { id: MessageId, from: Sender, to: AgentName }
Event::MessageDelivered { id: MessageId }
Event::MessageDropped { id: MessageId, from: Sender, to: AgentName, text: String, reason: String }
Event::InboxChanged { agent: AgentName, waiting: u32 }
Event::CardRaised(Card)
Event::CardAnswered { card: CardId, outcome: CardOutcome }
Event::ChatLine(ChatEntry)
Event::MessageRefused { to: AgentName, text: String, reason: String }      // your SendMessage refused (unknown agent, empty text)
Event::RoleAssigned { role: RoleKey, task: String, outcome: RoleOutcome }  // your AssignToRole
Event::AssignRefused { role: RoleKey, task: String, reason: String }      // "<agent> isn't free any more", unknown role
pub enum RoleOutcome { Given { agent: AgentName, given: Given }, Card(CardId) }
// AgentView gains `inbox: u32`; Snapshot gains `cards: Vec<Card>` (pending), `chat: Vec<ChatEntry>` (last 500)
// and `your_queued: Vec<MessageId>` (your messages not yet delivered);
// NeedsYouItem gains `card: Option<CardId>` (P3-D25). All empty or 0 until their tasks.
```

`duohive-mcp`:

```rust
// protocol.rs (P3-D17)
pub enum SockLine { Hook(HookMessage), Tools { token: String }, Call { token: String, tool: String, arguments: serde_json::Value } }
pub fn parse_line(line: &str) -> Result<SockLine, String>;     // no `kind` or "hook" → Hook
pub enum SockAnswer { Ok(serde_json::Value), Error { code: String, message: String } }
// calls.rs
#[derive(Clone)] pub struct Calls { /* Arc<Mutex<HashMap<CallId, oneshot::Sender<ToolResult>>>>, next id */ }
impl Calls { pub fn new() -> Self; pub fn open(&self) -> (CallId, oneshot::Receiver<ToolResult>);
             pub fn resolve(&self, id: CallId, result: ToolResult); pub fn forget(&self, id: CallId) }
// endpoint.rs
pub async fn serve_request(line: SockLine, stream: tokio::net::UnixStream, to_core: mpsc::Sender<Input>, calls: Calls);
// tools.rs
pub struct ToolSpec { pub name: &'static str, pub description: &'static str, pub input_schema: serde_json::Value }
pub fn specs(rights: Rights) -> Vec<ToolSpec>;
pub fn render(result: &ToolResult) -> SockAnswer;              // P3-D16 shapes
```

**Behaviour:**
- **`Tools`:** the endpoint opens a call, sends `Command::Tool { call: ToolCall::Rights }`, and answers `{"tools": specs(rights)}`; an error answers its error.
- **`Call`:** `parse_call` first (an error answers at once, without the core); then `Command::Tool`, await, `render`, write one line, close. A connection that closes early `forget`s its call; a late `ToolResult` for a forgotten id is dropped.
- **Ids:** `CallId`s count up from 1 per run and are never reused, so a result never reaches another call. `CallId(0)` is reserved for replayed calls: `Calls` never hands it out and `resolve` ignores it.

- [ ] **Step 1: Write the failing tests.**
  - Core (`tool.rs`):
    - `tool_names_round_trip`: `ToolName::parse(t.as_str()) == Some(t)` for all 9; `duo_only` is true for exactly `read_agent`, `wait_for_agent`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change`; `changes` is true for exactly the five of P3-D10.
    - `tools_for_agent_lacks_duo_tools`: `tools_for(Rights::Agent)` is `[ListAgents, SendMessage, MessageStatus]`; `tools_for(Rights::Duo)` is all 9.
    - `parse_call_accepts_each_tool`: one valid argument object per tool parses to the matching `ToolCall`; `run: "worktree"` → `Some(RunIn::NewWorktree)`; `wait_for_agent` without `timeout_minutes` → 10.
    - `parse_call_ignores_unknown_fields`: `send_message {to, text, "from": "Duo", "agent": "Duo"}` parses to `SendMessage { to, text }` (P3-D17).
    - `propose_change_shapes_parse`: each of P3-D27's nine example `change` objects parses to its `OrgEdit`; `{"duo": true}` is accepted only as a move target, and `before_team` and `"end"` only with a team node.
    - `parse_call_refuses_bad_arguments`: a missing `to`, an agent name `"../x"`, a text of 16 001 characters, `timeout_minutes: 61`, and an unknown `change.op` each give `InvalidArguments` with the field named; an unknown tool gives `InvalidArguments("unknown tool <name>")`.
    - `new_types_round_trip_json`: every new `Command` and `Event` variant, `ToolResult` of each `ToolOk` and each `ErrorCode`, equal after `serde_json`.
  - `duohive-mcp`:
    - `kindless_line_is_a_hook`: a phase-2 `HookMessage` line parses to `SockLine::Hook`; `{"kind":"hook",…}` too; `{"kind":"call",…}` to `Call`.
    - `specs_match_rights`: `specs(Rights::Agent)` names exactly the three every-agent tools; every spec's `input_schema` is an object schema whose `required` lists the P3-D27 required fields, and each of P3-D27's nine `change` examples validates against `propose_org_change`'s schema.
    - `render_matches_p3_d16`: each `ToolOk` renders the P3-D16 JSON (for example `Pending { card: CardId(3) }` → `{"status":"pending","card_id":3}`); each `ErrorCode` renders `{"error":{"code":"<snake>","message":…}}`, and `Limit(Rate)` adds `"limit":"rate"`.
    - With a fake core task that answers `Command::Tool` from a table: `endpoint_answers_unknown_token_error`, `tools_list_for_non_duo_lacks_duo_tools`, `bad_arguments_answered_without_core` (the fake core sees no command), `result_reaches_only_its_call` (two concurrent calls answered in reverse order each get their own result), `early_close_forgets_call` (the stream is dropped; a later `resolve` neither panics nor answers anyone), `replay_id_never_issued` (`open` never returns `CallId(0)`; `resolve(CallId(0), …)` answers nobody).
  - `duohive`: `listener_routes_by_kind`: on a temp `run.sock`, a hook line still reaches `Input::Hook`, and a `call` line reaches `Input::Command(Command::Tool { .. })`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features tool; cargo test -p duohive-mcp; cargo test -p duohive --lib hook_listener`
- [ ] **Step 3: Implement** the types, the endpoint, the listener routing and the `forward_events` change. The engine's `engine/tools.rs` answers `Err(ToolError { code: Unavailable, message: "not built yet" })` for now.
- [ ] **Step 4: Measure P3-D8.** Add `rmcp = "=<latest>"` to `duohive-mcp` and a stub use, `cargo build --release -p duohive`, and record the binary size with and without it under "Phase 3" in `bench-results.md`. Under 10 MB and passing `cargo deny check`: keep it. Else remove it and note the fallback; Task 3.2 writes the hand-written server.
- [ ] **Step 5: Run, see PASS.** `make check`
- [ ] **Step 6: Edit TD §3.2**, then commit. `feat: mcp endpoint, tool commands and phase 3 shared types`

### Task 3.2: The `duohive mcp` helper, per-launch MCP config and the closed queue

**Files:**
- Create `crates/duohive-mcp/src/helper.rs` (and `rpc.rs` only for P3-D8's fallback).
- Create `crates/duohive/src/mcp_cmd.rs` (`StoreFallback`, which implements `duohive_mcp::helper::ClosedFallback` over `duohive_store::closed`: only `duohive` depends on both); modify `args.rs` (`Cli::Mcp`, `USAGE`), `main.rs`, `lib.rs`.
- Create `crates/duohive-store/src/closed.rs` (plain functions; it knows nothing of `duohive-mcp`); modify `db.rs` (migration 4), `sqlite_store.rs`, `lib.rs`.
- Modify `crates/duohive-core/src/ports.rs` (`Store::take_queued_tool_calls`, `QueuedToolCall`; `SessionRow` gains `launched_as_duo`, P3-D30), `testkit.rs` (`FakeStore`), `crates/duohive/src/system/enriching_store.rs`, `engine/mod.rs` and `engine/lifecycle.rs` (`AgentRt.launched_as_duo`, set at launch from `ResolvedAgent::is_duo` and saved on the row), `engine/reconnect.rs` (restored from the row).
- Every `SessionRow {` site: `crates/duohive-core/src/{ports.rs, engine/mod.rs, engine/reconnect/tests.rs, engine/org_edit/tests.rs}` and `crates/duohive-store/{src/sqlite_store.rs, tests/state_db.rs}`. The engine's replay of queued calls is built in Task 3.4, where its first changing tool (`send_message`) exists to observe it.
- Modify `crates/duohive-cli/src/{claude.rs, opencode.rs, opencode_tests.rs, assets/opencode-plugin/index.js, opencode_plugin_tests.rs}` and their launch snapshots. Two existing tests change: `claude.rs`'s `no_auto_approve_flags` keeps every banned flag, and its "no `permissions` key" assertion becomes "`permissions` is exactly `{\"allow\": [\"mcp__duohive__*\"]}`"; `opencode_tests.rs`'s `no_auto_approve_flags` keeps banning `permission`, `permissions`, `agents` and `agent` (OpenCode's approvals stay in the plugin's `evaluate` hook) and also asserts the only `mcp` server is `duohive`; `launch_files_stay_in_run_dir`'s file count follows any new file.
- Create `crates/duohive/tests/mcp.rs`.
- Docs: TD §5.2 (MCP column, as measured), §6.2 ("Pre-approved", as measured), §6.3 (P3-D10, P3-D24), §7 (`run.sock` framing P3-D17, `pending_tool_calls`); `spike-report.md` (an S1.5 addendum and an S2.14 addendum for the live checks); `bench-results.md` (the helper's memory and start time).

**Interfaces:**
- Consumes `SockLine`, `SockAnswer`, `tools::specs`, `ToolName::changes` (Task 3.1).
- Produces:

```rust
// duohive-mcp/src/helper.rs
pub trait ClosedFallback: Send + Sync {
    fn rights(&self, token: &str) -> Option<Rights>;                          // the session row's launched_as_duo (P3-D30)
    fn queue(&self, token: &str, tool: &str, arguments: &serde_json::Value) -> Result<u64, String>;
}
pub struct HelperConfig { pub sock: PathBuf, pub token: Option<String>, pub connect_timeout: Duration,   // 200 ms
                          pub answer_timeout: Duration }       // 30 s; wait_for_agent: WAIT_MAX_MINUTES (60 min) + 30 s, fixed (P3-D17)
pub async fn run_helper(cfg: HelperConfig, stdin: impl AsyncRead + Unpin, stdout: impl AsyncWrite + Unpin,
                        closed: Arc<dyn ClosedFallback>) -> io::Result<()>;   // until stdin ends
// duohive-core/src/ports.rs
pub struct QueuedToolCall { pub id: u64, pub token: Token, pub tool: String, pub arguments: serde_json::Value, pub queued_at: Millis }
fn take_queued_tool_calls(&mut self) -> Result<Vec<QueuedToolCall>, StoreError>;   // oldest first; deletes them
// duohive-store/src/closed.rs (plain functions; own constant-time compare, since core's `same_token` is crate-private)
pub fn rights_for_token(paths: &Paths, token: &str) -> Result<Option<Rights>, StoreError>;
pub fn queue_tool_call(paths: &Paths, token: &str, tool: &str, arguments: &serde_json::Value, at: Millis) -> Result<u64, StoreError>;
// duohive/src/mcp_cmd.rs
pub struct StoreFallback { paths: Paths }                      // implements duohive_mcp::helper::ClosedFallback
// duohive/src/args.rs: Cli::Mcp; `duohive mcp` serves MCP on stdin/stdout for the agent whose DUOHIVE_TOKEN it has
```

**Migration 4:**

```sql
CREATE TABLE pending_tool_calls(id INTEGER PRIMARY KEY AUTOINCREMENT, token TEXT NOT NULL, tool TEXT NOT NULL,
                                arguments TEXT NOT NULL, queued_at INTEGER NOT NULL);
ALTER TABLE sessions ADD COLUMN launched_as_duo INTEGER NOT NULL DEFAULT 0;
```

**Per-launch config:**

| CLI | Added to each launch |
|---|---|
| Claude Code | `<run>/mcp.json` = `{"mcpServers": {"duohive": {"type": "stdio", "command": "<hook_exe>", "args": ["mcp"]}}}`, passed as `--mcp-config <run>/mcp.json` (no `--strict-mcp-config`: the user's servers stay); `settings.json` gains `"permissions": {"allow": ["mcp__duohive__*"]}` |
| OpenCode | `opencode.json` gains a `duohive` local MCP server running `<hook_exe> mcp`. The shape `"mcp": {"duohive": {"type": "local", "command": ["<hook_exe>", "mcp"], "enabled": true}}` is 1.18 evidence (S2.5) and provisional: 2.0 renamed several keys (S2.14), so Step 5 finds the 2.0 key and shape, and the adapter test and snapshot are updated to it there; the plugin's `evaluate` hook sets `effect` to `allow` for a request on a `duohive` server tool (the exact `action`/resource match is Step 5's finding) and changes nothing else |
| Fake (`bench-agent`) | nothing: it has `DUOHIVE_TOKEN` and `DUOHIVE_DATA_DIR` like every launch, and its scripted mode runs `duohive mcp` itself (Task 3.12) |

**Helper behaviour:**
- MCP over stdio, newline-delimited JSON-RPC 2.0. `initialize` answers with the server name `duohive` and the `tools` capability; `tools/list` sends `Tools` and lists the answer; `tools/call` sends `Call`.
- **DuoHive closed** (connecting fails within 200 ms): `tools/list` lists `specs(closed.rights(token))` (nothing when the token is unknown); an unknown token answers `unknown_token`; a Duo-only call from a token without Duo's rights answers `not_allowed`; a changing call is queued with `closed.queue` and answered `{"status":"queued_until_open","request":N}`; a read answers `duohive_closed` "DuoHive is closed".
- **Cut off** (the connection ends, or the answer timeout passes, with no answer line): `interrupted` (P3-D24), never queued.
- **No `DUOHIVE_TOKEN`:** lists nothing and answers every call `unknown_token`.
- `rights_for_token` finds the session row whose token matches (constant-time) and gives `Rights::Duo` when its `launched_as_duo` is set (P3-D30); `queue_tool_call` inserts with `open_db_for_hook`'s 200 ms budget.
- **On open** (built in Task 3.4): the queued calls run as P3-D24 says.

- [ ] **Step 1: Write the failing tests.**
  - `helper.rs`, with an in-memory stdin/stdout and a fake socket server on a temp path:
    - `initialize_then_lists_tools_by_rights`: a fake DuoHive answering `Rights::Agent` → `tools/list` has 3 tools; `Rights::Duo` → 9.
    - `call_relays_arguments_untouched`: the fake server receives `{"kind":"call","token":"tok-1","tool":"send_message","arguments":{…}}` byte-equal in its `arguments`, and the MCP answer's `structuredContent` is the server's `ok` JSON.
    - `closed_reads_answer_closed`: no socket → `list_agents`, `message_status`, `read_agent` and `wait_for_agent` each give `isError: true`, code `duohive_closed`, message `DuoHive is closed`; the fallback's `queue` is never called.
    - `closed_changes_queue_and_answer_queued`: no socket → each of the five changing tools calls `queue` once and answers `{"status":"queued_until_open","request":N}`.
    - `closed_mid_call_answers_interrupted_not_queued`: the fake server reads the call and closes without answering → code `interrupted`; `queue` is never called.
    - `closed_unknown_token_and_rights`: no socket, a token the fallback doesn't know → `unknown_token` for every call; an agent's token calling `stop_agent` → `not_allowed`, not queued.
    - `wait_answer_timeout_is_fixed`: a `wait_for_agent` call is given 60 min 30 s whatever its `timeout_minutes`; other calls 30 s.
    - `no_token_lists_nothing`.
  - `duohive-store`: `queued_tool_calls_round_trip` (insert through `queue_tool_call`, read back through `take_queued_tool_calls`, oldest first, then empty); `closed_rights_from_launched_as_duo` (a row with `launched_as_duo` → `Duo`, without → `Agent`, none → `None`); `session_launched_as_duo_round_trips`; `migration_4_applies_on_v3_db`.
  - Engine: `launched_as_duo_saved_and_restored`: Duo's launch saves the flag; a new engine restores it from the row.
  - Adapters:
    - `launch_carries_mcp_config` (Claude): `--mcp-config <run>/mcp.json` is in the args, and the file names `<hook_exe>` with `["mcp"]`.
    - `launch_allows_only_duohive_tools` (both CLIs): Claude's `settings.json` `permissions.allow` is exactly `["mcp__duohive__*"]` and has no other `permissions` key; OpenCode's config has exactly one MCP server, `duohive`, in the provisional shape until Step 5; every file the launch writes is under `<run>`.
    - The two `no_auto_approve_flags` tests, changed as Files says.
    - `evaluate_allows_only_duohive_tools` (plugin, under Node, with the provisional match until Step 5): a `duohive` tool request with `effect: "ask"` becomes `allow`; one the user set to `deny` stays `deny`; another server's tool, `edit` and `shell` keep their effects, and `edit` / `shell` `allow` still becomes `ask`.
    - Launch snapshots updated for both CLIs.
  - `crates/duohive/tests/mcp.rs`: `e2e_fake_cli_lists_tools_through_helper`: a running system with one fake agent; spawn `CARGO_BIN_EXE_duohive mcp` with that agent's `DUOHIVE_TOKEN` and `DUOHIVE_DATA_DIR` (`Command::env`), send `initialize` and `tools/list`, and get the 3 every-agent tools; with Duo's token, 9. `helper_exits_on_stdin_end`: closing stdin ends it with status 0 within 1 s.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-mcp helper; cargo test -p duohive-store; cargo test -p duohive-cli; cargo test -p duohive --test mcp`
- [ ] **Step 3: Implement.** With P3-D8's fallback, `rpc.rs` handles the five methods, echoes the client's `protocolVersion` when it is one it knows, else answers `2025-06-18`.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Live checks (P3-D1).** On the author's machine, with a throwaway org under a temp `DUOHIVE_DATA_DIR` and `DUOHIVE_CONFIG_DIR`, never the user's global CLI config:
  1. **Claude Code (pinned 2.1):** launch an agent through DuoHive and ask it to call `list_agents`. Record: the tool's name as Claude shows it; whether `mcp__duohive__*` in `permissions.allow` runs it with no prompt; that a file edit in the same session still prompts; that the helper received `DUOHIVE_TOKEN`; that `~/.claude.json` and `~/.claude/settings.json` are byte-equal before and after.
  2. **OpenCode 2.0 (`--standalone`):** the same. Record: whether the per-launch `mcp` server loads and lists its tools; the tools' names as the model sees them (S2.5 had `duohive_ping` on 1.18); the `action` and `resources` the `evaluate` hook receives for a `duohive` tool; that the tool runs with no prompt and an `edit` still asks; that the helper received `DUOHIVE_TOKEN`; that the global config is unchanged.
  3. **Long calls:** on each CLI, a `wait_for_agent` with `timeout_minutes: 60` on an agent kept working for longer. Record whether the CLI's own MCP request timeout ends the call first, and when. If it does, set that timeout per launch where the CLI has a per-launch setting (an environment variable or config key in the run directory, never global), and check again; with no per-launch setting, stop and ask the author whether to lower `WAIT_MAX_MINUTES` (P3-D11) below the CLI's limit.
  4. **The helper's cost:** RSS and start-to-`initialize` time of one `duohive mcp`, in `bench-results.md` under "Phase 3", with the total for 10 agents.

  Record 1–3 as the S1.5 and S2.14 addenda in `spike-report.md`. Then set the OpenCode `mcp` key and shape and the plugin's match to what 2 found, update `launch_allows_only_duohive_tools`, `evaluate_allows_only_duohive_tools` and the OpenCode snapshot to them, run `make check`, and edit TD §5.2 and §6.2 to the measured names and mechanism. **If a check fails** (a DuoHive tool still prompts, an edit stops prompting, the token doesn't reach the helper, the MCP server doesn't load, or any global file changes): stop and ask the author before going on. A fallback such as listing each tool name, or passing the token through the CLI's own environment syntax, is only tried with the author's say-so, then checked again.
- [ ] **Step 6: Edit the TD** (§5.2, §6.2, §6.3 with P3-D10 and P3-D24, §7 with P3-D17, P3-D30, the `pending_tool_calls` table and `sessions.launched_as_duo`).
- [ ] **Step 7: Commit.** `feat: duohive mcp helper, per-launch mcp config and the closed queue`

### Task 3.3: Tool rights, identity and the shared give path

The token check and rights belong here, as the outline has them. The shared give path is added here too, because Tasks 3.4, 3.7 and 3.5 all hand work over through it; one refactor before them avoids three.

**Files:**
- Create `crates/duohive-core/src/engine/rights.rs`.
- Modify `engine/lifecycle.rs` (`give`; `give_work` calls it), `engine/tools.rs` (`Rights` and the rights check before any call), `engine/signals.rs` (`same_token` becomes `pub(crate)`).

**Interfaces — Produces:**

```rust
pub(super) struct Caller { pub agent: AgentName, pub rights: Rights }
impl Engine {
    pub(super) fn caller(&self, token: &Token) -> Result<Caller, ToolError>;   // UnknownToken, NotAllowed via `allowed`
    pub(super) fn allowed(caller: &Caller, tool: ToolName) -> Result<(), ToolError>;
    /// Give work by state (TD §5.1), the one path for GiveWork, start_agent, assign_to_role and your messages.
    /// A new worktree gives `Given::CreatingWorktree`; the launch follows git's `Created` as in phase 2.
    pub(super) fn give(&mut self, agent: &AgentName, task: String, run: RunIn, out: &mut Vec<Event>) -> Result<Given, GiveRefusal>;
}
pub(super) enum GiveRefusal { Unknown, Busy(AgentState), Pending, GettingReady, EmptyTask, Launch(String) }
// Uses AgentRt.launched_as_duo from Task 3.2.
```

**Rules:**
- A token resolves only to an agent whose current launch holds it (`rt.token`, compared with `same_token`); a stopped agent has none.
- `Rights::Duo` only when `launched_as_duo`; renaming another agent can't give it.
- `ToolCall::Rights` needs only a known token; every other call is checked with `allowed` first, so a refused call has no other effect.
- `give_work`'s events and refusals stay exactly as in phase 2: its tests pass unchanged.

- [ ] **Step 1: Write the failing tests** (engine harness).
  - `agent_cannot_call_duo_tools`: with `squad-1`'s token, each of the six Duo tools → `NotAllowed`, with no runtime call, card or message; `Rights` → `Rights::Agent`.
  - `duo_token_has_duo_rights`: Duo's token → `Rights::Duo`.
  - `token_of_stopped_agent_rejected`: stop `squad-1`, then a call with its last token → `UnknownToken`.
  - `token_of_previous_launch_rejected`: stop and start `squad-1` again; the first launch's token → `UnknownToken`, the new one works.
  - `name_in_payload_ignored`: with `squad-1`'s token, `read_agent {"agent": "pay-qa", "from": "Duo", "caller": "Duo"}` and `stop_agent {"agent": "pay-qa", "from": "Duo"}` are `NotAllowed`, and `Rights` is still `Rights::Agent` (the extra fields are ignored, P3-D17; Task 3.4's `send_message_sender_is_token_not_payload` checks the sender).
  - `rename_never_grants_duo_rights`: an agent edited while running keeps `Rights::Agent`.
  - `give_returns_how_it_was_given`: a stopped agent → `Started { worktree: false }`; with `RunIn::NewWorktree` → `CreatingWorktree`; idle → `TypedIn`; working → `Busy(Working)`; pending delivery → `Pending`.
  - Phase 2's `lifecycle` tests pass unchanged.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features rights lifecycle`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: token-checked tool rights and one give path`

### Task 3.4: The inbox

**Files:**
- Modify `crates/duohive-core/src/inbox.rs` (limits, `frame`), create `engine/inbox.rs`.
- Modify `engine/mod.rs` (`Delivery` gains `message: Option<MessageId>`; `AgentView.inbox`), `engine/lifecycle.rs` (`not_taken` drops a message delivery instead of publishing `GiveWorkNotTaken`), `engine/signals.rs` (delivery on entering rest), `engine/reconnect.rs` (load at start-up), `engine/tools.rs` (`send_message`, `message_status`).
- Modify `ports.rs` (`Ids::boundary`, `Store` message methods), `testkit.rs` (`FakeIds`, `FakeStore`), `crates/duohive/src/real_ports.rs` (`RandomIds::boundary`), `crates/duohive/src/system/enriching_store.rs`.
- Modify `crates/duohive/src/system.rs`: say in a comment and keep the order that binds `run.sock` before the engine's `start_up` takes the closed queue (P3-D24); `crates/duohive/tests/mcp.rs` gains the test.
- Modify `crates/duohive-store/src/{db.rs, sqlite_store.rs}` (migration 5).
- Docs: TD §6.3 (P3-D18, P3-D19) and §7 (the messages table).

**Interfaces — Produces:**

```rust
// inbox.rs
pub const DUPLICATE_WINDOW: Duration = Duration::from_secs(600);
pub const RATE_WINDOW: Duration = Duration::from_secs(60);
pub const RATE_LIMIT: usize = 5;
pub const INBOX_LIMIT: usize = 20;
pub fn check_limits(recent: &[Message], from: &AgentName, to: &AgentName, text: &str, now: Millis) -> Result<(), LimitKind>;
pub fn frame(from: &AgentName, boundary: &str, text: &str) -> String;     // P3-D18
// ports.rs
trait Ids { …; fn boundary(&mut self) -> String; }                         // 12 lowercase hex; FakeIds: "b00000000001", …
fn load_messages(&mut self, since: Millis) -> Result<Vec<Message>, StoreError>;   // queued, delivered, or queued_at ≥ since
fn find_message(&mut self, id: MessageId) -> Result<Option<Message>, StoreError>;
fn save_message(&mut self, m: &Message) -> Result<(), StoreError>;
// engine/inbox.rs
impl Engine {
    pub(super) fn send(&mut self, from: Sender, to: &AgentName, kind: MessageKind, text: String, out: &mut Vec<Event>)
        -> Result<(MessageId, SendOutcome), ToolError>;
    pub(super) fn deliver_next(&mut self, agent: &AgentName, out: &mut Vec<Event>);   // called whenever `agent` may take one
}
```

**Migration 5:**

```sql
CREATE TABLE messages(id INTEGER PRIMARY KEY, sender TEXT, recipient TEXT NOT NULL, kind TEXT NOT NULL, card INTEGER,
                      text TEXT NOT NULL, status TEXT NOT NULL, reason TEXT, queued_at INTEGER NOT NULL, delivered_at INTEGER);
-- sender NULL = you; kind 'text' | 'card'; status 'queued' | 'delivered' | 'answered' | 'dropped'
```

**Rules:**
- **Accepting** (`send`): an unknown recipient → `UnknownAgent`; an agent sender runs `check_limits` (`Limit(Duplicate|Rate|InboxFull)`); you never do. The message is saved `queued`, then `MessageQueued` and `InboxChanged` are published, and an accepted B → A message turns A → B's delivered messages to `answered`.
- **Your text message to a stopped or errored agent:** `give(agent, text, resolved default run)` (`SendOutcome::Started`); the message is `delivered` once the launch takes it. Card answers and agents' messages wait (`Queued(Stopped)` or `Queued(Errored)`). A recipient at rest with nothing pending gets the paste at once (`TypedIn`); otherwise `Queued(<its state>)`.
- **Your `Command::SendMessage` refused** (unknown agent, empty text) → `Event::MessageRefused { to, text, reason }` (`no agent named <x>`, `the message is empty`). `Snapshot.your_queued` lists your messages not yet delivered.
- **When the recipient may take one** (at rest, no delivery pending, not being typed into, no worktree being created): `deliver_next` pastes the oldest queued message (framed for an agent sender, verbatim otherwise) through `type_next_turn`'s path, with `Delivery.message` set. Called on: accepting, entering finished or idle, a delivery ending, `Typing { agent: None }`, and after `Reconnected`.
- **Taken** (P2-D11's sign) → `delivered`, `MessageDelivered`. **Not taken or `PasteFailed`** → `dropped` with the reason, `MessageDropped`; never pasted again. `GiveWorkNotTaken` is not published for a message.
- **`InboxChanged`** carries the count of queued messages for the agent (`✉ N`).
- **`message_status`** answers only the caller's own messages, from memory or `find_message`, with P3-D16's `reason` (for a queued one, `<to> is stopped`, `is errored` or `is <state word>`).
- **`send_message`** is dispatched here: `ToolOk::Queued { message, outcome }`.
- **Start-up:** `load_messages(now - 10 min)` restores queues and limits; `next` ids continue from the highest.
- **Calls queued while closed** (P3-D10, P3-D24): `start_up` takes them (`take_queued_tool_calls`) and resolves each token against the session rows it loaded; after `Reconnected` they run in order through `engine/tools.rs`, with no caller to answer; an unknown token or bad arguments drop the call with a log line. Each tool that later tasks build runs this way with no further change.

- [ ] **Step 1: Write the failing tests** (engine harness, `FakeIds`, `FakeStore`).
  - Delivery:
    - `delivered_only_at_rest`: a message to a working agent stays `queued` with no paste; on its turn end (finished) it is pasted once.
    - `never_typed_onto_waiting`: a waiting agent with 2 queued messages gets no paste; waiting → working → finished pastes only the first.
    - `starting_agent_waits_for_rest`: a message to a starting agent is pasted after its first rest, never before.
    - `never_typed_while_delivery_pending`: two messages to an idle agent → one paste; the second only after the first is taken and the agent rests again.
    - `never_typed_while_you_type`: after `Typing { agent: Some(a) }` nothing is pasted to `a`; `Typing { agent: None }` pastes the oldest.
    - `delivered_when_taken`: a `busy` hook after the paste → `MessageDelivered`, status `delivered`.
    - `untaken_message_dropped_never_retried`: no sign within 5 s → `dropped` with `no sign it started`; the agent rests again and nothing more is pasted; no `GiveWorkNotTaken` is published.
    - `paste_failure_drops_with_reason`: `PasteFailed("a dialog stayed open")` → `dropped` with that reason.
  - Framing:
    - `agent_text_framed_with_fresh_boundary`: the pasted text from `squad-1` is exactly `[message from squad-1 · DuoHive boundary b00000000001]\nrun the tests\n[end of message b00000000001 · reply with send_message to squad-1]`.
    - `boundary_never_in_text`: a text containing `b00000000001` gets `b00000000002`.
    - `your_text_typed_verbatim`: your message is pasted byte-equal, with no frame.
  - Stopped and errored recipients:
    - `your_message_starts_stopped_agent`: `SendMessage` to a stopped agent launches it with the text as its task.
    - `agent_message_to_stopped_waits`: stays `queued`, no launch; delivered at the agent's first rest after you start it.
    - `card_answer_to_stopped_duo_waits_queued` (built with Task 3.6's `MessageKind::CardAnswer`; here with a hand-made one): no launch of Duo.
    - `agent_message_to_errored_waits`.
  - Limits:
    - `duplicate_refused_within_10_minutes`: the same text again at +9 min → `Limit(Duplicate)`; at +10 min 1 ms → accepted.
    - `rate_limited_to_5_a_minute_per_pair`: the 6th within 60 s → `Limit(Rate)`; to another recipient → accepted.
    - `inbox_full_at_20_waiting`: the 21st queued for one recipient → `Limit(InboxFull)`.
    - `user_never_throttled`: 30 identical messages from you in a minute are all accepted.
  - Status and count:
    - `answered_when_recipient_writes_back`: A → B delivered, then B → A accepted → A → B is `answered`.
    - `message_status_only_for_sender`: another agent asking for that id → `UnknownMessage`.
    - `message_status_says_what_it_waits_on`: `squad-1` → stopped `pay-qa` → `Status { status: Queued, reason: Some("pay-qa is stopped") }`; errored → `pay-qa is errored`.
    - `send_outcome_by_recipient_state`: to idle → `TypedIn`; to working → `Queued(Working)`; your message to stopped → `Started`; an agent's to stopped → `Queued(Stopped)`.
    - `send_message_sender_is_token_not_payload`: `send_message {to: pay-qa, text, "from": "Duo"}` with `squad-1`'s token queues a message whose `from` is `Agent(squad-1)` and whose paste is framed `[message from squad-1 …`.
    - `your_send_refused_for_unknown_agent`: `SendMessage { to: nobody }` → `MessageRefused { reason: "no agent named nobody" }`.
    - `snapshot_lists_your_queued`.
    - `inbox_count_published`: `InboxChanged { waiting }` follows 1, 2, 1, 0 through two queues and two deliveries.
  - Persistence:
    - `persisted_and_replayed_after_restart`: two queued messages survive a new engine on the same `FakeStore`; after `Reconnected` the at-rest recipient gets the first.
    - `closed_changes_queue_and_run_on_open`: a `send_message` queued in `FakeStore` with `squad-1`'s stored token becomes, after `Reconnected`, a `queued` message from `squad-1` with an id; the queue is then empty.
    - `queued_call_with_unknown_token_dropped_on_open`: a queued call whose token matches no session row adds no message and no event.
    - `replayed_call_publishes_no_tool_result`: the run under `CallId(0)` publishes no `ToolResult`.
    - `queued_before_open_runs_after` (`crates/duohive/tests/mcp.rs`): `queue_tool_call` into a closed data dir, then start the system → the message is queued after `Reconnected`, and a helper call made right after start-up connects instead of queuing.
  - Store: `messages_round_trip`; `migration_5_applies`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features inbox; cargo test -p duohive-store; cargo test -p duohive --test mcp`
- [ ] **Step 3: Implement**, and edit TD §6.3 and §7.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: inbox with framed, confirmed delivery and limits`

### Task 3.8: The Duo chat core

**Files:**
- Create `crates/duohive-core/src/engine/chat.rs`.
- Modify `engine/mod.rs` (`ChatToDuo` dispatch, `Snapshot.chat`), `engine/signals.rs` (Duo's reply at turn end), `engine/reconnect.rs` (load the last 500).
- Modify `ports.rs` (`Store` chat methods), `testkit.rs`, `crates/duohive/src/system/enriching_store.rs`, `crates/duohive-store/src/{db.rs, sqlite_store.rs}` (migration 6).
- Modify `engine/inbox.rs`: your `SendMessage` writes its `→` line.
- Docs: TD §6.5 (P3-Q5's 500 lines, P3-D26) and §7 (the chat table).

**Interfaces — Produces:**

```rust
pub const CHAT_LOAD: usize = 500;
impl Engine {
    pub(super) fn chat_to_duo(&mut self, text: String) -> Vec<Event>;
    pub(super) fn chat_action(&mut self, text: String, result: String, out: &mut Vec<Event>);   // used here, by 3.6 and 3.5
    fn chat_entry(&mut self, kind: ChatKind, text: String, out: &mut Vec<Event>);              // stores, then ChatLine
}
// ports.rs
fn load_chat(&mut self, last: usize) -> Result<Vec<ChatEntry>, StoreError>;   // oldest first, and the highest id
fn save_chat(&mut self, entry: &ChatEntry) -> Result<(), StoreError>;
```

**Migration 6:**

```sql
CREATE TABLE chat(id INTEGER PRIMARY KEY, at INTEGER NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL,
                  result TEXT, card INTEGER, message INTEGER);
```

**Rules:**
- **`ChatToDuo`:** a `You { message }` entry, then `send(Sender::You, Duo, Text, text)`: typed in at rest, queued while busy, starting Duo when stopped or errored (P3-D26).
- **Duo's reply:** every Duo `TurnEnded { last_reply: Some(r) }` adds a `Duo` entry with `r`, even when it equals the previous reply (`take_reply`'s dedupe is for `LastReplyChanged` only). No reply, no entry.
- **Duo's states:** a turn end goes straight to idle (TD §6.4, already built); Duo's own CLI prompt makes it waiting and adds it to Needs you like any agent.
- **`Note` entries** are never stored.
- **Your `SendMessage`** (from the chat's `@agent` or Give work's Send as message instead) writes `chat_action("@<to> \"<text cut to 40>\"", <result>)` from its `SendOutcome`: `typed in`, `queued (<to> is <state word>)` or `started` (P3-D14's words). A refused one writes nothing (`MessageRefused`).

- [ ] **Step 1: Write the failing tests** (engine harness).
  - `queued_until_duo_at_rest`: Duo working → `You` entry, message `queued`, no paste; Duo idle → pasted verbatim.
  - `chat_to_stopped_duo_starts_it`: launches Duo with the text as its task.
  - `reply_becomes_chat_line`: a Duo `Stop` hook with `last_assistant_message: "done"` → `ChatLine(Duo, "done")`; the same text on the next turn → a second entry.
  - `no_reply_no_line`.
  - `duo_goes_straight_to_idle`: Duo's turn end → idle, never finished.
  - `duo_waiting_joins_needs_you`: a Duo `PermissionRequest` → Duo waiting, in Needs you, with an alert titled `Duo needs you`.
  - `transcript_survives_restart`: entries saved; a new engine's `Snapshot.chat` holds them in order; with 600 stored, it holds the last 500.
  - `your_message_writes_action_line`: `SendMessage { to: pay-qa, text: "rerun" }` with `pay-qa` working → `ChatLine(Action { result: "queued (pay-qa is working)" }, "@pay-qa \"rerun\"")`; idle → `typed in`; stopped → `started`; to `nobody` → no line.
  - Store: `chat_round_trip` (every stored `ChatKind`); `migration_6_applies`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features chat; cargo test -p duohive-store`
- [ ] **Step 3: Implement**, and edit TD §6.5 and §7.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: duo chat transcript`

### Task 3.6: Approval cards

**Files:**
- Modify `crates/duohive-core/src/card.rs` (summary, `answer_text`), create `engine/cards.rs`.
- Modify `engine/tools.rs` (dispatch `propose_org_change`), `command.rs`, `org.rs` and `engine/org_edit.rs` (`OrgEdit::Move`, P3-D20), `engine/timers.rs` (cards in `needs_you`), `engine/alerts.rs` (card alerts), `engine/reconnect.rs` (load pending cards), `alert.rs`.
- Modify `ports.rs` (`Store` card methods), `testkit.rs`, `crates/duohive/src/system/enriching_store.rs`, `crates/duohive-store/src/{db.rs, sqlite_store.rs}` (migration 7).
- Every exhaustive `match` on `OrgEdit`: `crates/duohive-core/src/engine/org_edit.rs`, `crates/duohive-core/src/event.rs` (tests), `crates/duohive-tui/src/forms/mod.rs`.
- Docs: TD §4.2 (card Needs you rows, P3-D25), §6.2 (P3-D12, P3-D21), §7 (the cards table); UI §4.4.2: on a team, Move to… lists positions among the teams (`before <team>`, `at the end`), and the drag rule that refuses a team onto a role stays.

**Interfaces — Produces:**

```rust
// card.rs
impl Card { pub fn summary(&self, org: &Org) -> String; }            // "stop squad-1", "add pay-web", "move pay-qa to QA", "remove team Growth", …
pub fn answer_text(id: CardId, outcome: &CardOutcome, summary: &str) -> String;   // P3-D12 message
pub fn result_line(outcome: &CardOutcome, summary: &str) -> String;               // "approved: stop squad-1"
// org.rs
pub enum MoveTarget { Into(NodeKey), Before(NodeName), End }       // Before/End: a team's position among the teams
OrgEdit::Move { node: NodeKey, to: MoveTarget }                     // agent → Into(Role); role → Into(Team | Duo); team → Before(team) | End
// engine/cards.rs
impl Engine {
    pub(super) fn raise_card(&mut self, source: CardSource, change: CardChange, out: &mut Vec<Event>) -> CardId;
    pub(super) fn approve_card(&mut self, card: CardId, edited: Option<CardChange>) -> Vec<Event>;
    pub(super) fn reject_card(&mut self, card: CardId) -> Vec<Event>;
}
pub fn card_alert(duo: &AgentName, summary: &str) -> Alert;          // title "Duo needs you", body = summary, kind NeedsYou
// ports.rs
fn load_cards(&mut self) -> Result<Vec<Card>, StoreError>;            // pending ones, and the highest id
fn save_card(&mut self, card: &Card) -> Result<(), StoreError>;
```

**Migration 7:**

```sql
CREATE TABLE cards(id INTEGER PRIMARY KEY, source TEXT NOT NULL, change TEXT NOT NULL, raised_at INTEGER NOT NULL,
                   outcome TEXT, answered_at INTEGER);    -- change: CardChange as JSON
```

**Rules:**
- **Raise:** save, `CardRaised`, a Needs you row (P3-D25), and `card_alert` unless `settings.alerts.needs_you` is off or start-up is replaying (P2-D14).
- **Approve** (P3-D21): `Org(edit)` → the same path as `EditOrg`; `Stop` → `stop_agent` by the state now; `NewAgent` → `AddAgent`, then `give(new agent, task, run)`. An `edited` change must be the same kind (`Stop` can't be edited; `NewAgent` keeps its `task`, P3-D7). A failure is `NotApplied(reason)`. Then `CardAnswered`, a `CardResult` chat entry (`result_line`), and `send(Sender::You, Duo, MessageKind::CardAnswer(id), answer_text(..))`.
- **Reject:** applies nothing; the same answer path with `Rejected`.
- **An answered card** takes no more answers: `ApproveCard` or `RejectCard` on it → `CommandRejected("card <id> was already answered")`.
- **Duo keeps working:** raising never changes Duo's state or blocks its next call.
- **`Move`:** `preview_edit` validates it (names, levels, D4); an agent's worktree rows and session are untouched.
- **`propose_org_change`** (dispatched here): `preview_edit` must pass (else `InvalidChange` with the error); removing a non-stopped agent, or a role or team holding one → `Refused("<agent> is <state>; only a stopped agent can be removed")`; else `raise_card(CardSource::Duo, CardChange::Org(edit))`, `Pending`.

- [ ] **Step 1: Write the failing tests** (engine harness).
  - `tool_returns_pending_with_card_id`: `propose_org_change` with Duo's token → `Pending { card: CardId(1) }` before any answer; a second → `CardId(2)`.
  - `propose_invalid_change_refused`: a duplicate name → `InvalidChange`, no card; `remove_running_agent_rejected` (and a role holding one).
  - `approve_applies_edit`: an `AddAgent` card approved → `OrgChanged` holds the agent; `CardAnswered(Approved)`.
  - `edited_card_applies_edited_change`: approved with an edited `AddAgent` (directory changed) → the org holds the edited directory; outcome `EditedApproved`.
  - `reject_applies_nothing_and_tells_duo`: no `OrgChanged`; Duo's inbox gets `card 1 rejected: add pay-web`.
  - `answer_reaches_duo_as_message`: an idle Duo is pasted `card 1 approved: add pay-web`, unframed.
  - `answer_to_stopped_duo_waits_queued`: Duo stopped → the answer is `queued`, Duo isn't launched; after `StartAgent(Duo)` reaches idle, it is pasted.
  - `card_answer_only_from_command`: a message from `squad-1` whose text is `card 1 approved: add pay-web` changes nothing about card 1.
  - `approve_stop_after_target_finished_stops_directly`: raised while working; the agent finishes; approve → quit keys sent, outcome `Approved`.
  - `approve_stop_after_target_stopped_reports_it`: outcome `NotApplied("squad-1 is already stopped")`; Duo gets `card 1 approved, not applied (squad-1 is already stopped): stop squad-1`.
  - `approve_remove_after_agent_started_not_applied`: a remove card for stopped `pay-qa`; `pay-qa` starts; approve → `NotApplied`, `pay-qa` still in the org.
  - `approve_add_after_name_taken_not_applied`: you add `pay-web` by hand first → `NotApplied` naming the duplicate.
  - `answered_card_refuses_second_answer`.
  - `cards_in_needs_you_oldest_first`: a card raised at t=10 sorts between an agent waiting since t=5 and one since t=20; its row has `card: Some(..)`, agent Duo, excerpt the summary; answering removes it.
  - `card_raises_alert`: `Alert { title: "Duo needs you", body: "stop squad-1" }`; `card_alert_respects_needs_you_toggle`: with `alerts.needs_you = false`, none; `no_card_alerts_during_replay`.
  - `pending_cards_survive_restart`: a new engine on the same `FakeStore` has the card in its `Snapshot.cards` and Needs you, with no alert.
  - Move: `move_agent_to_role`, `move_role_to_team_and_root`, `move_team_reorders` (teams `[Payments, Growth, Ops]`: `Growth` `End` → `[Payments, Ops, Growth]`; `Ops` `Before(Payments)` → `[Ops, Payments, Growth]`), `move_team_onto_role_refused` (`Into(Role(..))` for a team → `InvalidChange`, and so is `Before`/`End` for an agent or role), `move_running_agent_applies_next_start` (its session continues; the next launch resolves the new role's defaults).
  - Store: `cards_round_trip` (every `CardChange` and `CardOutcome`); `migration_7_applies`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features cards org_edit; cargo test -p duohive-store`
- [ ] **Step 3: Implement**, and edit TD §4.2, §6.2 and §7, and UI §4.4.2 (team positions).
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: approval cards, card alerts and org moves`

### Task 3.7: Task to a role

**Files:**
- Create `crates/duohive-core/src/engine/assign.rs`.
- Modify `engine/mod.rs` (your `AssignToRole`: `Event::RoleAssigned` or `Event::AssignRefused`), `engine/tools.rs` (dispatch `assign_to_role`; parking a call on a worktree creation), `engine/worktrees.rs` (answer parked calls on `Created` or its failure).
- Docs: TD §5.4 (P3-D15, P3-D28).

**Interfaces — Produces:**

```rust
impl Engine {
    /// TD §5.4. `pick` (from Give work's ↑↓) must be free; `run` None = the picked agent's resolved default.
    pub(super) fn assign(&mut self, role: &RoleKey, task: String, pick: Option<AgentName>, run: Option<RunIn>,
                         out: &mut Vec<Event>) -> Result<Assigned, ToolError>;
    pub(super) fn free_agents(&self, role: &RoleKey) -> Vec<AgentName>;   // at rest longest first (P3-D28)
}
pub(super) enum Assigned { Given { agent: AgentName, given: Given }, Card(CardId) }
pub(super) fn clone_card(org: &Org, role: &RoleKey, task: String) -> CardChange;   // P3-D15
// engine/tools.rs: a call whose give is `CreatingWorktree` waits for git (P3-D23)
impl Engine { pub(super) fn park_until_created(&mut self, call: CallId, agent: AgentName); }
```

**Rules:**
- **Free:** stopped, finished or idle, no delivery pending (P2-D11), no worktree being created (P2-D20). Errored is not free.
- **One free:** `give` to the first of `free_agents`, or to `pick` when it is free (else `Refused("<agent> isn't free any more")`).
- **None free:** `raise_card(CardSource::NoFreeAgent, clone_card(..))`, whatever Duo's state.
- **Approving the clone card** (Task 3.6's path) adds the agent and gives it the task with the card's Run.
- **An unknown role** → `UnknownRole`.
- **Your `AssignToRole`** publishes `RoleAssigned { role, task, outcome: Given { agent, given } }` or `RoleAssigned { .., outcome: Card(id) }`; a refusal (`<agent> isn't free any more`, unknown role, empty task) publishes `AssignRefused { role, task, reason }`, so the dialog stays open with the task (P3-D28).
- **A worktree pick through the tool** parks the call; `Created(Ok)` answers `Given { Started { worktree: true } }`, a failure answers `Refused("couldn't create worktree: <e>")`.

- [ ] **Step 1: Write the failing tests** (engine harness).
  - `longest_at_rest_wins`: `squad-2` finished at t=100, `squad-1` idle at t=50, `squad-3` working → `squad-1` takes it (`TypedIn`).
  - `tie_goes_to_tree_order`.
  - `stopped_counts_as_free`: all stopped → the earliest-stopped launches with the task.
  - `pending_or_creating_not_free`: an idle agent with a delivery pending, and a stopped one getting a worktree, are skipped.
  - `none_free_raises_card_with_task`: all working → `CardRaised` with `NewAgent { task, clone_of: Some(squad-1) }`, named `generalist-1`; the tool returns `Pending`.
  - `clone_named_by_d11_skipping_taken`: with `generalist-1` taken → `generalist-2`; in team Payments, role Backend → `payments-backend-1`.
  - `clone_copies_first_agents_fields`: the draft's directory, CLI and flags equal `squad-1`'s own.
  - `none_free_card_raised_while_duo_stopped`, `none_free_card_raised_while_duo_busy`: the card is raised and Duo isn't touched.
  - `assign_none_free_returns_pending` (through the tool).
  - `approve_creates_and_gives`: approving adds `generalist-1` to the org and launches it with the task.
  - `approve_clone_after_role_deleted_not_applied`: the role is deleted before approval → `NotApplied`.
  - `squad_worktrees_on`: a role whose resolved worktree default is on → a stopped pick launches with `RunIn::NewWorktree`, and the clone card holds `NewWorktree`.
  - `pick_must_still_be_free`: `pick: squad-2`, which started working meanwhile → `AssignRefused { reason: "squad-2 isn't free any more" }`; a pick that is getting a worktree ready → the same.
  - `your_assign_publishes_who_took_it`: `RoleAssigned { outcome: Given { agent: squad-1, given: TypedIn } }`; none free → `RoleAssigned { outcome: Card(CardId(1)) }`.
  - `tool_assign_in_worktree_waits_for_created`: no `ToolResult` until `Created(Ok)`; a creation failure → `Refused`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features assign`
- [ ] **Step 3: Implement**, and edit TD §5.4.
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: give a task to a role`

### Task 3.5: Duo — its prompt and its tools

Runs last of the core tasks (after 3.4, 3.8, 3.6 and 3.7, whose paths its tools call). `send_message` and `message_status` (3.4), `propose_org_change` (3.6) and `assign_to_role` (3.7) are already dispatched; this task adds the other five tools and the action line for every call.

**Files:**
- Create `crates/duohive-core/src/duo.rs` and `engine/duo.rs`.
- Modify `inherit.rs` (Duo's resolved prompt), `engine/tools.rs` (dispatch `list_agents`, `read_agent`, `wait_for_agent`, `start_agent`, `stop_agent`; the action line after every call), `engine/assign.rs` (`start_agent` reuses its worktree parking), `engine/signals.rs` (wait and read waiters on turn end, stop and `Screen`), `engine/timers.rs` (wait and read deadlines), `engine/mod.rs` (`next_deadline`).
- Docs: TD §6.2 (P3-D16, P3-D23, P3-D27), §6.4 (P3-D13, P3-D14); UI §4.6's action-line words (P3-D14).

**Interfaces — Produces:**

```rust
// duo.rs
pub const DUO_PROMPT: &str;                                 // P3-Q6 draft; must contain P3-D13's points
pub fn action_line(call: &ToolCall, result: &ToolResult) -> Option<(String, String)>;   // (text, result word); None for Rights
// inherit.rs: resolve(Duo).prompt = "# Org\n<org prompt>\n\n# Duo (DuoHive)\n<DUO_PROMPT>" (the org heading only when set)
// engine/duo.rs
struct Wait { call: CallId, caller_token: Token, agent: AgentName, due: Millis }
struct Read { call: CallId, caller_token: Token, agent: AgentName, due: Millis }
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);
```

**Tools:**

| Tool | Does |
|---|---|
| `list_agents` | `ToolOk::Agents`, Duo first, then tree order; `free` per TD §5.4 |
| `read_agent` | stopped or errored with no window → `Refused("<agent> is stopped")`; else `Runtime::capture`, answered by the next `Screen` (P3-D23) |
| `wait_for_agent` | P3-D23; deadline `timeout_minutes`; an agent already stopped or errored answers at once |
| `start_agent` | `give(agent, task, run or resolved default)`: `Given`; `CreatingWorktree` parks the call until git answers (Task 3.7's `park_until_created`, P3-D23); busy → `Refused("<agent> is <state>; send_message instead")`; pending → `Refused("<agent> is still taking the last task")` |
| `stop_agent` | P3-D6: finished, idle → `stop_agent` path, `Stopped`; errored → stopped at once, `Stopped`; starting, working, waiting → `raise_card(CardChange::Stop)`, `Pending`; stopped → `Refused("<agent> is already stopped")` |
| `assign_to_role`, `propose_org_change`, `send_message`, `message_status` | already dispatched (3.7, 3.6, 3.4); this task only adds their action lines |

Each answered call except `Rights` writes `action_line`'s entry through Task 3.8's `chat_action`, then publishes `ToolResult`. Duo's stop or relaunch drops its pending waits and reads with no answer.

- [ ] **Step 1: Write the failing tests** (engine harness, Duo's token unless noted).
  - Prompt: `duo_prompt_has_required_points`: `DUO_PROMPT` contains `card`, `pending`, `[message from`, `DuoHive boundary`, `commit, push, merge, publish and delete`, and each of the 9 tool names; `duo_prompt_under_org_prompt`: with org prompt `O`, Duo's resolved prompt starts `# Org\nO\n\n# Duo (DuoHive)\n`; other agents' prompts don't contain `DUO_PROMPT`.
  - One test per tool, allowed and refused:
    - `list_agents_shows_org_state_and_replies`: 6 agents plus Duo, `squad-2` `finished` with its last reply, `auth-service` `waiting_messages: 1`.
    - `read_agent_returns_screen`: `RuntimeCall::Capture(billing-api)` then `Screen` → `Screen { text }`; `read_agent_times_out_no_screen` after 5 s; `read_stopped_agent_refused`.
    - `wait_returns_at_once_when_at_rest`, `wait_on_stopped_agent_answers_stopped` (at once, `outcome: Stopped`), `wait_ends_on_turn_end` (`TurnEnded`, with the reply), `wait_times_out` (`TimedOut` at 10 min by default), `wait_ends_when_target_stops`, `duo_restart_drops_its_waits` (no `ToolResult` for the dropped call).
    - `start_in_worktree_answers_after_created`: `run: "worktree"` → no `ToolResult` until `Created(Ok)`, then `Given { Started { worktree: true } }`; `start_in_worktree_creation_fails_refused`: `Created(Err(Other("disk full")))` → `Refused("couldn't create worktree: disk full")`, and the 120 s no-answer (P2-D22) the same with `git didn't answer`; `start_busy_agent_refused`; `start_idle_agent_types_in`.
    - `stop_idle_agent_direct`, `stop_errored_agent_direct`, `stop_busy_agent_raises_card`, `stop_starting_agent_raises_card`, `stop_waiting_agent_raises_card` (each `Pending { card }` with a `CardRaised` for `Stop`), `stop_stopped_agent_refused`.
  - `every_call_is_an_action_line`: each of the 9 tools, answered or refused, adds exactly one `ChatLine` with `ChatKind::Action` whose text and result match P3-D14 (for example `stop squad-1` / `card raised`); `Rights` adds none.
  - `no_tool_answers_a_prompt`: with `billing-api` waiting, no tool sends keys to it (`stop_agent` raises a card; `send_message` queues).
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-core --all-features duo`
- [ ] **Step 3: Implement.** Draft `DUO_PROMPT` against P3-D13 and show the draft to the author for P3-Q6; the approved wording must be in place before Task 3.12 Step 5.
- [ ] **Step 4: Edit the docs:** TD §6.2, §6.4; UI §4.6's "Action lines" bullet drops `delivered` and lists `typed in`, `queued (… is working)`, `started`, `stopped`, `refused` and `card raised` (P3-D14), and says delivery stays visible through `message_status` and the `✉` count.
- [ ] **Step 5: Run, see PASS.** `make check`
- [ ] **Step 6: Commit.** `feat: duo's prompt, tools and action lines`

### Task 3.9: The chat pane

Replaces P2-D8, and restores the full empty-org and empty-role copy held back by D7 and P2-D3.

**Files:**
- Modify `crates/duohive-tui/src/chat.rs` (the TUI's `Who`/`ChatLine` give way to the core's `ChatEntry`; routing), `render/chat_pane.rs`, `model.rs`, `input.rs`, `tree.rs` (`✉ N`), `render/agent_pane.rs` (empty org), `render/org_pane.rs` (empty role), `live.rs` (`Command::Typing`, P3-D19).
- Create `crates/duohive-tui/src/suggest.rs`.
- Update the chat snapshots and tests that used `Who`: `crates/duohive-tui/src/{fixtures.rs, app/tests.rs, render/pane_tests.rs, input/pane_tests.rs}`.

**Interfaces — Produces:**

```rust
// chat.rs
pub struct ChatLines { pub entries: Vec<ChatEntry>, pub scroll: Option<usize> }   // in memory: at most 1 000
pub enum Route { Duo(String), Agent { to: AgentName, text: String }, Help, Agents, Unknown(String), NoAgent(String) }
pub fn route(line: &str, m: &UiModel) -> Route;
pub(crate) fn chat_rows(entries: &[ChatEntry], width: u16, queued: &BTreeSet<MessageId>, m: &UiModel) -> Vec<Line<'static>>;   // hanging indent
// suggest.rs
pub enum SuggestKind { Agent, Command }
pub struct Suggest { pub kind: SuggestKind, pub items: Vec<(String, String)>, pub cursor: usize }   // (insert, detail)
pub fn suggestions(input: &str, cursor: usize, m: &UiModel) -> Option<Suggest>;
impl Suggest { pub fn take(&self, input: &mut LineEdit); }
// UiModel gains `queued: BTreeSet<MessageId>` (your messages not yet delivered) and `suggest: Option<Suggest>`
```

**Behaviour** (UI §4.6–§4.7):
- **History** from `Snapshot.chat` and `ChatLine`s: `10:05 you   …`, `10:05 Duo   …`, and `→ <text>   <result>` lines indented under the message column; wrapped lines hang at the message column.
- **`enter`** sends by `route`: `Duo` → `ChatToDuo`; `Agent` → `SendMessage` (your line shows `→ @<to> "…"` with the core's result); `Help`, `Agents` → local `Note` lines; `Unknown`, `NoAgent` → footer, text kept (P3-D26). `alt+enter` adds a line.
- **Your messages refused:** `MessageRefused` keeps the text in the input with the footer `<reason>`.
- **Queued lines:** `queued` starts from `Snapshot.your_queued` and follows `MessageQueued`/`MessageDelivered`/`MessageDropped`; a `you` line whose message is in `queued` shows `queued (Duo is <state word>)` right-aligned in `fg.muted` until `MessageDelivered`.
- **Dropped:** `MessageDropped` from you puts the text back in an empty input with `· enter to send again`.
- **`✉ N`** on a tree row from `AgentView.inbox` / `InboxChanged`, `✉N` in the compact tree.
- **Suggestions:** `@` lists agents with their state cell (Duo first, then tree order), `/` lists `/help commands and keys` and `/agents every agent and its state`; typing filters by prefix; `tab` or `enter` takes; `esc` closes. At most 6 rows, above the input.
- **`esc` layers:** the suggestion list first, then a focused card back to the input (Task 3.10), then the org pane keeping the draft (P2-D28).
- **Kept from phase 2:** the wheel 3 lines a tick, `PgUp`/`PgDn` a page, `· scrolled` (P2-D25); letter keys type.
- **Friction entry 13:** at 80×24, when the newest entry doesn't fit the history rows, its first lines show, not its tail.
- **Copy restored:** empty org `Your org is empty · n to add a team, role or agent, or ask Duo to set one up`; empty role `no agents yet · n to add one or ask Duo`.
- **Typing mode** sends `Command::Typing { agent: Some(..) }` on entering and `{ agent: None }` on leaving, or when the stream closes (P3-D19).

- [ ] **Step 1: Write the failing tests.**
  - Snapshots matching UI §4.6–§4.7: `chat_history_80x24` / `_160x48` (you, Duo and `→` lines), `chat_suggest_agents_160x48`, `chat_suggest_commands_160x48`, `chat_queued_you_line_160x48`.
  - `at_agent_routes_by_state`: `@pay-qa hi` sends `SendMessage { to: pay-qa, text: "hi" }` whatever the state; the core's action line from Task 3.8's `your_message_writes_action_line` renders as `→ @pay-qa "hi"   queued (pay-qa is working)` (and `typed in`, `started` for an idle and a stopped `pay-qa`), with `✉ 1` on a busy or stopped `pay-qa`'s row.
  - `plain_text_goes_to_duo`: `ChatToDuo`; `@Duo hi` too.
  - `unknown_agent_keeps_text`: `@nobody hi` → no command, footer `no agent named nobody`, input unchanged.
  - `slash_help_and_agents_are_local`: no command is sent; `Note` lines appear; `/x` → `unknown command /x · /help lists them`.
  - `suggestions_filter_and_take`: `@a` lists `auth-service` only; `enter` makes the input `@auth-service `; `/` lists two commands.
  - `messages_between_agents_not_in_chat`: a `MessageQueued` from `squad-1` to `pay-qa` adds no chat row, only `✉ 1`.
  - `dropped_message_returns_to_input`.
  - `chat_scroll_kept_from_p2_d25`: P2-D25's six chat scroll tests pass unchanged against `ChatEntry`.
  - `esc_layers_popup_card_pane`: with the list open, `esc` closes it; again, focus goes to the org pane and the draft stays.
  - `chat_80x24_shows_message_start`: a 5-line Duo reply in a 4-row chat shows its first line.
  - `hanging_indent_wraps_under_text`.
  - `empty_org_and_role_ask_duo`: the restored copy, replacing `empty_org_keeps_d7_text`.
  - `typing_mode_sends_typing_command`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui chat suggest`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`. The reviewer checks the snapshot diffs against UI §4.6–§4.7.
- [ ] **Step 5: Commit.** `feat: duo chat pane with routing and suggestions`

### Task 3.10: Approval cards in the TUI

**Files:**
- Create `crates/duohive-tui/src/render/card.rs`.
- Modify `focus.rs` (`Focus.card`), `model.rs` (`cards`), `input.rs`, `chat.rs`, `render/chat_pane.rs`, `render/needs_you.rs`, `input/mouse.rs`, `forms/{mod.rs, agent.rs, team.rs, role.rs}` (pre-fill and Save as approval).

**Interfaces — Produces:**

```rust
// focus.rs: Focus gains `card: Option<CardId>` (Some only while the chat pane has focus)
// model.rs: UiModel gains `cards: BTreeMap<CardId, Card>` (pending, from Snapshot, CardRaised, CardAnswered)
// render/card.rs
pub(crate) fn card_title(card: &Card, org: &Org) -> String;      // "◆ Proposed change" | "◆ Stop squad-1" | "◆ No free agent in Generalist"
pub(crate) fn card_lines(card: &Card, org: &Org) -> Vec<String>;  // body, then the button row
pub(crate) fn render_card(card: &Card, org: &Org, focused: bool, area: Rect, buf: &mut Buffer, p: &Palette);
// forms/mod.rs
pub enum FormPurpose { Edit, Card(CardId) }                      // Card: Save sends ApproveCard { edited: Some(..) }
pub fn form_for_card(card: &Card, m: &UiModel) -> Option<Form>;  // None for stop, remove and move cards (P3-D7, P3-D22)
```

**Behaviour** (UI §4.6, §4.1.3):
- **A pending card** draws in the chat at its place, indented like action lines: outline `border`, heavy when focused, an amber `◆` in its title (P3-D4). Body lines: an org change in UI §4.6's style (`+ Add agent pay-web in Payments › Frontend (new role)`, `  CLI Claude Code · ~/code/web · worktrees on`); a stop `squad-1 is ● working 12m`; a new-agent card P3-D15's two lines. Buttons: `[y] Approve   [e] Edit   [n] Reject`, without `[e]` where it is disabled.
- **Keys:** `↑` from an empty input focuses the newest pending card; `y`/`e`/`n` act only while a card has focus; `esc` returns to the input. With a card focused the footer reads `y approve · e edit · n reject · esc input` (`e edit` dropped where disabled). `e` where disabled → footer `nothing to edit · y or n`.
- **`e`** opens `form_for_card`; Save sends `ApproveCard { card, edited: Some(change) }` and returns focus to the chat; `esc` in the form returns to the card, unanswered. A new-agent card's form edits the agent; the task is kept and shown read-only under it.
- **Answered:** `CardAnswered` replaces the card with its `CardResult` line, `→ approved: stop squad-1   10:16`.
- **Needs you:** a card row reads `◆ Duo  approval  1m  <Summary>` (summary's first letter upper-cased, cut with `…`); `enter` or a click on it focuses that card in the chat, so an older card takes the keys.
- **Mouse:** a click on a card focuses it; a click on `[y]`, `[e]` or `[n]` acts as its key (P2-D25 handed these to phase 3).

- [ ] **Step 1: Write the failing tests.**
  - `up_from_input_focuses_newest_card`: two pending cards → `↑` focuses the newer; with text in the input, `↑` doesn't.
  - `needs_you_enter_focuses_older_card`: `enter` on the older card's row focuses it and focus moves to the chat.
  - `yen_only_when_focused`: `y` in the input types `y`; focused → `ApproveCard { edited: None }`; `n` → `RejectCard`.
  - `edit_saves_as_approval`: `e` on an add-agent card opens the agent form with the proposed values; Save → `ApproveCard { edited: Some(Org(AddAgent{..})) }`.
  - `e_on_stop_card_disabled`: no form; footer `nothing to edit · y or n`; same for remove and move cards (P3-D22).
  - `e_on_new_agent_card_keeps_task`: the form's Save carries the card's task unchanged.
  - `esc_from_card_returns_to_input`.
  - `answered_card_collapses_to_line`.
  - `needs_you_card_row`: `◆ Duo  approval  1m  Add pay-web`.
  - `click_card_row_focuses_card`, `click_card_button_acts`: a click on `[n]` sends `RejectCard`.
  - Snapshots: `card_focused_80x24` / `_160x48`, `card_unfocused_160x48`, `stop_card_160x48`, `new_agent_card_160x48`, `needs_you_with_card_160x48`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui card`
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: approval cards in the chat and needs you`

### Task 3.11: Give work — roles and Send as message instead

Restores the role summary's `space give work to this role · e edit` (P2-D3) and replaces P2-D10.

**Files:**
- Modify `crates/duohive-tui/src/forms/give_work.rs`, `render/give_work.rs`, `input/give_work.rs`, `render/agent_pane.rs` (role summary footer), `input.rs` (`space` on a role).
- Update `crates/duohive-tui/src/forms/tests/give_work.rs` and its snapshots.
- Docs: UI §4.5's role mock (P3-D29).

**Interfaces — Produces:**

```rust
// forms/give_work.rs: Variant gains
Variant::Role { role: RoleKey, free: Vec<AgentName>, total: usize, pick: usize }   // free in P3-D28 order
Variant::RoleNoneFree { role: RoleKey, total: usize }
// GiveFocus gains `Pick`; the busy variant's buttons: `GiveFocus::{GiveButton, SendAsMessage}`
```

**Behaviour** (UI §4.5):
- **`space` on a role** opens the role variant: title `Give work · role <path> (<free> of <total> free)`, where `<path>` is `Payments › Backend` or `Generalist`.
  - **Goes to:** `▌<agent> (<state word> <time>)` with `↑↓ pick another`; the Run row follows the pick (P3-D29). `enter` sends `AssignToRole { role, task, pick: Some(agent), run }`.
  - **None free:** `Goes to  No free agent · enter proposes one` and `(the approval card holds the task)`; the hint `enter propose one · esc`. `enter` sends `AssignToRole { pick: None, run: None }`; the footer then says `No free agent · a card in the chat proposes one`.
  - **`space` on a team or on Duo** keeps phase 2's note, `space gives work to an agent or a role`.
- **Busy variant:** P2-D10's text becomes UI §4.5's (`… Give work when it is at rest, or send this as a message: it is typed in when <agent> is at rest.`), with `[ Give work ] (disabled)` and `▌[ Send as message instead ]` focused; `enter` sends `SendMessage { to, text: task }` and the footer says `→ queued for <agent>`.
- **The role summary** ends `space give work to this role · e edit`.
- **Results:** `AssignRefused` (for example `<agent> isn't free any more`) keeps the dialog open with the task and the reason as the footer note; `RoleAssigned` closes it with `<agent> took the task` or, for `Card`, `No free agent · a card in the chat proposes one`.

- [ ] **Step 1: Write the failing tests.**
  - Snapshots of each variant, `_80x24` / `_160x48`: `give_work_role`, `give_work_role_none_free`, `give_work_busy_send_as_message`.
  - `role_pick_cycles_free_agents`: `↓` twice over 2 free agents wraps; the sent `pick` follows.
  - `role_run_row_follows_pick`: a finished pick shows the next-turn line; a stopped pick shows radios preset from its default.
  - `no_free_proposes_clone_card`: `enter` sends `AssignToRole { pick: None, .. }`, and the footer note follows.
  - `send_as_message_queues`: sends `SendMessage`, closes the dialog.
  - `role_summary_offers_give_work`: replaces `role_summary_has_phase_2_footer`.
  - `team_and_duo_still_note`.
  - `pick_not_free_keeps_dialog`.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive-tui give_work`
- [ ] **Step 3: Implement**, and edit UI §4.5's role mock (P3-D29): the Run choices beside the finished `squad-2` go, and its Run row reads `Run      in ~/code/my-app, as the next turn of squad-2's session`; a line under the mock says the Run row follows the picked agent (radios for a stopped or errored pick).
- [ ] **Step 4: Run, see PASS.** `make check`
- [ ] **Step 5: Commit.** `feat: give work to a role, and send as message instead`

### Task 3.12: End to end and acceptance

The two scripted scenarios are test-first; the real-Duo session and the docs are acceptance.

**Files:**
- Modify `crates/duohive/src/fake_agent.rs` and `args.rs` (`bench-agent --mcp-script <path>`), `crates/duohive-cli/src/fake.rs` (passes the script path from a flag in the agent's own `flags`).
- Create `crates/duohive/tests/e2e_duo.rs` and its scripts under `crates/duohive/tests/fixtures/duo/`.
- Docs: `docs/plans/v1-roadmap.md` (row 3), `docs/product/prd.md` (Delivery Phases row 3 status), TD §12 row 3 and any section a mechanism changed, UI where a screen changed, `docs/README.md`, `docs/plans/v1-traceability.md`, `docs/plans/bench-results.md`.

**The scripted fake Duo:** with `--mcp-script`, `bench-agent` starts `duohive mcp` as a child (its own `DUOHIVE_TOKEN`), and for each turn runs the first script entry whose `on` is a substring of the turn's text: `{"on": "<text>", "steps": [{"call": "<tool>", "args": {…}} | {"reply": "<text>"}]}`. Each call's answer is logged to `<run>/calls.jsonl`; `$result.<n>.<field>` in a later step's `args` takes a value from the n-th answer (for a `card_id`). The turn ends with its `reply` as `turn-end`'s reply. No model is called.

- [ ] **Step 1: Write the failing tests** (real tmux on a throwaway socket, fake CLIs, `FakeNotifier`, `NoGit`).
  - `e2e_story_through_the_factory`: org with `auth-service` (Payments › Backend), `pay-qa` (Payments › QA) and `squad-1`, `squad-2` (Generalist), all stopped. `ChatToDuo("Story: let users retry a failed payment")`. The fake Duo calls `start_agent auth-service "PAY-88a backend: retry endpoint"`, `start_agent pay-qa "PAY-88c QA: retry cases"`, `assign_to_role Generalist "PAY-88b frontend: retry button"`, then `wait_for_agent` on each, then replies `PAY-88: all three subtasks finished`. Asserted within 30 s: the chat holds the `you` line, three `→ give work …   started` lines, three `→ wait for …   finished` lines, and the reply; `auth-service`, `pay-qa` and `squad-1` are finished.
  - `e2e_whats_blocked_then_fix_it`: `billing-api` waiting on `ask` (its fake approval), `squad-1` working on a `slow` task. `ChatToDuo("what's blocked?")` → the fake Duo calls `list_agents`, `read_agent billing-api`, and replies `billing-api is waiting on an approval; squad-1 is busy`. `ChatToDuo("fix it: stop squad-1 and tell pay-qa to rerun the tests")` → `stop_agent squad-1` (`card raised`) and `send_message pay-qa "rerun the tests"`. `ApproveCard(card 1)` → `squad-1` stopped; Duo is pasted `card 1 approved: stop squad-1` as a later turn and replies `squad-1 stopped`. Every step is visible as chat entries, with no `tmux attach` and no key sent to any pane but by the inbox.
  - `e2e_role_task_reaches_free_agent`: `AssignToRole { role: Generalist, pick: None }` with `squad-2` finished and `squad-1` working → `squad-2` takes it (busy hook within 6 s); with both working → a `CardRaised` holding the task.
- [ ] **Step 2: Run, see FAIL.** `cargo test -p duohive --test e2e_duo`
- [ ] **Step 3: Implement** the script runner.
- [ ] **Step 4: Run, see PASS.** `make check`. Then commit: `test: duo end to end with a scripted fake`.
- [ ] **Step 5: Acceptance on the author's org.** First, the author approves `DUO_PROMPT`'s wording (P3-Q6). Then, with real Duo on Claude Code and on OpenCode, record in `bench-results.md` under "Phase 3 acceptance":
  1. a "what's blocked, then fix it" round trip done entirely in the chat (the chat lines, and that no `tmux attach` and no agent-pane typing was needed);
  2. a task given to a role reaching a free agent, and a no-free-agent card approved;
  3. a card answered while Duo was stopped, delivered when Duo next ran;
  4. `duohive bench --agents 10` still meets the 10-agent targets with the helpers running (idle CPU < 1 %, memory < 20 MB, hook → screen < 1 s), and the release binary under 10 MB.

  Anything that fails is fixed test-first before Step 6.
- [ ] **Step 6: Update the docs.** TD §12 row 3 checked against what phase 3 built, and any section a mechanism changed; roadmap row 3 `done <date>` and row 4 next; PRD Delivery Phases row 3 status; `docs/README.md`; the traceability matrix's adaptations table (P2-D8, P2-D10, P2-D3's copy, D7's copy removed).
- [ ] **Step 7: Run `make check`, then commit.** `docs: phase 3 acceptance and status`

**Order:** 3.1 → 3.2 → 3.3 → 3.4 → 3.8 → 3.6 → 3.7 → 3.5, in the order written above: each core task calls what the earlier ones built (Global Constraints). Then 3.9 (after 3.8), 3.10 (after 3.6) and 3.11 (after 3.7), which may run in parallel. 3.12 is last. The task numbers keep the outline's.

---

## Risks

| Risk | Response |
|---|---|
| One helper process per agent at 100 agents | Task 3.2 Step 5 measures one helper's memory and start time; it is a relay with one socket per call. PRD memory targets count DuoHive only; the bench reports the helpers' total |
| An agent's message is used to inject instructions into another | P3-D18's per-delivery boundary; Duo-only tools unreachable by token; cards change only on your key (Review Focus 1) |
| Duo loops (messaging agents that message back) | Inbox limits; `wait_for_agent` timeouts; every call a `→` line |
| Reply extraction drifts between CLI versions | Pinned versions and recorded fixtures (TD §13); the untested-version notice |
| `mcp__duohive__*` or the OpenCode `evaluate` match doesn't suppress the prompt | Task 3.2 Step 5 checks it live and stops for the author on failure |
| The MCP crate adds weight against the 10 MB binary target | P3-D8: measured in Task 3.1, with a hand-written fallback |

---

## Self-review notes

- **Spec coverage, by the outline's table:**

  | Requirement | Tasks |
  |---|---|
  | MCP endpoint and the `duohive mcp` helper | 3.1, 3.2 |
  | Per-launch MCP config (Claude `--mcp-config`, OpenCode `mcp`), checked on 2.0 | 3.2 |
  | Tokens and tool rights | 3.3 |
  | Inbox: delivery, framing, statuses, limits, stopped recipients, closed app | 3.4 (closed app: 3.2) |
  | Duo: fixed prompt, tools, visible `→` actions, Duo's own prompts | 3.5, 3.8 |
  | Approval cards: raise, approve, edit, reject, Needs you rows, alerts | 3.6, 3.10 |
  | Task to a role | 3.7, 3.11 |
  | Duo chat: input, queueing, replies, stored transcript | 3.8, 3.9 |
  | `@agent text`, `/help`, `/agents`, suggestions, `✉` on rows | 3.9 |
  | Give work: role and no-free-agent variants; Send as message instead | 3.11 |
  | End-to-end story with no model calls; user flow "a story through the factory" | 3.12 |

- **Traceability rows owned by phase 3:** PRD chat with Duo (3.8, 3.9); Duo's capabilities (3.5, 3.6, 3.10); messages by token (3.3, 3.4); Needs you cards (3.6, 3.10); task to a role (3.7, 3.11); messages to a stopped agent (3.4); pre-approved tools (3.2); no tool answers a prompt (3.5 `no_tool_answers_a_prompt`); card alerts (3.6); user flow step 5 and the story (3.12). UI §2.2 `✉` (3.4, 3.9); §4.1.2 role and empty-org copy (3.11, 3.9); §4.1.3 card rows and clicks (3.10); §4.1.4 card alert (3.6); §4.5 role, no-free, Send as message (3.11); §4.6 (3.5, 3.9, 3.10); §4.7 (3.9); §4.11 chat and card keys (3.9, 3.10); §4.12 empty role (3.9); §5 cards answer only with focus (3.10). TD §2 tools (3.1–3.3); §3.2 tool commands (3.1); §3.3 MCP listener (3.1); §3.4 `duohive-mcp` (3.1, 3.2); §4.2 card alerts and rows (3.6); §5.2 MCP (3.2); §5.4 (3.7); §6.1–§6.5 (3.1–3.8); §7 tables (3.2, 3.4, 3.6, 3.8); §8 message undeliverable (3.4); §9 end to end (3.12); §12 row 3 (3.12).
- **Review Focus:** each item names its tests, and each test is in its owning task's Step 1.
- **Type consistency:** `CallId` (with `CallId::REPLAY`), `ToolCall`, `ToolOk`, `SendOutcome`, `Given`, `ToolError`, `Rights`, `MessageId`, `Sender`, `MessageKind`, `MessageStatus`, `CardId`, `CardChange`, `CardOutcome`, `Card`, `ChatEntry`, `ChatKind`, `RoleOutcome` and the new events are defined once in 3.1; `QueuedToolCall`, `SessionRow.launched_as_duo` and `ClosedFallback`/`StoreFallback` in 3.2; `Engine::give`/`GiveRefusal` in 3.3; `send`/`deliver_next` in 3.4; `chat_action` in 3.8; `raise_card` and `MoveTarget` (in `OrgEdit::Move`, also used by P3-D27 and phase 4's 4.8) in 3.6; `assign`/`Assigned`/`park_until_created` in 3.7.
- **Dependency rule:** `ClosedFallback` is a `duohive-mcp` trait implemented in `duohive` (`mcp_cmd.rs`) over plain `duohive_store::closed` functions, so neither adapter crate depends on the other.
- **Order:** every call a test makes is built by its own task or an earlier one in the order 3.1 → 3.2 → 3.3 → 3.4 → 3.8 → 3.6 → 3.7 → 3.5; migrations 4–7 follow that order. Later tasks use these names only. `Event::ChatLine` (TD §3.2's name) carries the core's `ChatEntry`; the TUI's phase-2 `ChatLine` type is retired in 3.9.
- **Spec conflicts ruled:** UI §4.5's role mock shows Run radios beside a finished agent; the PRD's Give work by state decides (P3-D29, approved 2026-10-18; mock edited in 3.11). The PRD lets Duo propose moves, which the outline left to phase 4; the core move is built here (P3-D20), and phase 4's 4.8 row was edited to build only the picker and drag on it. UI §4.6's `delivered` action word is dropped (P3-D14).
- **Proportion:** 12 tasks, as the outline; 3.3 also carries the shared give path (said why). Code bodies appear only for the migrations; every other step gives signatures, values and tests.
