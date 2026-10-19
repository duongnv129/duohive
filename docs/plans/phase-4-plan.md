# DuoHive Phase 4 Plan — Scale and polish (outline)

**Status:** Outline · written 2026-10-05, revised 2026-10-19 after phases 2 and 3 were done and a two-reviewer spec check (task order, measurement method, risks and the specs for the picker, key editor, filter and replace flow) · expanded into a full test-first plan, in the format of [phase-2-plan.md](phase-2-plan.md), when phase 4 starts. Depends on phase 3 (Agent Squad needs task to a role).
Sources: [PRD](../product/prd.md) (Success Metrics; capability rows for templates, model and effort, key and mouse; Setup templates; UX principles) · [UI/UX spec](../ux/ui-spec.md) §4.1.1, §4.3, §4.4.1 (model fields), §4.4.2 (menu, move, drag), §4.8 (replace org), §4.10, §4.11, §5, §6 · [Technical design](../design/technical-design.md) §4.3, §5.2 (model lists), §7, §8, §10 · [Roadmap](v1-roadmap.md) row 4 · [Traceability](v1-traceability.md).

**Goal:** DuoHive meets every Success Metric at 100 agents on Apple M-series. A new user gets a ready-made org from a template. Model and effort come from each CLI. Every action works by key and by mouse, with a `?` map and editable bindings.

**Done when (roadmap):** Every benchmark target in Success Metrics (idle CPU, memory, status freshness, start-up, binary size) passes on Apple M-series.

## Constraints carried

- **Inherited:** every Global Constraint of phases 0 and 2, decisions D1–D32, P2-D1–P2-D28, P3-D1–P3-D31 (P3-D8 sets the binary-size budget, P3-D20 the Move edit) and the phase 3 open-question answers (P3-Q).
- **Targets** (PRD Success Metrics), gated on Apple M-series; Linux numbers are reported, not gating:

  | Metric | At 10 agents | At 100 agents |
  |---|---|---|
  | Idle CPU | < 1 % | < 3 % |
  | DuoHive memory | < 20 MB | < 60 MB |
  | Hook state to screen | < 1 s | < 2 s |
  | Screen-read (`~`) state to screen | < 2 s | < 2 s |
  | First frame | < 100 ms | < 300 ms |
  | Binary size | < 10 MB | |

