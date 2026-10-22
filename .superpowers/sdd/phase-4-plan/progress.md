# SDD ledger — plan: docs/plans/phase-4-plan.md

Branch: feat/phase-4-bench (cut from main 6162833)
Executor: inline (executing-plans)
Note: skill helper scripts blocked by sandbox ("Code from External"); ledger is kept by hand.

## Pre-flight scan

| Producer | Produces | Consumer | Consumes | Finding |
|---|---|---|---|---|
| 4.6 | `Keymap`, `ActionId`, `Action::Move/Filter/KeyMap` | 4.8 | `ActionId::Move`, handler registered in 4.6 | Consistent: 4.6 adds no-op handlers, 4.8 fills Move |
| 4.6 | `Action::Filter` no-op | 4.9 | `ActionId::Filter` handler | Consistent |
| 4.6 | `SettingsRow::Keys` (ALL `[Self; 7]` → 8) | 4.4 | `SettingsRow::Org` | Consistent; 4.4 grows ALL from whatever 4.6 left |
| 4.3 | `TemplateInput`, `build_tree`, `first_run_org`, `Command::CreateOrg`, wizard | 4.4 | `build_tree`, `TemplateInput`, wizard replace mode | Consistent: 4.4 uses `build_tree`, not `first_run_org` |
| 4.5 | `FieldKind::Dropdown` | 4.7 | dropdown click path | Consistent |
| 4.8 | `valid_targets` | 4.7 | drag/drop targets | Consistent |
| 4.9 | `Filter` | 4.7 | header count click | Consistent |

Pre-flight: no blocking conflicts.

## Tasks

