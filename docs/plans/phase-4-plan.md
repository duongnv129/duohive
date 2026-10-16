# DuoHive Phase 4 Plan — Scale and polish (outline)

**Status:** Outline · written 2026-10-05 · expanded into a full test-first plan, in the format of [phase-2-plan.md](phase-2-plan.md), when phase 4 starts. Depends on phase 3 (Agent Squad needs task to a role).
Sources: [PRD](../product/prd.md) (Success Metrics; capability rows for templates, model and effort, key and mouse; Setup templates; UX principles) · [UI/UX spec](../ux/ui-spec.md) §4.1.1, §4.3, §4.4.1 (model fields), §4.4.2 (menu, move, drag), §4.8 (replace org), §4.10, §4.11, §5, §6 · [Technical design](../design/technical-design.md) §4.3, §5.2 (model lists), §7, §8, §10 · [Roadmap](v1-roadmap.md) row 4 · [Traceability](v1-traceability.md).

**Goal:** DuoHive meets every Success Metric at 100 agents on Apple M-series. A new user gets a ready-made org from a template. Model and effort come from each CLI. Every action works by key and by mouse, with a `?` map and editable bindings.

**Done when (roadmap):** Every benchmark target in Success Metrics (idle CPU, memory, status freshness, start-up, binary size) passes on Apple M-series.

## Constraints carried

- **Inherited:** every Global Constraint of phases 0 and 2, decisions D1–D32, P2-D1–P2-D21 and the phase 3 decisions (its P3-Q answers, recorded when phase 3 is expanded).
- **Targets** (PRD Success Metrics), gated on Apple M-series; Linux numbers are reported, not gating:

  | Metric | At 10 agents | At 100 agents |
  |---|---|---|
  | Idle CPU | < 1 % | < 3 % |
  | DuoHive memory | < 20 MB | < 60 MB |
  | Hook state to screen | < 1 s | < 2 s |
  | Screen-read (`~`) state to screen | < 2 s | < 2 s |
  | First frame | < 100 ms | < 300 ms |
  | Binary size | < 10 MB | |

- **Templates** (PRD Setup templates, UI §4.3):

  | Template | What it creates |
  |---|---|
  | Agent Squad | One Generalist role directly under Duo, with N agents (default 3, 1–100) sharing one directory and CLI, worktrees on |
  | Product Team | One team with Backend, Frontend and QA roles, one agent each, with a starter prompt per role |
  | Engineering Org | Several named teams, each with Backend and QA roles and its own repo |
  | Blank | You and Duo only |

  Everything a template creates stays editable.
- **New org from template** replaces the org after a confirmation. Running agents are stopped first, and the old `org.toml` is kept as a timestamped backup (TD §7, UI §4.8).
- **Model and effort lists** are read from each CLI at runtime, never hard-coded:
  - **Claude Code:** the aliases and `--effort` choices come from `claude --help`, and any other model is typed;
  - **OpenCode:** `opencode models` lists the models, and `opencode models --verbose` gives each model's variants as its efforts.

  While loading, the field shows `loading models…`. On failure it shows `✕ couldn't list <cli>'s models · type a name or keep inherit` (TD §5.2, UI §4.4.1).

## Scope note

