# Pre-launch UX check

Task 5.1 of the [phase 5 plan](phase-5-plan.md). The PRD (UX checks) asks for a second check of the phase 3–4 screens against the six UX principles before launch. This file records what the tests establish and what only a person can judge.

**Checked 2026-10-23 by tests** (`crates/duohive-tui/src/render/prelaunch_tests.rs`, with the older `a11y_tests`, `menu_tests` and the form and wizard snapshots): every screen below is drawn at 80×24, 100×30 and 160×48 in the `dark`, `light`, `auto` and no-colour palettes. Nothing overflowed, nothing drew blank, and the no-colour palette drew with no colour in any cell. **The matrix found no failures, so no screen was changed.** The checks that need eyes on a real terminal are listed at the end for the author's walk (phase 4 Task 4.10 step 3, done once for both phases).

Principles (PRD, UX principles): **P1** one glance answers "who needs me?" · **P2** colour is never the only signal · **P3** works at 80×24 · **P4** the same key means the same thing everywhere · **P5** nothing important is more than one key or click away · **P6** calm by default.

`T` = covered by the named tests · `H` = needs a human look · `n/a` = the principle does not apply to the screen.

| Screen | P1 | P2 | P3 | P4 | P5 | P6 |
|---|---|---|---|---|---|---|
| Main screen (org, Needs you, header) | T `spec_model` snapshots, header count | T `state_never_colour_only`, `no_color_draws_without_any_colour` | T `prelaunch_screens_fit_every_size` | H | T `every_action_has_a_click_path` | T `reduce_motion_new_screens` |
| Filter chip | T `filter` snapshots (chip text and count) | T `no_color_new_screens_readable` | T matrix | H | T `filter` key tests | T `reduce_motion_new_screens` |
| Settings (incl. Keys and Org rows) | n/a | T no-colour matrix | T matrix | H | T `settings_tests` | H |
| Key map (`?`) | n/a | T `no_color_new_screens_readable` | T matrix | T `every_action_has_key` | T `every_action_has_key` | T `reduce_motion_new_screens` |
| Key-binding editor | n/a | T `no_color_new_screens_readable` | T matrix | H | T `keymap_tests` | T `reduce_motion_new_screens` |
| New chooser (`n`), Give work, Edit form | n/a | T no-colour matrix | T matrix | H | T `give_work_tests`, form tests | H |
| Model and effort dropdown | n/a | T `dropdown_*` snapshots | T `dropdown_*_80x24` | H | T dropdown key tests | H |
| Worktrees | n/a | T no-colour matrix | T matrix | H | T `worktrees_tests` | H |
| Move to… | n/a | T `no_color_new_screens_readable` | T matrix | H | T `move_tests` | T `reduce_motion_new_screens` |
| Right-click menu and drag | n/a | T `no_color_menu_and_drag_readable` | T `menu_stays_on_screen_at_the_edges` | H | T `every_action_has_a_click_path` | T `reduce_motion_menu_and_drag` |
| First run: you, template, form | n/a | T no-colour matrix | T matrix | H | T `first_run` tests | H |
| First run: no CLI found | n/a | T `the_wizard_names_what_is_missing_when_no_cli_is_found` | T matrix | n/a | T (UI §4.3: the screen names both CLIs) | H |
| tmux missing or too old (UI §4.12) | n/a | T `tmux_screen_fits_every_size` | T same | n/a | T (names the version needed) | H |
| `Replace your org?` and the replace wizard | n/a | T no-colour matrix | T matrix | H | T `replace_*` tests | T `reduce_motion_new_screens` |
| Duo chat and approval cards | T `card_tests`, Needs you rows | T `colour_tests` | H | H | T `card_tests` | H |

## What only a person can judge (the author's walk)

1. **P4, everywhere:** `enter`, `esc`, `tab` and `?` behave the same in every pane, form and dialog. The key tests cover each screen alone; the walk checks them together.
2. **P6, calm:** nothing flashes or animates except the working spinner; desktop alerts fire only when an agent needs you or finishes. `reduce_motion` is tested; the real feel is not.
3. **Duo chat and cards at 80×24:** the main screen's chat pane with a long conversation and several cards. The matrix draws the main screen, but not a long chat.
4. **The three themes on a real terminal:** `auto` follows the terminal's own colours; only a terminal shows whether they read well.
5. **Screenshots for the README** (`TODO(author)` in the README): a recording or a screenshot of the first team.
