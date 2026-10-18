# DuoHive Phase 3 Plan — Duo and messaging (outline)

**Status:** Outline · written 2026-10-05 · expanded into a full test-first plan, in the format of [phase-2-plan.md](phase-2-plan.md), when phase 3 starts. Depends on phase 2.
Sources: [PRD](../product/prd.md) (Duo, the manager agent; capability rows for messages, Needs you, task to a role; User flows) · [UI/UX spec](../ux/ui-spec.md) §4.5 (role and busy variants), §4.6, §4.7, §4.1.3 (card rows) · [Technical design](../design/technical-design.md) §5.4, §6, §7, §9 (end to end) · [Roadmap](v1-roadmap.md) row 3 · [Traceability](v1-traceability.md).

**Goal:** You direct the team through Duo. Duo sees the org, messages agents, starts and stops them, and proposes org changes you approve on cards. Agents message each other safely, and a task given to a role reaches a free agent.

**Done when (roadmap):** A "what's blocked, then fix it" round trip happens entirely in chat, and a task given to a role reaches a free agent.

## Constraints carried

- **Inherited:** every Global Constraint of phase 2, and decisions D1–D32 and P2-D1–P2-D28. P2-D8 and P2-D10, and P2-D3's held-back copy, are replaced here.
- **Delivery confirmation:** inbox delivery uses P2-D11. A pasted message counts as delivered only when the agent leaves rest.
- **Identity:** a per-launch secret token (D1: generated since phase 1), checked on every tool call. It is never a name anyone can set (PRD, TD §6.1).
- **Tool rights** (TD §6.2):

  | Who | Tools |
  |---|---|
  | Every agent | `list_agents`, `send_message`, `message_status` |
  | Duo only | `read_agent`, `wait_for_agent`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change` |

  No tool answers a CLI permission prompt. Each launch pre-approves only these tools (P3-D1), which narrows phase 0's Global Constraint "no auto-approve flags of its own".
- **Inbox** (TD §6.3):
  - **Delivery:** only at rest (finished or idle) with no delivery pending, never onto a waiting prompt. It counts once the recipient leaves rest (P2-D11; TD §6.3 as edited in 2.12).
  - **Framing:** an agent's message carries the sender's name and a boundary token; your own messages are typed in verbatim.
  - **Statuses:** `queued`, `delivered`, `answered`, `dropped`. A dropped message is never retried in a way that could deliver it twice.
  - **Limits, between agents only:** a duplicate is refused within 10 minutes; at most 5 a minute per sender → recipient pair; at most 20 waiting per recipient. You are never throttled.
- **Messages to a stopped agent:** yours start it with the message as its task; an agent's message waits as `queued · stopped`. Your answer to a card, when Duo is stopped, also waits as `queued · stopped` and doesn't start Duo (P3-D2, TD §6.3).
- **Duo's limits:**
  - it cannot change the org without a card;
  - it cannot stop a starting, working or waiting agent without a card (finished, idle or errored: direct; stopped: refused, P3-D6);
  - it cannot propose removing an agent that isn't stopped (the core rejects it);
  - it cannot approve anything for you.
- **Agent Squad tasks** use the same picking as Give work to a role, with worktrees on (TD §5.4). The template itself is phase 4.
- **Closed DuoHive:** messages and Duo's requests wait in the database and are processed when DuoHive next opens (TD §6.3).

## Author decisions

| # | Decision |
|---|---|
| P3-D1 | **MCP tool approvals: only DuoHive's tools are pre-approved.** Decided by the author on 2026-10-17. Each launch pre-approves DuoHive's own MCP tools and nothing else: Claude Code: `mcp__duohive__*` (the naming of spike S1.5, which never called the tool; a prompt is expected, like any MCP tool) in the per-launch `--settings` file's `permissions.allow`, checked live on the pinned Claude Code in 3.2. OpenCode: the plugin's `evaluate` hook (2.0) allows the `duohive` server's tools; the mechanism and the tool naming are checked on 2.0 in 3.2. It is a narrow exception to TD §2's "passes no auto-approve flags of its own": none of these tools commits, pushes, merges, publishes or deletes, and every CLI prompt for the agent's own work is untouched. Global config is never touched. This keeps the done-when "entirely in chat". Built in 3.2; TD §2 and §6.2 say so. |
| P3-D2 | **Card tools don't block.** Decided by the author on 2026-10-17. `stop_agent` (when it raises a card) and `propose_org_change` return at once `{status: "pending", card_id}`. The answer (approved, edited and approved, or rejected) reaches Duo later as a message from you through the inbox, so it follows the inbox's delivery rules. When Duo is stopped, the answer waits for it as `queued · stopped`, like an agent's message, and doesn't start Duo; it is delivered when Duo next runs. Built in 3.6; TD §6.2 says so. |
| P3-D3 | **The engine raises the no-free-agent card.** Decided by the author on 2026-10-17. TD §5.4 stands: the core raises the clone card itself, and the UI shows it as Duo's, so it works while Duo is stopped or busy. The no-free-agent variant of Give work says `enter propose one`, not "ask Duo". The answer to that card reaches Duo as a message from you, as in P3-D2. Built in 3.7 and 3.11; PRD Must and UI §4.5 say so. |
| P3-D4 | **A card's colour.** Decided by the author on 2026-10-17. A card's outline is `border` (heavy when focused), with an amber `◆` (`status.attention`) in its title; no amber outline (P2-D27). Built in 3.10; UI §4.6 says so. |
| P3-D5 | **Card alerts.** Decided by the author on 2026-10-17. A card's alert follows the `needs you` toggle (`settings.alerts.needs_you`), with the title `Duo needs you` and the card's summary as its body. Built in 3.6; UI §4.1.4 and TD §4.2 say so. |
| P3-D6 | **`stop_agent` by state.** Decided by the author on 2026-10-17. Finished or idle: direct. Errored: direct. Starting, working or waiting: a card. Stopped: refused, with the reason. Built in 3.5; TD §6.2 and the PRD say so. |
| P3-D7 | **`e` on cards.** Decided by the author on 2026-10-17. On a stop card `e` is disabled, and the footer gives the reason (`nothing to edit · y or n`). On a new-agent card `e` opens the agent form pre-filled, and the task the card holds is kept unchanged. Built in 3.10; UI §4.6 says so. |

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| MCP endpoint and the `duohive mcp` helper | TD §2 Agents' tools, §3.4, §6 | 3.1, 3.2 |
| Per-launch MCP config (Claude `--mcp-config`, OpenCode `mcp`) | TD §5.2; spike S1.5, S2.5 (OpenCode 1.18; re-checked on 2.0 in 3.2) | 3.2 |
| Tokens and tool rights | PRD Identity; TD §6.1–§6.2 | 3.3 |
| Inbox: delivery, framing, statuses, limits, stopped recipients, closed app | PRD Must (messages); TD §6.3 | 3.4 |
| Duo: fixed prompt, tools, visible `→` actions, Duo's own prompts | PRD Duo; TD §6.4 | 3.5 |
| Approval cards: raise, approve, edit, reject, Needs you rows, alerts | PRD Must (Duo, Needs you); TD §4.2, §6.2; UI §4.6 | 3.6, 3.10 |
| Task to a role | PRD Must; TD §5.4; UI §4.5 | 3.7, 3.11 |
| Duo chat: input, queueing, replies, stored transcript | PRD One screen (Duo pane); TD §6.5, §7 | 3.8, 3.9 |
| `@agent text`, `/help`, `/agents`, suggestions, `✉` on rows | PRD One screen; UI §2.2, §4.7 | 3.9 |
| Give work: role and no-free-agent variants; Send as message instead | UI §4.5 | 3.11 |
| End-to-end story scenario with no model calls | TD §9 | 3.12 |
| User flow: a story through the factory | PRD User flows | 3.12 |

## Tasks

| # | Task | Crates and files | Test intent | Spec |
|---|---|---|---|---|
| 3.1 | **MCP endpoint.** Each tool call becomes a plain, serialisable `Command::Tool { call_id, token, call: ToolCall }`; the endpoint awaits the matching `Event::ToolResult { call_id, result }` (TD §3.2: no channels inside commands). `tools/list` answers per token (rights). The transport follows P3-Q2 | `duohive-mcp/src/{lib,endpoint,tools,protocol}.rs`; `duohive-core` `ToolCall`, `ToolResult` | Unit: every tool's JSON schema and result shapes; an unknown token gets an error; a non-Duo token's `tools/list` lacks the Duo tools | TD §3.2, §6.2 |
| 3.2 | **`duohive mcp` helper and per-launch config.** A stdio MCP server per agent. It reads `DUOHIVE_TOKEN` and relays to DuoHive. Both adapters add the MCP config to each launch, and pre-approve only DuoHive's tools (P3-D1): Claude Code's per-launch `--settings` allows `mcp__duohive__*` (checked live); OpenCode: the plugin's `evaluate` hook (2.0) allows the `duohive` server's tools (mechanism and naming checked live) | `duohive-mcp/src/helper.rs` (the stdio relay); `duohive/src/{mcp_cmd.rs, args.rs}` (the `mcp` subcommand wiring); `duohive-cli/src/{claude,opencode}.rs` | Launch snapshots now carry MCP config; the helper exits cleanly when DuoHive is closed and answers per P3-Q3; e2e: a fake CLI lists tools through the helper; `launch_allows_only_duohive_tools` (the launch config allows exactly the `duohive` tools and nothing else; no global config written); live on Claude Code and OpenCode 2.0: a DuoHive tool runs with no prompt, and an edit still asks; live: the pinned OpenCode 2.0 `--standalone` loads the per-launch `mcp` server and lists its tools (and how it names them), then TD §5.2 is updated | TD §5.2, §6.1 |
| 3.3 | **Tool rights and identity.** Token → agent and its rights; Duo's rights come from the Duo launch only | `duohive-core/src/engine/rights.rs` | `agent_cannot_call_duo_tools`, `token_of_stopped_agent_rejected`, `name_in_payload_ignored` | PRD Identity; TD §6.1 |
| 3.4 | **Inbox.** Queue, deliver at rest, frame, statuses, limits, `answered` detection, stopped recipients, persistence (the `messages` table, in the next free migration), replay on open, the `✉ N` count in `AgentView` | `duohive-core/src/{inbox.rs, engine/inbox.rs}`; `duohive-store` migration | Engine: each rule of the inbox constraints; `never_typed_onto_waiting`; `dropped_never_redelivered`; `user_never_throttled`; `persisted_and_replayed_after_restart` | TD §6.3 |
| 3.5 | **Duo.** Duo's fixed prompt (wording approved by the author, P3-Q6), stacked under the org prompt. Duo's tools: `read_agent` (screen text), `wait_for_agent` (turn end or timeout, P3-Q4), `start_agent` (§5.1 including the Run choice), `stop_agent` (by state, P3-D6: direct, card or refused), `assign_to_role`, `propose_org_change`. Each call is recorded as an action → `→` line with its result | `duohive-core/src/{duo.rs, engine/duo.rs}`; `duohive-cli` (Duo launch adds prompt and Duo rights) | One test per tool for allowed, refused and result; `stop_busy_agent_raises_card`, `stop_starting_agent_raises_card`, `stop_errored_agent_direct`, `stop_stopped_agent_refused`; `remove_running_agent_rejected`; `every_call_is_an_action_line` | PRD Duo; TD §6.2, §6.4 |
| 3.6 | **Approval cards.** `CardRaised`/`CardAnswered`. A card holds an `OrgEdit`, a stop, or "new agent + task". The tool returns at once `{status: "pending", card_id}` (P3-D2). Approve applies; Edit applies the edited change; Reject applies nothing. The answer reaches Duo as a message from you, through the inbox. Cards join Needs you (oldest first) and alert (`Duo needs you`, the card's summary, under the `needs you` toggle, P3-D5). Duo keeps working while a card waits. Cards are stored (the `cards` table, in the next free migration) | `duohive-core/src/{card.rs, engine/cards.rs}`; store | `approve_applies_edit`, `edited_card_applies_edited_change`, `tool_returns_pending_with_card_id`, `answer_reaches_duo_as_message`, `reject_applies_nothing_and_tells_duo`, `answer_to_stopped_duo_waits_queued`, `cards_in_needs_you_oldest_first`, `card_raises_alert`, `card_alert_respects_needs_you_toggle`, `pending_cards_survive_restart` | PRD Approval cards; TD §4.2, §6.2 |
| 3.7 | **Task to a role.** Free = stopped, finished or idle, with no delivery pending (P2-D11) and no worktree being created (P2-D20); the one at rest longest takes the task, through §5.1. When none is free, the core raises a card proposing a clone of the role holding the task, shown as Duo's, so it works while Duo is stopped or busy (P3-D3); approving it creates the agent and gives it the task, and the answer reaches Duo as a message from you | `duohive-core/src/engine/assign.rs`; `Command::AssignToRole` | `longest_at_rest_wins`, `none_free_raises_card_with_task`, `none_free_card_raised_while_duo_stopped`, `none_free_card_raised_while_duo_busy`, `assign_none_free_returns_pending`, `approve_creates_and_gives`, `squad_worktrees_on` | PRD Must; TD §5.4 |
| 3.8 | **Duo chat core.** `ChatToDuo`; queued while Duo is busy; replies taken at turn end (Claude `last_assistant_message`, OpenCode `duohive_last_reply`); `ChatLine` events; `chat` table (in the next free migration); Duo goes straight to idle; Duo's own CLI prompts make it waiting | `duohive-core/src/{chat.rs, engine/chat.rs}`; store; adapters' reply extraction already exists | `queued_until_duo_at_rest`, `reply_becomes_chat_line`, `transcript_survives_restart`, `duo_waiting_joins_needs_you` | TD §6.5, §7 |
| 3.9 | **Chat pane (full).** Replaces P2-D8, and restores the full UI §4.1.2 and §4.12 empty-org and empty-role copy (`… or ask Duo to set one up`, `… or ask Duo`) that D7 and P2-D3 held back: history from `ChatLine`s; `enter`/`alt+enter`; history scrolling kept from P2-D25 (the wheel 3 lines a tick, `PgUp`/`PgDn` a page, `· scrolled`) and P2-D28 (`esc` to the org pane, keeping the draft); `esc` closes the suggestion list first, then leaves a focused card for the input; at 80×24 the history row shows the start of the newest message, not its tail (friction entry 13); `@agent text` routing (at rest: typed in at once; busy: `→ … queued (<agent> is working)` plus `✉ 1` on its row; stopped: starts it); the `@`/`/` suggestion popup (agents with their state; `/help`, `/agents`); hanging-indent wrap; `→` action lines | `duohive-tui/src/{render/chat_pane.rs, chat_input.rs, suggest.rs}` | Snapshots matching UI §4.6–§4.7 mocks at 80×24 and 160×48; `at_agent_routes_by_state`, `suggestions_filter_and_take`, `messages_between_agents_not_in_chat`, `chat_scroll_kept_from_p2_d25`, `esc_layers_popup_card_pane`, `chat_80x24_shows_message_start` | UI §4.7; PRD Duo pane |
| 3.10 | **Approval cards in the TUI.** A card in the chat, outlined in `border` (heavy when focused) with an amber `◆` in its title, never an amber outline (P3-D4); `↑` from an empty input focuses the newest; `y`/`e`/`n` only while it has focus; `e` opens the form pre-filled and Save approves (on a stop card `e` is disabled with `nothing to edit · y or n`; on a new-agent card the held task is kept, P3-D7); answered cards collapse to `→ approved: …`; Needs you row `◆ Duo  approval  1m  <summary>`; `enter` on it focuses the card, and a click on it does the same; a click on a card's `[y]`/`[e]`/`[n]` button acts as its key (P2-D25 handed card-row clicks to phase 3) | `duohive-tui/src/render/card.rs`, `input.rs`, `forms/*` (pre-fill) | `up_from_input_focuses_newest_card`, `needs_you_enter_focuses_older_card`, `yen_only_when_focused`, `edit_saves_as_approval`, `e_on_stop_card_disabled`, `e_on_new_agent_card_keeps_task`, `click_card_row_focuses_card`, `click_card_button_acts`, snapshot of a card at 80×24 and 160×48 | UI §4.6, §4.1.3 |
| 3.11 | **Give work, the rest of §4.5.** Restores the role summary's `space give work to this role · e edit` (P2-D3). The role variant (`Goes to ▌<agent> (finished 3m)`, `↑↓` pick another), the no-free-agent variant (`enter propose one`, P3-D3), and Send as message instead on the busy variant. Replaces P2-D10 | `duohive-tui/src/forms/give_work.rs` | Snapshots of each variant; `role_pick_cycles_free_agents`, `no_free_proposes_clone_card`, `send_as_message_queues` | UI §4.5 |
| 3.12 | **End to end and acceptance.** A scripted fake Duo (fake CLI + MCP calls) runs "story → Duo splits it → three agents finish → Duo reports" and "what's blocked → fix it", with no model calls. Then a real-Duo session on the author's org; mark roadmap row 3 done; edit TD (including §12) and UI where mechanisms changed | `duohive/tests/e2e_duo.rs`; `duohive/src/fake_agent.rs` (scripted tool calls); docs | Both scenarios pass headless; the acceptance notes record the round trip and a role task reaching a free agent | Roadmap row 3; TD §9; PRD User flows |

**Order:** 3.1 → 3.2 → 3.3. Then 3.4, 3.6, 3.7 and 3.8 in parallel; each new table takes the next migration number at merge time (migrations are append-only). Then 3.5. 3.9, 3.10 and 3.11 follow their core tasks. 3.12 is last.

## Risks

| Risk | Response |
|---|---|
| One MCP helper process per agent at 100 agents | Measure its memory and start time in 3.2; the helper is a thin relay with no runtime of its own beyond stdio and one socket. PRD memory targets count DuoHive only, but report the helpers' total in the bench |
| An agent's message is used to inject instructions into another | Framing with a per-delivery boundary token (TD §6.3); Duo-only tools stay unreachable by token, whatever the text says |
| Duo loops (messaging agents that message back) | Inbox limits; `wait_for_agent` timeouts; every action visible as a `→` line |
| Reply extraction drifts between CLI versions | Pinned versions and recorded fixtures (TD §13); the untested-version notice |
| The MCP crate adds weight against the 10 MB binary target | P3-Q1 measures the binary size with and without it |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P3-Q1 | The MCP server library: `rmcp` (3.5.0 on crates.io, 2026-10-05) or a minimal hand-written JSON-RPC over stdio? | `rmcp` if the release binary stays under 10 MB, else hand-written |
| P3-Q2 | Do the helper and DuoHive talk over `run.sock` (shared with hooks, one framed protocol) or a second socket? | Share `run.sock` with a message-type field: one listener and one permission model |
| P3-Q3 | A tool call while DuoHive is closed. TD §6.3: "messages and Duo's requests wait in the database" | Requests that change something (`send_message`, `start_agent`, `stop_agent`, `assign_to_role`, `propose_org_change`) queue in the database and run on open. Reads (`list_agents`, `message_status`, `read_agent`, `wait_for_agent`) answer "DuoHive is closed", because a stale answer misleads. 3.2 records this in TD §6.3 |
| P3-Q4 | The `wait_for_agent` default and maximum timeout | 10 minutes default, 60 minutes maximum |
| P3-Q5 | How much chat history the pane loads and keeps | The last 500 lines loaded; the whole table kept |
| P3-Q6 | The wording of Duo's fixed prompt | Drafted in 3.5, approved by the author before 3.12 |
