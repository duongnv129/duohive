# Directory suggestions and the Own prompt box — Implementation Plan

**Status:** DSP.1 to DSP.6 built on the branch `feat/dir-suggest` (2026-11-13), not merged · DSP.7's hand check is open (for the author). Plan 2026-11-13. Approved by the author 2026-11-13: directory suggestions as a popup under the field (option A), line-aware `↑↓` in the Own prompt (option A), and a prompt box that grows to 4 rows.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox syntax.

**Goal:** Directory fields suggest folders as you type, and `↑↓` in a form's Own prompt move between its lines and leave only from the first or last line.

**Architecture:** `DirProbe` (a core port) gains `list`. A pure `dir_suggest` module in `duohive-tui` turns typed text into popup rows and holds the popup's state; `forms::Form` and the first-run wizard each own one per directory field and refresh it after a key or paste, as they already call `probe_dir`. The Model/Effort popup drawing is generalised and reused. `LineEdit` learns wrapped rows, and the Own prompt field draws and navigates them (1–4 rows).

**Specs:** [PRD *Should* row](../product/prd.md) · [UI §4.4.1 *Moving*, *Directory suggestions*](../ux/ui-spec.md) · [TD §3.1 *Directory suggestions*, *Own prompt vertical keys*](../design/technical-design.md).

## Global Constraints