- **What this phase adds beyond the roadmap's wording:** roadmap row 4 says "key and mouse completeness". This plan reads that as every remaining UI §4.11 binding (`?`, `f`, `m`), the rest of the mouse (UI §5) beyond P2-D25's wheel and left click, and Settings' Keys and Org rows (P2-D13).
- **Also assigned here:** theme `auto` and the accessibility re-check (UI §2.3, §6), as polish; no other phase owns them.
- **The three phase tables match:** roadmap row 4, PRD Delivery Phases row 4 and TD §12 row 4 list theme `auto` and the accessibility re-check, since 2026-10-05. The rest falls under their "key and mouse completeness", "the key-binding editor" and "first-run setup with templates".
- **Recorded as P4-D1:** theme `auto`, the accessibility re-check, `f`, `m` and Settings' Keys and Org rows belong to phase 4. The UI spec requires them, and no other phase owns them.

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| Full `bench`: 100 agents, `~` freshness, first frame at 10 and 100, binary size; tuning | PRD Success Metrics; TD §4.3, §10; roadmap row 4 | 4.1, 4.2 |
| First-run setup (3 steps), the no-CLI screen | PRD Setup templates, User flow step 2; UI §4.3; TD §8 (deferred by D28) | 4.3 |
| Templates: Agent Squad, Product Team, Engineering Org, Blank | PRD Must; UI §4.3 | 4.3 |
| New org from template, replace confirmation, timestamped backup | PRD The org file; UI §4.8, §4.10; TD §7 | 4.4 |
| Model and effort picker | PRD Should; TD §5.2; UI §4.4.1; replaces D14 | 4.5 |
| `?` key map; key-binding editor; bindings applied everywhere | PRD Should; UI §4.10, §4.11 | 4.6 |
| Mouse parity: right-click menu, the wheel over the org pane and Needs you, drag node, drag chat edge, header counts (wheel and click to select: P2-D25) | PRD Should; UI §4.4.2, §5 | 4.7 |
| `m` Move to… with the inherited-values diff | UI §4.4.2 | 4.8 |
| `f` filter by state | UI §4.1.1 | 4.9 |
| Theme `auto` follows the terminal; colour-blind palettes checked | UI §2.3, §4.10, §6 | 4.9 |
| Phase 4 acceptance: every target on Apple M-series | Roadmap row 4 | 4.10 |

## Tasks

| # | Task | Crates and files | Test intent | Spec |
|---|---|---|---|---|
| 4.1 | **Full benchmark.** Run at `--agents 100` (the flag already accepts 1–1000) and add the screen-read freshness path (fake agents with no hooks), first-frame timing at 10 and 100, the binary size, and a realistic-output profile (P4-Q2). The report prints every PRD target with pass or fail | `duohive/src/bench/*` | `report_lists_every_target`, `first_frame_measured`, `tilde_freshness_measured`; a bench run at 100 in CI on macOS (reported) | TD §10; PRD Success Metrics |
| 4.2 | **Tuning at 100.** Profile and fix what misses a target. Candidates: activity-poll batching, settle-read concurrency, render diffing, start-up work deferred after the first frame, `[profile.release]` size options. Each fix is measured before and after | `duohive-tmux`, `duohive-tui`, `duohive-core` as found; `Cargo.toml` | Each change lands with the bench numbers before and after in `bench-results.md` "Phase 4" | TD §4.3, §13 |
| 4.3 | **First-run setup.** Step 1: your name and Duo's CLI from those detected, with versions and `(-)` for missing. Step 2: the templates with a live preview. Step 3: each template's form (Agent Squad 1–100, directory `✓ git repo`, CLI, worktrees on, role prompt; Product Team; Engineering Org rows with `+ Add team`; Blank). `esc` on step 1 asks before quitting; on later steps it goes back. The no-CLI blocking screen. Template builders live in core as pure functions | `duohive-core/src/templates.rs`; `duohive-tui/src/first_run/*`; `duohive/src/startup.rs` (no `org.toml` → first run) | Core: `squad_has_n_agents_under_root_role`, `squad_worktrees_on`, `product_team_three_roles_with_prompts`, `engineering_org_skips_empty_rows`, `templates_validate`. TUI snapshots of every step at 80×24 and 160×48; `no_cli_blocks` | PRD Setup templates; UI §4.3; TD §8 |
| 4.4 | **New org from template.** The Settings Org row opens step 2 of first run. Confirm with UI §4.8's `Replace your org?` (focus on Cancel). Running agents are stopped first; the backup is written as `org.toml.<timestamp>.bak` | `duohive-core` `Command::NewOrgFromTemplate`; `duohive-store/src/org_file.rs`; TUI settings | `replace_stops_agents_then_writes`, `timestamped_backup_kept`, `cancel_changes_nothing` | PRD The org file; TD §7; UI §4.8 |
| 4.5 | **Model and effort picker.** Adapters list models and efforts asynchronously after the first frame, cached per CLI version. Form fields become dropdowns (`▾`) with `inherit: <v>`; `loading models…`; on failure, typing a name | `duohive-core/src/adapter.rs` (`list_models`); `duohive-cli/src/{claude,opencode}.rs`; `duohive-tui/src/forms/*` | Adapter tests parse recorded `claude --help` and `opencode models --verbose` fixtures; `loading_then_list`, `failure_allows_typing`, form snapshots | PRD Should; TD §5.2; UI §4.4.1 |
| 4.6 | **Keys.** `?` opens the key map (three columns at 100 wide or more, one scrolling column below). The key-binding editor (Settings → Keys): `enter` rebinds by pressing the new key, `backspace` restores the default, and conflicts are refused with the clashing action named. `settings.keys` is applied to every binding, the footers and the map | `duohive-tui/src/{keymap.rs, render/help.rs, render/keys_editor.rs}`; `input.rs` reads bindings | `help_lists_current_bindings`, `rebind_applies_everywhere`, `conflict_refused`, `typing_and_forms_unaffected`; snapshots at 160×48 and 80×24 | UI §4.10, §4.11; PRD Should |
| 4.7 | **Mouse.** Builds on P2-D25 (mouse reporting, the wheel, click to select and focus, already in phase 2). Right-click opens the node menu (every action with its key; disabled actions give the reason); the wheel over the org pane and Needs you; dragging a node moves it, highlighting valid targets and refusing invalid ones with the reason; dragging the chat edge resizes; clicking a header count filters | `duohive-tui/src/{mouse.rs, render/menu.rs}`; (mouse capture is on since P2-D25) | One test per gesture with synthetic `MouseEvent`s; `invalid_drop_refused_with_reason`; `typing_mode_right_click_and_drag_ignored` | UI §4.4.2, §5; PRD Should |
| 4.8 | **Move to….** `OrgEdit::Move`; the picker lists valid targets and the inherited values that change (`worktrees off → on`) before you confirm; drag and drop shares it | `duohive-core/src/engine/org_edit.rs`; `duohive-tui/src/forms/move_to.rs` | `move_lists_inherited_changes`, `move_running_agent_applies_next_start`, `invalid_targets_absent` | UI §4.4.2 |
| 4.9 | **Filter, theme and accessibility.** `f` cycles `◆ need you`, `● working`, `✓ finished`, off, with a chip in the header; `esc` clears it. Theme `auto` detects the terminal background (P4-Q3). The colour-blind palettes are checked (deuteranopia, protanopia) and the contrast table re-verified | `duohive-tui/src/{filter.rs, theme.rs}` | `filter_keeps_matches_and_parents`, `esc_clears_filter`, `auto_theme_from_background_reply`, `contrast_ratios_meet_4_5` | UI §2.3, §4.1.1, §6 |
| 4.10 | **Acceptance.** Run the full bench at 10 and 100 on Apple M-series (three runs each); every target passes; record in `bench-results.md`; mark roadmap row 4 and the PRD row done; check TD §12 | docs | All targets pass in three runs out of three | Roadmap row 4 |