Task 4.1: Ruling: the load gate would discard every run on a busy machine (the dev host sits at load ~3, and CI runners are busier) — added `DUOHIVE_BENCH_IGNORE_LOAD` (constant `IGNORE_LOAD_VAR`) that only tests and the informational CI job set; `make bench-gate` never sets it — cost if wrong: someone could disable the gate by hand, which the gate script never does.
Task 4.1: Ruling: the plan shuts the first system down "after the turns"; I shut it down after the idle window instead — the idle window is measured on the first system and the sessions survive `shutdown()` (that is what the reopen needs) — cost if wrong: the reopen loses nothing it measures.
Task 4.1: Ruling: the reopen is timed from just before the second `start_system` to the first complete draw `run_app_on` reports through its frame sender, rather than `SystemOptions.first_frame` — that sender fires on the complete draw the plan names — cost if wrong: the span could start a few microseconds earlier than the plan's point.
Task 4.1: Ruling: the `duohive mcp` helpers are spawned before the turns so their resident size has settled by the end-of-window sample, and killed after it — cost if wrong: the reported helper figure is a settled one, which is the conservative reading.
Task 4.1: Ruling: `duohive --version` did not exist, so `Cli::Version` was added (`-V` too) — `exec_once` runs it and takes its wall time; a non-zero exit reports `None` — cost if wrong: a missing exec figure, which the report prints as absent, not passed.
Task 4.1: Ruling: the gate script runs a tier's three counted runs and then its informational run, and reads the JSON with `sed` (no `jq` on stock macOS) — cost if wrong: key order assumptions break with serde_json's `preserve_order`, which the crate does not enable.
Task 4.1: Ruling: `run_app_on`'s first frame is measured on a `TestBackend` with `NoPaneView` and a local `NoDirs` probe (duohive-tui's `NoProbe` is private) — cost if wrong: the probe answers `Found` for every directory, which no target depends on.
Task 4.1: complete (commits 6162833..ff9bd21, tests: make check → pass, exit 0)
Task 4.6: Ruling: the 80×24 compact org footer keeps its short string (`tab pane · ↑↓ move · space give work`) and the `? help` hints appear only in the full footers — UI §4.1's compact mock shows no hint and the full org footer needs about 85 columns; `help_hint_in_header_and_org_needs_you_footers_80x24` checks the header at 80×24 and the footers at 120×36 — cost if wrong: at 80 columns the org footer doesn't advertise `?`, but the header does.
Task 4.6: Ruling: the binding editor is 76 columns rather than the mock's 72 — the conflict message `s is already Start / stop · rebind that first` is 45 cells and the 72-wide box cut it at `firs`; 76 keeps it whole at 80×24 — cost if wrong: two more cells of margin than the mock, and at 80×24 the pane behind shows 2 columns each side.
Task 4.6: Ruling: the header drops `? help` whole when the remaining width can't hold it — at 50 columns it left a dangling ` ·`, and UI §4.11 says the header carries the hint rather than a fragment of it — cost if wrong: a narrow header has no hint, which the narrow footer hint in the key map covers.
Task 4.6: Ruling: the ignored-settings notices draw above the editor footer (the list shrinks to keep room), not inside the scrolling list as UI §4.10's parenthetical said; §4.10 was corrected to match — cost if wrong: many bad entries shrink the list instead of scrolling a combined list.
Task 4.6: complete (commits ff9bd21..36abd3e, tests: make check → pass, exit 0)
Task 4.2: Ruling: no tuning is done now — the 4.1 gate run at 100 agents listed no target misses, and a real gate run on this host exits 3 because the 1-minute load is above the method's ceiling of 2; the task repeats after 4.5 (model-list load) and Task 4.10 runs the acceptance gate on a quiet machine — cost if wrong: a miss that only shows under a quiet run is attributed to 4.5/4.10 rather than tuned here.
Task 4.3: Ruling: Blank's step-3 focus starts on Create org, not Back — with no fields, the first control would be Back, so `enter` on entering the form would go back to the template list; UI §4.3 shows the buttons in that order, not the initial focus — cost if wrong: one `tab` less for a user who wanted Back.
Task 4.3: Ruling: an unfocused wizard field shows the text's start (`LineEdit::head`), while the focused one scrolls to the cursor — the built prompt is wider than its box, and scrolling an unfocused box to the end clipped `before` to `efore` (also a `typos` failure); the UI §4.3 sample now shows the same — cost if wrong: an unfocused box hides the end of a long value, which focusing reveals.
Task 4.3: complete (commits 8467aa4..e5483dd, tests: make check → pass, exit 0)
Task 4.4: Ruling: `PendingReplace` stores the built and validated `Org` rather than the `TemplateInput` — a bad template is rejected before anything is stopped, and apply needs no rebuild — cost if wrong: none visible; the plan's field list differs.
Task 4.4: Ruling: pre-flight refusals (replacing, reconnecting, broken org file, worktree being created, bad template) use `CommandRejected` with the existing reason strings, and `OrgReplaceFailed` is for a stop that did not finish — the wizard already shows a rejection under its fields — cost if wrong: a refusal shows as a form error, not the banner.
Task 4.4: Ruling: `timestamped_backup_kept` is tested at store level (`org_file`), and `cancel_changes_nothing` in the TUI (nothing is sent) — the engine never sees a cancel — cost if wrong: none.
Task 4.4: Ruling: the replace wizard has an empty CLI inventory (detection runs only at first run); `available()` treats an empty inventory as unknown, so both CLIs stay selectable — cost if wrong: a missing CLI can be picked and fails at launch.
Task 4.4: complete (commits e5483dd..8a919c9, tests: make check → pass, exit 0)
Task 4.5: Ruling: spike S4 on OpenCode 2.1.1 (installed; the plan's 2.0 is not on this machine) — `opencode models --verbose` is rejected (`Unrecognized flag: --verbose`); plain `opencode models` lists 381 `provider/model` ids. The fixture is `opencode-models.txt` (plain), `parse_opencode_models` returns models with empty `efforts` and `variants`, and OpenCode's Effort field accepts typed text (the plan's `empty_list_allows_typing` path). Tests `opencode_models_verbose_gives_variants_as_efforts` and `opencode_variants_map_to_hash_effort` become `opencode_models_lists_ids_with_no_variants` and a launch-side check that an existing `model#effort` value still launches — TD §5.2 says so — cost if wrong: no effort dropdown for OpenCode until a build that lists variants is checked.
Task 4.5: Ruling: the Model and Effort fields stay text fields with a popup (FieldKind::Dropdown, popup state on Field, lists on Form) rather than a separate DropdownState inside the kind — typing is the filter and the value, so the existing text paths and tests stay valid — cost if wrong: none visible.
Task 4.5: Ruling: CLIs that are not installed answer with an Err list, so their fields don't say 'loading models…' forever — cost if wrong: none.
Task 4.5: complete (commits 8a919c9..976a620, tests: make check → pass, exit 0; the start-up bench repeat of step 6 is Task 4.2's second pass)
Task 4.8: Ruling: MoveOption carries a 'blocked' reason (the plan's field list has none) so a role-name collision shows '<team> already has a role <role>' inside the picker and enter sends nothing — the core's refusal is mapped from validate's 'a role named X already exists in T' — cost if wrong: a reworded core message shows as the raw text.
Task 4.8: Ruling: the diff reads the first affected agent (the agent, or a moved role's first agent) and adds a 'role prompt' row for an agent move and a 'team prompt' row for a role move; a team move changes order only, so it lists no inherited values — cost if wrong: a role with no agents shows no diff.
Task 4.8: complete (commits 976a620..c78645c, tests: make check → pass, exit 0)
Task 4.9: Ruling: Duo's row always stays under a filter (it is the tree's root and the mock-up keeps it); the empty-result line shows only when no team or role is kept and Duo itself doesn't match — cost if wrong: Duo shows in a filter it doesn't match.
Task 4.9: Ruling: roll-ups on kept groups count the whole group, as the 4.1.1 mock-up shows (Payments ● 1 ◆ 1 under ◆) — cost if wrong: none.
Task 4.9: Ruling: clicking a header count to set the filter is done in Task 4.7 (mouse parity), not here — cost if wrong: none.
Task 4.9: complete (commits c78645c..d0e1092, tests: make check → pass, exit 0; no palette fix was needed, every UI §6 ratio is ≥ 4.5)
Task 4.9: note: two earlier full-gate runs hit timing flakes (e2e idle wait; a bench test saw a wall-clock jump); cargo test --workspace and a later make check both passed (exit 0).
Task 4.7: Ruling: dialog clicks are recorded by the renderers while a click draws the screen off-screen (render/hits.rs: a thread-local list of (rect, keys), on only inside keys_at), and a click replays the keys the button or row stands for — one mechanism for every dialog, with hit boxes that cannot drift from what is drawn — cost if wrong: one extra off-screen draw per click.
Task 4.7: Ruling: UiModel.org_scroll is Option<(usize, NodeKey)> (offset valid only while that node stays selected) and needs_scroll a plain offset, instead of the plan's single usize — so any selection change returns the tree to following the selection with no reset calls — cost if wrong: none.
Task 4.7: found and fixed a 4.4 bug: the template step's Next button did not advance to the form (next_step mapped Template to Template); click_first_run_steps and click_replace_dialog_buttons caught it.
Task 4.7: complete (commits d0e1092..ca0da04, tests: make check passes fmt, clippy and every unit/integration test; e2e_duo_cli_change_restarts_duo timed out once in the full gate but passed 9/9 when the duohive tests were re-run alone — load flake, see Final)