- **How the targets are measured** (the gate). TD §10 "Method (phase 4)" is the one definition; this is its summary. Drafted 2026-10-19 as defaults for the author to confirm when the phase starts:
  - **Machine and runs:** Apple M-series, plugged in; a run whose 1-minute load average is above 2 is discarded (the bench exits 3 with `discarded: load`). Three counted runs each at 10 and 100 agents with the live view on (figures without it are reported); the worst of the three decides, and a discarded run is retried, twice at most, before the gate fails.
  - **Idle CPU:** mean over 60 s after 30 s of settling.
  - **Memory:** DuoHive's resident size plus the tmux control client's, sampled at the end of that window; agents and the `duohive mcp` helpers excluded (the helpers' total is reported). The whole-run peak is still reported but doesn't gate.
  - **Hook state to screen:** the slowest of 100 samples. **Screen-read (`~`) state to screen:** p95 of 100 samples; only every fifth fake agent is read from the screen, so at 100 agents (20 of them) five turns give 100 samples, and `--turns` is set so that each count reaches 100.
  - **First frame:** the reopen case. The bench starts the system with N agents, stops DuoHive leaving their tmux sessions, starts it again, and times from the start of `run()` to the first complete draw into the test backend. Exec and dynamic-linking time is not included; the wall time of `duohive --version` is reported beside it.
  - **Binary size:** the file written by `cargo build --release`, where MB means 10,000,000 bytes.
- **Templates** (PRD Setup templates, UI §4.3):

  | Template | What it creates |
  |---|---|
  | Agent Squad | One Generalist role directly under Duo, with N agents (default 3, 1–100) sharing one directory and CLI, worktrees on (set on the role, not in the org defaults) |
  | Product Team | One team with Backend, Frontend and QA roles, one agent each, with a starter prompt per role |
  | Engineering Org | Several named teams, each with Backend and QA roles and its own repo |
  | Blank | You and Duo only |

  Everything a template creates stays editable.
- **What a first run writes in `[org]`:** only `cli`, which TD §7 requires: Duo's CLI chosen in step 1. Every other org default is left unset, and `[duo]` gets the name `Duo` and the same CLI. A template then sets CLI, directory and worktrees on the teams or the Generalist role it creates, never in `[org]`. Blank writes just `[org] cli` and `[duo]`.
- **New org from template** replaces the org's tree (teams, roles, agents) after a confirmation, keeping `[duo]` and the org-level defaults and prompt (P4-D3). Running agents in the old tree are stopped first; Duo keeps running; and the old `org.toml` is kept as a timestamped backup (TD §7, UI §4.8).
- **Model and effort lists** are read from each CLI at runtime, never hard-coded:
  - **Claude Code:** the aliases and `--effort` choices come from `claude --help`, and any other model is typed;
  - **OpenCode:** `opencode models` lists the models, and `opencode models --verbose` gives each model's variants as its efforts.

  While loading, the field shows `loading models…`. On failure it shows `✕ couldn't list <cli>'s models · type a name or keep inherit` (TD §5.2, UI §4.4.1).

## Author decisions

P4-D1 is recorded in the scope note below.

| # | Decision |
|---|---|
| P4-D2 | **The mouse everywhere.** Decided by the author on 2026-10-17. Task 4.7 adds clicks on dialog and form buttons, list rows (Worktrees, Settings, Move to…, pickers, the key-binding editor) and first-run steps, each doing what its key does. P2-D25's ignored clicks in dialogs, confirmations, Settings and Worktrees are phase 2 only. In every phase the wheel still does nothing there, the too-small screen ignores the mouse (wheel included), and clicks are ignored while zoomed; UI §4.1.2 and §5 say so. PRD Should is unchanged. |
| P4-D4 | **Order, estimate and who owns `[profile.release]`.** Set on 2026-10-19 after the spec check. The keymap layer (4.6) comes before the keys and screens that use it; the mouse (4.7) is last, since each click test needs its screen. The estimate becomes 3–4 weeks (roadmap row 4), since P4-D2 put the mouse on every screen. Task 4.1 owns `[profile.release]` (strip, lto, codegen-units) and the size measurement; 5.2 only gates on the result. |
| P4-D3 | **What New org from template keeps.** Decided by the author on 2026-10-17. `[duo]` (name, CLI) and the org-level defaults and prompt are kept; only the tree (teams, roles, agents) is replaced. Running agents in the old tree are stopped after the confirmation. Duo keeps running. Agent Squad's directory, CLI and worktrees on live on its Generalist role, not in the org defaults, so keeping the org defaults doesn't conflict. Product Team's and Engineering Org's CLI and worktree choices are set on each team they create, not in the org defaults. Built in 4.4; TD §7 and UI §4.8 say so. |

## Scope note

- **What this phase adds beyond the roadmap's wording:** roadmap row 4 says "key and mouse completeness". This plan reads that as every remaining UI §4.11 binding (`?`, `f`, `m`), the rest of the mouse (UI §5) beyond P2-D25's wheel and left click, and Settings' Keys row and the Org group's `New org from template…` row (P2-D13).
- **Also assigned here:** the accessibility re-check (UI §2.3, §6), as polish; no other phase owns it. Theme `auto` moved to phase 2 (P2-D26).
- **The three phase tables match:** roadmap row 4, PRD Delivery Phases row 4 and TD §12 row 4 list the accessibility re-check, since 2026-10-05 (theme `auto` moved to phase 2 on 2026-10-16, P2-D26). The rest falls under their "key and mouse completeness", "the key-binding editor" and "first-run setup with templates".
- **Recorded as P4-D1:** the accessibility re-check (theme `auto` moved to phase 2, P2-D26), `f`, `m` and Settings' Keys row and the Org group's `New org from template…` row belong to phase 4. The UI spec requires them, and no other phase owns them.

## Spec coverage

| Requirement | Source | Task |
|---|---|---|
| Full `bench`: 100 agents, `~` freshness, first frame at 10 and 100, binary size; tuning | PRD Success Metrics; TD §4.3, §10; roadmap row 4 | 4.1, 4.2 |
| First-run setup (3 steps), the no-CLI screen | PRD Setup templates, User flow step 2; UI §4.3; TD §8 (deferred by D28) | 4.3 |
| Templates: Agent Squad, Product Team, Engineering Org, Blank | PRD Must; UI §4.3 | 4.3 |
| New org from template, replace confirmation, timestamped backup | PRD The org file; UI §4.8, §4.10; TD §7 | 4.4 |
| Model and effort picker | PRD Should; TD §5.2; UI §4.4.1; replaces D14 | 4.5 |
| `?` key map; key-binding editor; bindings applied everywhere | PRD Should; UI §4.10, §4.11 | 4.6 |
| Mouse parity: right-click menu, the wheel over the org pane and Needs you, drag node, drag chat edge, header counts, clicks in dialogs, forms, lists and first run (P4-D2; wheel and click to select: P2-D25) | PRD Should; UI §4.4.2, §5 | 4.7 |
| `m` Move to… with the inherited-values diff | UI §4.4.2 | 4.8 |
| `f` filter by state | UI §4.1.1 | 4.9 |
| Colour-blind palettes checked (theme `auto` follows the terminal since P2-D26) | UI §2.3, §4.10, §6 | 4.9 |
| Phase 4 acceptance: every target on Apple M-series | Roadmap row 4 | 4.10 |

## Tasks

| # | Task | Crates and files | Test intent | Spec |
|---|---|---|---|---|
| 4.1 | **Full benchmark.** Run at `--agents 100` (the flag already accepts 1–1000). The bench already runs every fifth fake agent without hooks (`SCREEN_READ_EVERY`) but doesn't time them: add the `~` freshness timing, first-frame timing at 10 and 100 (today `first_frame` is `None`), the binary size, and a realistic-output profile (P4-Q2), all by the method in Constraints. Set `[profile.release]` (strip, lto, codegen-units) here and record the size after it (P4-D4). The bench changes the method needs: memory becomes a current-RSS sample at the end of the window plus the tmux client's (today it is the whole-run peak, `peak_rss_mb`, still reported); `--idle-secs` defaults to 60 with a 30 s settle; `--turns` defaults so 100 hook samples are taken at each count; load above the ceiling exits 3 with `discarded: load`; `scripts/bench-gate.sh` (run by `make bench-gate`) runs `duohive bench` three times, retries a discarded run at most twice, and takes the worst. Phase 5's `--gate` (5.2) moves the same three-run rule into the binary and replaces the script. The report prints every PRD target with pass or fail, and the machine load. It also reports the `duohive mcp` helpers' total memory and start time (reported, not gated), measured by spawning N real `duohive mcp` processes beside the fake agents and summing their resident size | `duohive/src/bench/*`; `duohive-cli/src/fake.rs` if the fake needs a timing hook; `scripts/bench-gate.sh`; `Makefile`; `Cargo.toml` | `report_lists_every_target`, `first_frame_measured_on_reopen`, `tilde_freshness_measured`, `helpers_memory_reported`, `memory_is_end_of_window_not_peak`, `load_above_ceiling_exits_3_discarded`, `worst_of_three_decides`; a bench run at 100 in CI on macOS (reported) | TD §10; PRD Success Metrics |
| 4.2 | **Tuning at 100.** Profile and fix what misses a target. Candidates: activity-poll batching, settle-read concurrency, render diffing, start-up work deferred after the first frame (the model lists of 4.5 load after it, and are measured here once they exist). Each fix is measured before and after; it stops when every target passes in three runs, or when a target still misses after the S0 fallback, which goes to the author | `duohive-tmux`, `duohive-tui`, `duohive-core` as found | Each change lands with the bench numbers before and after in `bench-results.md` "Phase 4" | TD §4.3, §13 |
| 4.3 | **First-run setup.** Step 1: your name and Duo's CLI from those detected, with versions and `(-)` for missing. Step 2: the templates with a live preview. Step 3: each template's form (Agent Squad 1–100, directory `✓ git repo`, CLI, worktrees on, role prompt, all set on its Generalist role, not in the org defaults; Product Team's and Engineering Org's CLI and worktree choices are set on each team they create, not in the org defaults; Product Team; Engineering Org rows with `+ Add team`; Blank, which shows a one-line summary instead of a form). `esc` on step 1 asks before quitting; on later steps it goes back. Today a first run silently builds a blank org (`system.rs`, `Loaded::FirstRun`); the wizard replaces that, and the engine starts only after Create org. The no-CLI blocking screen. Template builders live in core as pure functions | `duohive-core/src/templates.rs`; `duohive-tui/src/first_run/*`; `duohive/src/startup.rs` (no `org.toml` → first run) | Core: `squad_has_n_agents_under_root_role`, `squad_worktrees_on`, `product_team_three_roles_with_prompts`, `engineering_org_skips_empty_rows`, `templates_validate`, `template_agent_names_unique`, `templates_write_only_org_cli`, `blank_writes_org_cli_and_duo`. TUI snapshots of every step and every error state (UI §4.3) at 80×24 and 160×48; `no_cli_blocks`; `first_run_applies_name_and_duo_cli` | PRD Setup templates; UI §4.3; TD §8 |
| 4.4 | **New org from template.** The Settings Org row opens the wizard in replace mode (UI §4.3): the template step and its form, titled `of 2`, with Cancel in place of Quit. `Create org` opens UI §4.8's `Replace your org?` (focus on Cancel). Only the tree is replaced; `[duo]` and the org-level defaults and prompt are kept (P4-D3). Running agents in the old tree are stopped after the confirmation (TD §5.5's grace kill applies); if any window can't be stopped, nothing is written and a banner says which agent. Duo keeps running. The backup is written as `org.toml.<timestamp>.bak`. The old agents' worktree rows stay listed under `w`; their session rows go; pending cards and queued messages that name a removed agent are dropped with the reason `agent removed`; a template agent name that matches a kept worktree row gets the next free number (`squad-4`) | `duohive-core` `Command::NewOrgFromTemplate`; `duohive-store/src/org_file.rs`; TUI settings | `replace_stops_agents_then_writes`, `replace_keeps_duo_and_org_defaults`, `replace_keeps_duo_running`, `timestamped_backup_kept`, `cancel_changes_nothing`, `replace_aborts_when_a_stop_fails`, `replace_keeps_worktree_rows`, `replace_drops_cards_and_messages_for_removed_agents`, `template_name_skips_kept_worktree_names`, `replace_mode_back_and_cancel` | PRD The org file; TD §7; UI §4.8 |
| 4.5 | **Model and effort picker.** First a 30-minute spike (S4) of `opencode models --verbose` on the pinned OpenCode 2.0, since 2.0 already dropped `--model` and `--agent` and the fixtures can't be recorded without it; TD §5.2 is updated from it. Adapters list models and efforts asynchronously after the first frame, cached in `<data_dir>/model-cache.json` per CLI and version (TD §7). The `Choice` field only cycles with ←→, so this adds a new dropdown widget (UI §4.4.1): a popup list with type-ahead, an `inherit` entry, and a typed custom name; the Effort list is the chosen model's variants | `duohive-core/src/adapter.rs` (`list_models`); `duohive-cli/src/{claude,opencode}.rs`; `duohive-tui/src/forms/*` | Adapter tests parse recorded `claude --help` and `opencode models --verbose` fixtures, the latter re-recorded on the pinned OpenCode 2.0, confirming its variants map to `#<effort>`; `loading_then_list`, `failure_allows_typing`, `typeahead_filters_117_models`, `custom_name_accepted`, `inherit_uses_the_inherited_clis_list`, `effort_resets_when_model_changes`, `empty_list_allows_typing`, `cache_hit_per_cli_version`; form and popup snapshots | PRD Should; TD §5.2; UI §4.4.1 |
| 4.6 | **Keys.** Built first among the key tasks, as a keymap registry: every action has an id and a default key, and `input.rs`, its sub-modules and the footers (hard-coded in `render/header.rs` today) read it. The registry holds every action, `m` and `f` included, from the start; 4.8 and 4.9 build what those keys do. `?` opens the key map (three columns at 100 wide or more, one scrolling column below). The key-binding editor (Settings → Keys) follows UI §4.10: the rebindable actions, the reserved keys, the capture state and the conflict rule are defined there, and `settings.keys` follows TD §7. `? help` shows in the header and the footers of the Selected-mode screens, never in Typing mode, forms or the chat input (P2-D9) | `duohive-tui/src/{keymap.rs, render/help.rs, render/keys_editor.rs}`; `input.rs`, `input/panes.rs`, `chat.rs`, `settings.rs`, `worktrees.rs` and `render/header.rs` read it | `help_lists_current_bindings`, `rebind_applies_everywhere`, `conflict_refused_in_same_context`, `conflict_check_covers_reserved_keys`, `two_context_action_rebinds_both`, `backspace_restores_default`, `esc_cancels_capture`, `unknown_id_notice_shown`, `same_key_allowed_in_different_contexts`, `reserved_keys_refused`, `invalid_saved_key_falls_back_to_default`, `every_action_has_a_default_key`, `typing_and_forms_unaffected`, `help_hint_in_selected_mode_footers_80x24`; snapshots at 160×48 and 80×24 | UI §4.10, §4.11; PRD Should |
| 4.7 | **Mouse.** Builds on P2-D25 (mouse reporting, the wheel, click to select and focus, already in phase 2). Right-click opens the node menu (every action with its key; disabled actions give the reason); the wheel over the org pane and Needs you (it scrolls the viewport and leaves the selection where it is, which needs a scroll offset apart from the selection); dragging a node moves it, highlighting valid targets and refusing invalid ones with the reason; dragging the chat edge resizes; clicking a header count filters. Clicks on dialog and form buttons, list rows (Worktrees, Settings, Move to…, pickers, the key-binding editor) and first-run steps do what their key does (P4-D2) | `duohive-tui/src/{input/mouse.rs, render/menu.rs}`; (mouse capture is on since P2-D25) | One test per gesture with synthetic `MouseEvent`s; `invalid_drop_refused_with_reason`; `typing_mode_right_click_and_drag_ignored`; replaces three phase 2 tests: 2.21's `mouse_ignored_in_dialogs_confirmations_settings_worktrees_and_too_small` with `wheel_ignored_in_dialogs_confirmations_settings_worktrees` and `mouse_ignored_on_too_small_screen` (wheel included), `wheel_ignored_over_org_and_needs_you` with `wheel_scrolls_org_and_needs_you_without_moving_selection`, and `right_middle_click_and_drag_ignored` with `right_click_opens_menu_and_drag_moves_node` (middle-click stays ignored); `click_ignored_while_zoomed` kept; a click test per screen (`click_dialog_buttons`, `click_form_buttons`, `click_worktree_rows`, `click_settings_rows`, `click_move_to_rows`, `click_picker_rows`, `click_key_binding_rows`, `click_first_run_steps`) | UI §4.4.2, §5; PRD Should |
| 4.8 | **Move to….** Built on phase 3's `OrgEdit::Move` (Task 3.6, P3-D20), (`Move { node, to: MoveTarget }`), which already moves an agent into a role, a role into a team or the root, and a team to a position among the teams (`Before(team)` or `End`), and applies to a running agent at its next start; on a team the picker lists positions; the picker lists valid targets and the inherited values that change (`worktrees off → on`) before you confirm; drag and drop shares it | `duohive-tui/src/forms/move_to.rs` | `move_lists_inherited_changes`, `invalid_targets_absent`, `move_sends_org_edit_move` | UI §4.4.2 |
| 4.9 | **Filter, theme and accessibility.** `f` cycles `◆ need you` (waiting, quiet and errored agents, as the header roll-up counts them), `● working`, `✓ finished`, off, with a chip in the header; `esc` in the org pane clears it; an empty result and a hidden selection follow UI §4.1.1. The accessibility re-check, by tests: the contrast table (`dark` and `light`; `auto` uses the terminal's own colours), a state is never shown by colour alone (each has its own glyph and word), the `NO_COLOR` and Reduce motion renders of the new screens, and keyboard-only reach | `duohive-tui/src/{filter.rs, theme.rs}` | `filter_keeps_matches_and_parents`, `filter_need_you_includes_quiet_and_errored`, `filter_never_hides_cards`, `filter_empty_result_message`, `filter_clears_hidden_selection_to_first_match`, `esc_in_org_pane_clears_filter`, `esc_elsewhere_keeps_filter`, `contrast_ratios_meet_4_5`, `state_never_colour_only`, `no_color_new_screens_readable`, `reduce_motion_new_screens`, `every_action_has_key_and_click` | UI §2.3, §4.1.1, §6 |
| 4.10 | **Acceptance.** Run the full bench at 10 and 100 on Apple M-series by the method in Constraints (three runs each, with the live view on); every target passes; record in `bench-results.md`; mark roadmap row 4 and the PRD row done; check TD §12 | docs | All targets pass in three runs out of three | Roadmap row 4 |

**Order (P4-D4):** 4.1, then 4.2 (it repeats after any later task that adds start-up or render work). Then 4.6 (the keymap registry, which already holds `m` and `f`, and the editor), since 4.8 and 4.9 read their keys from it. Then 4.3 → 4.4, and 4.5, 4.8 and 4.9, in any order. 4.7 comes after 4.3, 4.5, 4.6, 4.8 and 4.9, since each click test needs its screen. 4.10 is last.

## Risks

| Risk | Response |
|---|---|
| A target misses at 100 agents (CPU or `~` freshness) | 4.2 tunes with numbers; the S0 fallback (pause unselected panes harder, read on settle only) stays available (TD §13) |
| `claude --help` changes its format | Recorded fixtures per pinned version; on a parse failure the field falls back to typing a name |
| The estimate: tuning at 100 is open-ended, and 4.1's bench rework (end-of-window memory, the reopen procedure, the helpers, the three-run script) is itself a small project | 4.1 and 4.2 go first so a miss shows early; if a target still misses after the S0 fallback, the author decides whether to move the target or the date |
| The bench's fake agents don't run long real-agent output (bench-results.md) | The realistic-output profile (P4-Q2) is replayed output; a one-hour run with real agents is a manual check in 4.10 |
| Binary size: 9,829,520 B today, about 170 KB under the target, with no release profile yet; this phase adds templates, help, the editor, pickers and the mouse | 4.1 sets `[profile.release]` first and records the size; every task that adds a dependency or a screen re-checks it; the P3-D8 fallback (hand-written code over a crate) applies |
| The keymap refactor touches about five input modules and the footers | 4.6 comes first, behind the registry; the existing key tests must pass unchanged with the default bindings |
| OpenCode 2.0's `models --verbose` differs from 1.18 | Spike S4 before 4.5; recorded fixtures for the pinned version; typing a name stays available |
| Replace-flow data: worktree rows, messages and cards that name removed agents | The rules in 4.4 and their tests; nothing is deleted on disk |
| 10-agent memory with the live view is within about 2 MB of 20 MB | The gate runs with the view on (Constraints); 4.2 tunes against it |
| Mouse capture breaks text selection in some terminals | The terminal's bypass modifier (`⌥`-drag in iTerm2) keeps its own selection, since P2-D25; noted in the key map |
| Templates' starter prompts are opinionated | Wording approved by the author (P4-Q1); everything stays editable |

## Open questions (decide when the phase starts)

| # | Question | Default if not decided |
|---|---|---|
| P4-Q1 | The wording of the template starter prompts (Generalist, Backend, Frontend, QA) | Drafted in 4.3; approved by the author |
| P4-Q2 | The bench's "realistic output" profile | Replayed recorded CLI output at its recorded pace, plus a spinner burst every 2 s |
| P4-Q4 | Right-click and drags while the agent pane is in Typing mode (the wheel and clicks are settled by P2-D25) | Ignored |
| P4-Q5 | The timestamped backup's name (TD §7 says only "timestamped") | `org.toml.<YYYYMMDD-HHMMSS>.bak` beside `org.toml` |
| P4-Q6 | The names of template agents (names are unique across the org) | `<team-slug>-backend`, `<team-slug>-qa` for Engineering Org; `backend`, `frontend`, `qa` for Product Team; `squad-N` for Agent Squad. These are the template defaults; the New role form's `<team>-<role>-N` (UI §4.4.1) applies only to agents made there |