**Order:** 4.1, then 4.2 (it can repeat after any task). 4.3 → 4.4. 4.5 to 4.9 are independent. 4.10 is last.

## Risks

| Risk | Response |
|---|---|
| A target misses at 100 agents (CPU or `~` freshness) | 4.2 tunes with numbers; the S0 fallback (pause unselected panes harder, read on settle only) stays available (TD §13) |
| `claude --help` changes its format | Recorded fixtures per pinned version; on a parse failure the field falls back to typing a name |
| Mouse capture breaks text selection in some terminals | The terminal's bypass modifier (`⌥`-drag in iTerm2) keeps its own selection, since P2-D25; noted in the key map |
| Templates' starter prompts are opinionated | Wording approved by the author (P4-Q1); everything stays editable |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P4-Q1 | The wording of the template starter prompts (Generalist, Backend, Frontend, QA) | Drafted in 4.3; approved by the author |
| P4-Q2 | The bench's "realistic output" profile | Replayed recorded CLI output at its recorded pace, plus a spinner burst every 2 s |
| P4-Q3 | How `auto` detects the background | OSC 11 query with a 100 ms timeout; dark on no answer |
| P4-Q4 | The mouse while the agent pane is in Typing mode | The wheel is settled by P2-D25 (scrolls DuoHive's snapshot, or passes to the agent when its CLI enables mouse reporting). Clicks: P2-D25 keeps them for DuoHive. Right-click and drags in Typing mode: ignored |
| P4-Q5 | The timestamped backup's name (TD §7 says only "timestamped") | `org.toml.<YYYYMMDD-HHMMSS>.bak` beside `org.toml` |