[AGENTS.md](../../AGENTS.md) and [Phase 0's](phase-0-plan.md#global-constraints): `make check` before every commit; conventional commits, no `Co-Authored-By`, never push; no `unwrap`/`expect` outside tests; test first; files under 800 lines (`forms/mod.rs` is at 768, so new `Form` code goes in new files) and functions under 50; `duohive-core` does no I/O (only `FsDirProbe`, in the binary, reads directories); no `std::env::set_var` in tests. Work on a branch: `git checkout -b feat/dir-suggest main` before the first commit.

## Decisions

| # | Decision |
|---|---|
| DSP-D1 | `DirProbe::list` has a default body returning no entries, so other implementers (`NoProbe`, bench `NoDirs`) need no change. |
| DSP-D2 | The popup state is computed by the caller after a key or paste (`refresh_suggest`), not inside `Form::handle`, which has no `DirProbe`. `handle` only moves, takes or closes rows already listed. |
| DSP-D3 | `DirSuggest` remembers the text its rows were listed for (`listed_for`) and opens only when the text differs from it: focus alone never opens it, and `esc` stays closed until the text changes again. `reset(text)` (form open, focus change) closes it and sets `listed_for`. |
| DSP-D4 | Taking a row gives the field `<typed parent><name>/`, so the next `refresh` lists that folder's children (text changed). **Superseded 2026-11-14 for `enter` (DSP-D8):** `tab` still does this; `enter` picks without the slash and without re-listing. |
| DSP-D8 | `enter` on a folder picks it: the field gets `<typed parent><name>` (no `/`), the popup closes, and `DirSuggest` remembers that text as already listed, so it reopens only when the text changes (typing `/` lists the folder's children). A click on a row presses `enter`, so it picks. Decided by the author 2026-11-14 (option A). |
| DSP-D5 | Rows show the typed parent, not an expanded one (`~/code/app`, not `/Users/…/code/app`); `git` follows in `fg.muted`. |
| DSP-D6 | The Own prompt's row count is derived from its text and the box's width on every draw; nothing is stored. A click on any of its rows focuses it. |
| DSP-D7 | Give work's "Run in" is a choice, not a path field, and gets no popup; the team form has no Directory field. The popup is in the agent and role forms and first-run step 3. |

## Review Focus

1. A path typed as `~`, `~name/…`, `/`, a relative path, or text with spaces: no popup, no panic, the `✓` / `✕` note unchanged (DSP.2).
2. A folder with thousands of entries: `list` reads at most 500 and returns at most 50; the popup still shows 6 rows and `… N more` (DSP.1, DSP.3).
3. Wide (CJK) and long names: rows are clipped inside the popup and the popup stays on screen; when it can't fit under the field it is not drawn, as the Model popup (DSP.3).
4. A pasted path opens the popup once; a pasted path with a line break becomes spaces first (DSP.3).
5. Prompt text ending in `\n`, an empty first line, a single word longer than the box, and wide characters at the wrap edge: `↑` from an empty first line leaves, the trailing empty row exists, the cursor never lands inside a grapheme (DSP.5).

---

### Task DSP.1: `DirProbe::list`

**Files:** modify `crates/duohive-core/src/validate.rs`, `crates/duohive-core/src/testkit.rs` (`FakeDirs`), `crates/duohive/src/real_ports.rs` (`FsDirProbe`); tests beside each.

**Interfaces:**
- Produces: `pub struct DirEntry { pub name: String, pub is_git: bool }` (`Clone, Debug, PartialEq, Eq`); `DirProbe::list(&self, parent: &Path, prefix: &str) -> Vec<DirEntry>` (default `Vec::new()`); `pub const LIST_READ_MAX: usize = 500; pub const LIST_RETURN_MAX: usize = 50;`; `FakeDirs::with_listing(self, parent: impl Into<PathBuf>, names: &[(&str, bool)]) -> Self` (canned listings; `list` applies the same prefix rule).

- [ ] Tests, red, in `real_ports.rs` (a temp dir): `list_returns_folders_matching_prefix_ignoring_case_sorted`, `list_skips_files_and_hidden_unless_prefix_starts_with_dot`, `list_follows_symlinked_folders`, `list_marks_git_repos` (`.git` dir and `.git` file), `list_reads_at_most_500_and_returns_at_most_50` (600 folders), `list_of_missing_or_unreadable_parent_is_empty`. In `testkit`: `fake_listing_filters_by_prefix`.
- [ ] Implement `FsDirProbe::list` with `std::fs::read_dir(parent).take(LIST_READ_MAX)`, `file_type` via `metadata` (follows symlinks), sort by lowercase name, truncate to `LIST_RETURN_MAX`; every error gives `Vec::new()`.
- [ ] `make check`; commit `feat: list folders through the directory probe`.

### Task DSP.2: The suggestion logic

**Files:** create `crates/duohive-tui/src/dir_suggest.rs` (+ `mod dir_suggest;` in `lib.rs`), tests in `dir_suggest/tests.rs`.

**Interfaces:**
- Consumes: `DirProbe::list`, `DirEntry` (DSP.1); the existing `expand_home(&str, &Path) -> PathBuf` used by `Form::probe_dir`.
- Produces:
  - `pub struct SuggestRow { pub path: String, pub label: String, pub is_git: bool }` (`path` = typed parent + name + `/`; `label` = typed parent + name; the drawer adds the muted `   git` when `is_git`).
  - `pub struct DirSuggest { pub open: bool, pub cursor: usize, pub rows: Vec<SuggestRow>, /* private */ listed_for: String }` (`Clone, Debug, Default, PartialEq, Eq`).
  - `DirSuggest::reset(&mut self, typed: &str)`, `refresh(&mut self, typed: &str, dirs: &dyn DirProbe, home: &Path)`, `close(&mut self)`, `step(&mut self, down: bool)` (clamped, no wrap), `take(&mut self) -> Option<String>` (the highlighted row's `path`; closes).
  - `pub fn split_typed(typed: &str) -> Option<(&str, &str)>`: `None` unless the text starts with `~/` or `/`; otherwise (everything through the last `/`, the rest).

- [ ] Tests, red: `split_typed_needs_tilde_slash_or_slash` (`~`, `~bob/x`, `rel/x`, `` → `None`; `~/co` → `("~/", "co")`; `/` → `("/", "")`), `refresh_opens_with_matches_and_highlights_first`, `refresh_with_unchanged_text_does_not_reopen`, `reset_closes_and_remembers_text`, `refresh_with_no_match_closes`, `step_clamps_at_both_ends`, `take_returns_path_with_trailing_slash_and_closes`, `refresh_after_take_lists_children`, `rows_carry_the_git_mark`.
- [ ] Implement as specified; `refresh` expands the parent with `expand_home`, calls `list(parent, partial)`, and sets `open = !rows.is_empty()`, `cursor = 0`, `listed_for = typed`.
- [ ] `make check`; commit `feat: directory suggestion state`.

### Task DSP.3: Suggestions in the forms

**Files:** modify `forms/field.rs` (`pub suggest: DirSuggest` on `Field`), `forms/mod.rs` (call `dir_key` first in `handle`; `ready` calls `reset` for a Directory field), `input.rs` (call `refresh_suggest` beside both `probe_dir` calls, lines ~596 and ~655), `render/form.rs` (draw and click rows); create `forms/dir_keys.rs`; tests in `forms/tests/dir_suggest.rs` with snapshots.

**Interfaces:**
- Consumes: `DirSuggest` (DSP.2), `FakeDirs::with_listing` (DSP.1).
- Produces: `impl Form { pub(super) fn dir_key(&mut self, a: Action, org: &Org) -> Option<FormOutcome>; pub fn refresh_suggest(&mut self, dirs: &dyn DirProbe, home: &Path) }`. `dir_key` acts only when the focused field is `Directory` with `suggest.open`: `Esc` closes (outcome `Continue`), `Up`/`Down` step, `Tab`/`DialogEnter` take (sets the edit text, `touched`, `revalidate`), any other key returns `None`. `refresh_suggest` closes every other field's popup and refreshes the focused Directory field's. The drawn popup uses a shared `popup(buf, area, at, labels: &[String], cursor, hint, right, p)` extracted from `draw_popup`; the hint is `↑↓ · tab or enter take · esc close`.

- [ ] Tests, red (names): `typing_a_path_opens_the_popup`, `opening_the_form_or_focusing_the_field_does_not_open_it`, `down_then_tab_takes_and_lists_children`, `enter_takes_when_open_and_moves_on_when_closed`, `esc_closes_the_popup_then_cancels_the_form`, `up_down_leave_the_field_when_the_popup_is_closed`, `relative_path_gets_no_popup`, `paste_opens_the_popup_once` (a paste with `\n` becomes spaces), `popup_closes_when_focus_leaves`, `clicking_a_row_takes_it`, `taking_a_row_updates_the_check_note` (`✓ git repo`); screens `directory_popup_under_field` and `directory_popup_caps_at_six_rows_with_more_line` (snapshots at 160×48), `popup_not_drawn_when_it_cannot_fit_below` (small screen).
- [ ] Implement per the interfaces; keep `forms/mod.rs` under 800 lines.
- [ ] `make check`; commit `feat: suggest folders in directory fields`.

### Task DSP.4: Suggestions in first-run step 3

**Files:** modify `first_run/mod.rs` (`FirstRun.suggest: DirSuggest`), `first_run/steps.rs` (`key`, `paste`, focus changes), `render/first_run.rs` (draw the popup under the focused directory box, reusing `popup` from DSP.3 and recording click rows); tests in `first_run/tests.rs`.

**Interfaces:**
- Produces: `fn focused_dir(w: &FirstRun) -> Option<&LineEdit>`: Agent Squad focus 1; Product Team focus 2..=4; Engineering Org the even focus indexes up to `2 * rows` (the repo column); else `None`. `key` and `paste` end with `w.suggest.refresh(focused text, dirs, home)` when a directory box has focus; any focus change calls `reset` (including the Engineering `+ Add team` row move).

- [ ] Tests, red: `squad_directory_popup_opens_and_tab_takes`, `product_third_directory_popup`, `engineering_repo_column_popup_not_name_column`, `esc_closes_popup_before_going_back`, `focus_change_closes_the_popup`, `add_team_row_resets_the_popup`, `popup_does_not_open_on_focus_alone`; a snapshot of the squad popup at 80×24.
- [ ] Implement; `esc` with the popup open closes it and does not call `back`.
- [ ] `make check`; commit `feat: suggest folders in first-run directories`.

### Task DSP.5: Wrapped rows in `LineEdit`

**Files:** modify `forms/line_edit.rs` (stays well under 800), tests in `forms/line_edit_tests.rs`.

**Interfaces:**
- Produces: `LineEdit::wrap(&self, cells: usize) -> Vec<std::ops::Range<usize>>` (grapheme ranges, one per row: break at `\n` (the `\n` belongs to the row it ends), else after the last space that fits, else mid-word at the last grapheme that fits; wide graphemes never split; text ending in `\n` gives a final empty row; empty text gives one empty row); `line_up(&mut self, cells: usize) -> bool` and `line_down(&mut self, cells: usize) -> bool` (move to the row above or below at the remembered display column, clamped to that row's length; `false` and no move on the first or last row); `window_rows(&self, cells: usize, rows: usize) -> (Vec<String>, (usize, usize))` (at most `rows` rows scrolled to keep the cursor row in view, no `↵`, and the cursor's `(row, column)` within them). A private `want_col: Option<usize>` is set by vertical moves and cleared by every other edit or move.

- [ ] Tests, red: `wrap_breaks_at_newline`, `wrap_breaks_at_last_space_that_fits`, `wrap_long_word_mid_word`, `wrap_never_splits_wide_graphemes`, `wrap_trailing_newline_gives_empty_last_row`, `wrap_empty_text_is_one_row`, `line_up_down_keep_column_across_a_short_row`, `line_up_on_first_row_returns_false_and_stays`, `line_down_on_last_row_returns_false_and_stays`, `horizontal_move_clears_remembered_column`, `window_rows_scrolls_to_cursor_row`.
- [ ] Implement per the interfaces. `make check`; commit `feat: wrapped rows and vertical moves in the line editor`.

### Task DSP.6: The Own prompt box and its keys

**Files:** modify `forms/field.rs` (`Field::height(&self) -> u16`, `Field::prompt_cells(&self) -> usize` = `Look::inner` minus the `Own: ` prefix width), `forms/mod.rs` (`handle` offers `Up`/`Down` to a focused Prompt first); create `forms/prompt_keys.rs` (`impl Form { pub(super) fn prompt_key(&mut self, a: Action) -> Option<FormOutcome> }`: `Up`/`Down` return `Some(Continue)` when `line_up`/`line_down` moved, else `None` so the existing `step` runs), `render/prompt_box.rs` (draws rows 2..n of a Prompt field, called from `draw_fields`; row 1 stays in `draw_control`); modify `render/form.rs` (`rows()` pushes `height() - 1` continuation rows after the field's row; `record_field` records every row of the box); tests in `forms/tests/prompt.rs`, snapshots.

**Interfaces:**
- Consumes: `LineEdit::wrap`, `line_up`, `line_down`, `window_rows` (DSP.5).
- Produces: `Field::height` is `1` for every field except Prompt, where it is `min(wrap(prompt_cells).len(), 4)` and at least 1.

- [ ] Tests, red: `up_inside_a_prompt_moves_the_cursor_not_the_field`, `up_on_the_first_line_goes_to_the_previous_field_from_any_column`, `down_on_the_last_line_goes_to_the_next_field`, `one_line_prompt_arrows_leave_as_before`, `a_wrapped_long_line_counts_as_lines`, `tab_and_shift_tab_always_leave_the_prompt`, `up_from_an_empty_first_line_leaves`; screens (160×48): `prompt_one_row_form_unchanged` (the existing snapshots must not change), `prompt_grows_to_two_rows`, `prompt_caps_at_four_rows_and_scrolls`; `form_with_four_row_prompt_keeps_buttons_at_80x24`; `click_on_a_later_prompt_row_focuses_it`.
- [ ] Implement; the cursor position comes from `window_rows`. No `↵` is drawn in a prompt box any more (`head` and `window` remain for the one-line fields).
- [ ] `make check`; commit `feat: grow the own prompt box and move between its lines with the arrow keys`.

### Task DSP.7: Documents and hand check

**Files:** modify `docs/plans/v1-traceability.md` (a *Should* row for the PRD line, v1.1, tasks DSP.1–DSP.6), `docs/README.md` (a link to this plan in the *Roadmap and plans* row), `CHANGELOG.md` (an *Unreleased* entry for both changes), this file's **Status**.

- [ ] Hand check by the author (not an agent): type `~/co` in an agent's Directory and go down two levels; paste a path; in a three-line Own prompt press `↑` and `↓` through it and out of both ends; open Agent Squad in first run and use the popup; look at an 80×24 terminal.
- [ ] `make check`; commit `docs: plan, traceability and changelog for directory suggestions`.
