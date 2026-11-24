# The org pane as grouped boxes, a colour per team and a Manager card — Implementation Plan

**Status:** Plan, 2026-11-17, updated 2026-11-18 for the Org layout choice. The design was approved by the author (the boxes on 2026-11-17, the layout choice on 2026-11-18) and is written into the three living documents on `docs/org-boxes`. Not built.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox syntax. The specs land first: the author opens a pull request from `docs/org-boxes` and merges it. Then cut the code branch from `main` (`git checkout -b feat/org-boxes main`), as the author's git rules require, and the author pushes it.

**Goal:** The org pane has two layouts, chosen by a personal setting (first run, Settings, the key `l`): the classic tree, which stays the default for existing installs and gains team colours, and grouped boxes at 100 columns and 30 rows or more (a Manager card for Duo, a box per team holding a box per role holding the agents). Each team has a colour from a four-colour palette that is saved in `org.toml` and chosen in the team and top-level role forms, and the selected and typed-into states are marked as the spec says. Below that size both settings draw the compact tree.

**Architecture:** The core gains a pure `TeamColour` and a `color` on teams and top-level roles; the store reads and writes it. The TUI gets a pure row builder (`boxes.rs`) that turns the existing `UiModel` into box lines, a painter (`render/org_boxes.rs`, `render/manager_card.rs`) that draws them with new palette tokens, a const wordmark font (`wordmark.rs`) and a `Colour` form row. A new `OrgLayout` setting and one dispatch in `org_pane` choose between the boxes and the tree. The tree, `tree_rows` and every command stay, except that `CreateOrg` carries the first-run layout choice. The setting and its dispatch are built first, tree-only (OB.0), so the boxes land behind it.

**Tech Stack:** Rust 1.99.0 (edition 2024), ratatui and crossterm (pinned with `=`), serde and toml (store), insta snapshots (existing).

**Specs (the plan argues from these; they hold every rule, value and message text):** [UI/UX §2.1, §2.2, §2.3, §2.4, §3, §4.1, §4.1.3, §4.1.5, §4.1.6, §4.3, §4.4.1, §4.4.2, §4.6, §4.10, §4.11, §4.12, §6](../ux/ui-spec.md) · [PRD: org model, *One screen*, decisions log](../product/prd.md) · [TD §6.5, the bullet *Org boxes and the Manager card*, and §7 *Team colour*](../design/technical-design.md).

## Global Constraints

[AGENTS.md](../../AGENTS.md) and [Phase 0's](phase-0-plan.md#global-constraints) apply. In particular:

- `make check` (format check, clippy, tests, `cargo deny`, `typos`) passes before every commit. Conventional commits, no `Co-Authored-By`, never push.
- Toolchain 1.99.0, edition 2024. No `unwrap`/`expect` outside tests. No `unsafe`.
- `duohive-core` does no I/O. `duohive-tui` and `duohive-store` depend only on `duohive-core`.
- Test first, except infrastructure.
- `org.toml` tables deny unknown keys; a parse error names the 1-based line.
- Every text colour meets 4.5:1 on `dark` and `light` (UI §6). Colour is never the only signal. No hover: pointer motion is not read.
- Names are ASCII letters, digits, `-` and `_`, starting with a letter or digit, 1 to 40 characters.
- New `*_tests` modules are declared by hand: `render/mod.rs` lists every render test module, and `lib.rs` the others.

## Decisions

Only decisions that supersede older behaviour are recorded here; everything else is in the documents (the colour sibling rule in TD §7 and UI §2.3, the colour error texts in TD §7, which layout is drawn in UI §3, §4.1, §4.1.6 and §4.10, Needs you rows in UI §4.1.3 and §4.1.5, the drag marker in UI §4.4.2).

| # | Decision |
|---|---|
| OB-D1 | **One visible list.** `box_rows` returns every line of the pane, `Detail` lines included. Drawing, hit-testing, scrolling and the wheel all read it, so they cannot disagree about heights: in the boxes the row index half of `UiModel.org_scroll` (`Option<(usize, NodeKey)>`, unchanged in type) is an index into this list. The compact tree keeps `tree_rows`. |
| OB-D2 | **The last-reply setting keeps one meaning in both layouts:** show the last reply where the layout shows it (under each agent in the tree, on the selected agent's second line in the boxes). The `replies: bool` the render functions take stays `settings.show_last_reply && shows_replies(screen)` for the tree (`shows_replies` is unchanged; the size rule is UI §3); the boxes use `settings.show_last_reply` alone, since they draw at 100×30 too. The Settings row (the const `LAST_REPLY` in `render/settings.rs`) shortens to `Last reply`, so `Reduce motion`, which follows it two spaces after the checkbox, moves left, as the UI §4.10 mock draws; the right-click row (in `input/menu.rs`) reads `Hide reply` or `Show reply`. |
| OB-D3 | **The first-run choice travels in `Command::CreateOrg`** as a new field `org_layout: OrgLayout`, like `your_name`, and the engine writes it to `settings.toml`; the wizard also sets the same value in `m.settings`, because `Event::OrgCreated` does not carry settings and `UpdateSettings` sends the whole `Settings` (TD §6.5). The Replace flow (`NewOrgFromTemplate`) leaves the setting alone. |
| OB-D4 | **Switching layout is a redraw, not a model reset:** `m.selected` stays, and `m.org_scroll` is cleared so the next draw anchors on the selected node. |

## Review Focus

1. **A 100-agent squad** (Agent Squad, one role of 100): the box is taller than the pane, the selected agent and its opened second line stay fully in view as `↓` moves and as the wheel scrolls, and `box_rows` stays linear (OB.5, OB.6).
2. **Renaming Duo** to `a`, `a-`, `a_`, `7-eleven` and a 40-character name (a name cannot start with `-` or `_`, and `-bee` is refused): the card is four rows, the wordmark is 11 columns, the name is cut at 16 columns with `…` in the middle, nothing slices a string mid-character (OB.4, OB.5, OB.6).
3. **A hand-edited `org.toml`**: no `color`, an unknown colour, `color` on a role inside a team, two teams with the same colour. Each gives the specified result, and a file that loads is never rewritten until a save (OB.1, OB.2). A `settings.toml` without `org_layout` (every existing install) keeps the tree (OB.0).
4. **The size edge, resizes and layout switches:** 99 columns and 29 rows draw the compact tree under both settings, 100 columns and 30 rows the chosen layout; `l` below that size changes the setting and says why; a switch keeps the selected node and clears the scroll; a resize while Typing keeps Typing; a selected agent at the bottom edge with its second line cut off scrolls into view (OB.0, OB.6).
5. **Selection and problems in awkward places:** an agent in a folded role, under an active filter, with a very long name or last reply, stopped or errored, with a missing directory (a second line even when not selected), and a click on the second line (OB.5, OB.6).

## File structure

| File | Responsibility |
|---|---|
| `crates/duohive-core/src/colour.rs` (new) | `TeamColour`, the palette order, `next_free`, `parse`, the error text |
| `crates/duohive-core/src/org.rs`, `command.rs`, `engine/org_edit.rs`, `templates.rs`, `validate.rs` | the `color` field, its assignment and its validation |
| `crates/duohive-store/src/org_file.rs` | `color` in the file; `TopRoleFile` for roles under Duo |
| `crates/duohive-tui/src/theme.rs` | hue, tint, manager and pill tokens for `dark`, `light`, `auto`, mono |
| `crates/duohive-tui/src/wordmark.rs` (new) | the 38-glyph font and `wordmark(name)` |
| `crates/duohive-tui/src/boxes.rs`, `boxes_tests.rs` (new) | `box_rows`: the pure list of box lines |
| `crates/duohive-tui/src/render/org_boxes.rs`, `render/org_boxes_tests.rs`, `render/manager_card.rs` (new) | drawing, scrolling, the wheel and hit-testing the boxes and the card |
| `crates/duohive-core/src/settings.rs`, `command.rs`, `engine/create_org.rs` | `OrgLayout`, the setting, and the first-run choice in `CreateOrg` |
| `crates/duohive-tui/src/keymap.rs`, `input.rs`, `forms/settings.rs`, `render/settings.rs` | the key `l` and the Settings row |
| `crates/duohive-tui/src/first_run/` | the layout row in step 1 |
| `crates/duohive-tui/src/render/org_pane.rs`, `render/mod.rs` | the dispatch on `drawn_layout`, the tree's hue and selection (OB.3a); module registration |
| `crates/duohive-tui/src/render/needs_you.rs` | the no-word, no-time rows |
| `crates/duohive-tui/src/forms/{field,team,role,mod,org}.rs`, `render/form.rs` | the `Colour` row and the wordmark preview in Edit org |

---

### Task OB.0: The Org layout setting, the dispatch, the Settings row and the key

Built first and tree-only, so the boxes land behind it and the org pane still draws the tree. The Settings row, the key and the key-map entries are visible, so their snapshots change here.

**Files:**
- Modify: `crates/duohive-core/src/settings.rs` (`OrgLayout`, `Settings.org_layout`), `crates/duohive-tui/src/keymap.rs` (`ActionId::OrgLayout`), `forms/settings.rs` (`SettingsRow::OrgLayout`, `toggled`), `render/settings.rs` (the new row takes one line, so the dialog's `HEIGHT` grows from 16 to 17, or the broken-file line lands on the footer row), `render/help.rs` (the `?` key map), `render/org_pane.rs` (`drawn_layout` and the dispatch in `draw`, `key_at` and `wheel`), `input.rs` (the key)
- Test: the `settings.rs` tests, the keymap tests, the settings form tests, `render/settings_tests.rs`, `render/help_tests.rs`, `render/pane_tests.rs`, the snapshots `settings_80x24`, `settings_160x48`, `key_map_*` and every `keys_editor_*` (`keys_editor_80x24`, `_resume_80x24`, `_conflict_notice`, `_ignored_settings_line`: 18 actions no longer fit the 80×24 editor, so it scrolls with `↓ more`), and a store test in `crates/duohive-store/tests/settings_file.rs` for a `settings.toml` round trip

**Interfaces:**
- Produces: `#[serde(rename_all = "lowercase")] enum OrgLayout { Tree, Boxes }` in `duohive_core::settings` (`Clone, Copy, Debug, Default` with `Tree` as the default, `PartialEq, Eq, Serialize, Deserialize`); `Settings.org_layout: OrgLayout` with `#[serde(default)]`; `pub(crate) fn drawn_layout(setting: OrgLayout, compact: bool) -> OrgLayout` in `render/org_pane.rs` (`Boxes` only when the setting is `Boxes` and `compact` is false); `ActionId::OrgLayout` (id `org_layout`, label `Org layout`, default key `l`, placed right after `LastReply` in `ActionId::ALL`, registered in the contexts `LastReply` is, the org pane's; it is not a row of the node menus).

- [ ] **Step 1: Failing tests.** In the core: `a_settings_file_without_org_layout_means_tree`; `org_layout_round_trips_in_lowercase` (`"boxes"` and `"tree"`); `an_unknown_org_layout_is_refused_with_its_line` (as an unknown theme is). In the keymap: `org_layout_defaults_to_l`, `l_clashes_are_reported_like_any_key`, `org_layout_is_rebindable_and_listed_on_the_keys_screen`. In the settings form: `the_org_layout_row_cycles_tree_and_boxes` and `a_click_on_the_row_cycles_it`. In render: `settings_org_layout_row_reads_the_choice` (`Org layout [ boxes ▾ ]`), `the_key_map_lists_l_org_layout` (UI §4.11), `settings_row_reads_boxes_needs_100x30_while_compact`, and `drawn_layout_is_boxes_only_when_set_and_not_compact` (all four combinations). In input: `l_cycles_the_layout_and_saves_settings`, `l_below_100x30_changes_the_setting_and_says_why` (the footer reads `org layout: boxes · needs 100×30`), and `switching_layout_keeps_the_selected_node_and_clears_the_scroll` (OB-D4).
- [ ] **Step 2: Implement** the enum and the field, the action and its default key, the Settings row (UI §4.10), the key handler (cycle, save through the existing settings save path, set the footer notice when compact, clear `m.org_scroll`), and `drawn_layout` with the dispatch: `org_pane::draw`, `key_at` and `wheel` call it, and until OB.6 the `Boxes` arm calls the tree path, so behaviour is unchanged.
- [ ] **Step 3: Run** `cargo test --workspace` (pass; accept and review the changed Settings, Keys and key-map snapshots: they gain the Org layout row and the `l` entry, and nothing else changes, since `Settings::default()` is `Tree`), `make check`, commit `feat: an org layout setting, with the key l and a Settings row`.

### Task OB.1: `TeamColour` and the `color` field in the core

**Files:**
- Create: `crates/duohive-core/src/colour.rs` (declare it in `lib.rs`)
- Modify: `org.rs` (`Team` and `Role` gain `pub color: Option<TeamColour>`, with `#[serde(default, skip_serializing_if = "Option::is_none")]` if `Org` derives serde; fix every constructor the compiler lists: the `Team` and `Role` literals, and the `TeamDraft` and `RoleDraft` literals in `tool.rs` (Duo's tool carries no colour, so `color: None`), `card.rs`, `event.rs`, `testkit.rs`, `forms/team.rs`, `forms/role.rs` and many tests; a small test helper is fine where a test builds many), `command.rs` (`TeamDraft` and `RoleDraft` gain `#[serde(default)] pub color: Option<TeamColour>`), `engine/org_edit.rs`, `templates.rs`, `validate.rs`
- Test: `colour.rs` (`mod tests`), `engine/org_edit/tests.rs`, `templates.rs` tests

**Interfaces:**
- Produces: `#[serde(rename_all = "lowercase")] enum TeamColour { Teal, Violet, Slate, Orchid }` (`Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize`); `TeamColour::ALL: [TeamColour; 4]`; `fn name(self) -> &'static str`; `fn parse(s: &str) -> Result<TeamColour, ColourError>`; `fn next_free(used: &[TeamColour]) -> TeamColour`; `Org::colour_of(&self, key: &NodeKey) -> Option<TeamColour>` (a team or top-level role: its own or derived as TD §7 says; a role inside a team and an agent: the enclosing team's or top-level role's; Duo: `None`).

- [ ] **Step 1: Failing tests in `colour.rs`.** `parse_accepts_the_four_names_and_refuses_others` (`"tael"` fails with the exact text of TD §7); `serde_uses_lowercase_names` (JSON `"teal"`, not `"Teal"`); `next_free_takes_the_first_unused_then_wraps` (`&[]` → Teal; `&[Teal, Slate]` → Violet; four used → Teal; five used → Violet); `colour_of_derives_from_position_when_unset` (teams A, B and top-level role G, all `None`: Teal, Violet, Slate; a role in A and an agent in it: Teal; Duo: `None`).
- [ ] **Step 2: Implement `colour.rs` and `Org::colour_of`;** add the `color` fields and make the workspace compile (`cargo build --workspace --all-targets`). Expected: no errors.
- [ ] **Step 3: Failing engine tests.** `add_team_without_colour_takes_the_next_free`, `add_team_with_colour_keeps_it`, `edit_team_sets_the_colour`, `a_draft_without_colour_still_parses` (a persisted card's `TeamDraft` JSON with no `color` deserializes with `color: None`), `an_edit_draft_without_colour_keeps_the_current_one`, `moving_a_root_role_into_a_team_clears_its_colour`, `moving_a_role_out_takes_the_next_free` (UI §4.4.2, TD §7), `a_card_proposed_team_takes_the_next_free_colour_when_approved` (Duo's tool carries no colour), `add_root_role_counts_teams_as_siblings` (a team Teal, a new root role without colour: Violet), `a_role_in_a_team_refuses_a_colour` (refused with the TD §7 text); `an_add_keeps_the_siblings_effective_colours` (a colourless team A and root role R load as teal and violet; adding team B, which sorts before R, gives B slate and leaves R violet); `deleting_a_sibling_after_a_save_does_not_recolour_the_others` (save A and B as teal and violet, delete A, save again: B is still violet). In `templates.rs` tests: `product_team_colours_follow_the_palette_order` and `agent_squad_generalist_is_teal`.
- [ ] **Step 4: Implement** the assignment in `engine/org_edit.rs` (`AddTeam`, `AddRole` with `team: None`, `EditTeam`, `EditRole`, and `move_node`: clear a role's colour when it enters a team, give it the next free when it leaves), the refusal in `validate.rs`, and the template colours. A draft without a colour on edit keeps the current one. Before any structural edit (add, remove or move of a team or a top-level role) set every sibling's effective colour in the in-memory org (`Org::fix_colours(&mut self)`, called at the top of `apply_edit`), as TD §7 says.
- [ ] **Step 5: Run** `cargo test -p duohive-core` (all pass), then `make check`, then commit `feat: a colour per team and top-level role in the core`.

### Task OB.2: `color` in `org.toml`

**Files:**
- Modify: `crates/duohive-store/src/org_file.rs`
- Test: `crates/duohive-store/tests/org_file.rs`

**Interfaces:**
- Consumes: `TeamColour::{parse, name}`, `Org::colour_of` (OB.1).
- Produces: `[[team]]` and top-level `[[role]]` accept `color = "teal" | "violet" | "slate" | "orchid"`; a role inside a team does not.

- [ ] **Step 1: Failing tests.** `color_round_trips_on_a_team_and_a_root_role`; `color_on_a_nested_role_fails_with_its_line` (error mentions the field and the 1-based line); `an_unknown_colour_gives_the_td_text_with_its_line`; `an_existing_org_toml_without_color_still_loads` (the pre-change fixture files load and every colour is derived), `a_file_without_color_loads_with_derived_colours_and_is_not_rewritten` (load, assert `colour_of` is Teal, Violet; assert the file text is unchanged); `a_save_writes_every_effective_colour` (after `save_org`, the text has `color = "teal"` and `color = "violet"`); `two_teams_may_share_a_colour` (both load as written).
- [ ] **Step 2: Implement.** Add `color: Option<TeamColour>` (`#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = ...)]` with a custom `deserialize_with` that calls `TeamColour::parse`, as the existing `cli` field does; without `default` a missing key is an error) to `TeamFile`, and a new `TopRoleFile` (the `node_file!` role shape plus `color`) used only by `OrgFile.role`; nested `RoleFile` stays without it, and its `into_core` sets `color: None`. `team_file` and `top_role_file` write `org.colour_of(..)`. Every save-then-load equality test in `tests/org_file.rs` (`save_then_load_round_trips`, `every_field_round_trips`, `save_keeps_previous_version_as_backup`, `save_refreshes_last_good`, `loading_never_touches_last_good`, `mark_last_good_writes_only_last_good`, `backup_name_is_timestamped_and_beside_org_toml`, `timestamped_backup_kept`, `replace_org_with_no_file_saves_without_backup`) compares the loaded org with the expected org after `fix_colours`, through one test helper, because a saved org now loads with `Some(colour)`; the fixtures stay colourless, so `parses_prd_example_org` and `org-example.toml` stay as they are; add `a_mixed_file_derives_missing_colours_by_position` (team A `color = "violet"`, team B unset: B takes `ALL[1 % 4]`, violet too, as TD §7 says).
- [ ] **Step 3: Run** `cargo test -p duohive-store` (pass), `make check`, commit `feat: color in org.toml`.

### Task OB.3: Palette tokens

**Files:**
- Modify: `crates/duohive-tui/src/theme.rs`
- Test: `crates/duohive-tui/src/render/colour_tests.rs`

**Interfaces:**
- Consumes: `TeamColour` (OB.1).
- Produces: on `Palette`: `fn hue(&self, c: TeamColour) -> Color`; `fn tint(&self, c: TeamColour, percent: u8) -> Color` (the hue over `bg`; under `auto` and mono it returns the hue's ANSI colour, and the painter then paints no background); `manager: Color`; `fn tint_manager(&self, percent: u8) -> Color`; `enum Pill { Idle, Starting, Working, Waiting, Quiet, Errored, Stopped }` and `fn pill(&self, p: Pill, typed_into: bool) -> (Color, Color)` (background, text). Values are the tables in UI §2.3.

- [ ] **Step 1: Failing tests.** `team_hues_meet_the_contrast_floor` (dark title text on `bg` and chip text on the hue: teal 7.92, violet 6.53, slate 6.13, orchid 7.36; light: 4.91, 6.88, 7.00, 5.83, each within 0.01 and at least 4.5); `selected_agent_text_meets_4_5_on_the_wash` (`fg` and white on the 30% tint: 7.26 and 10.73 on dark, 8.89 on light); `manager_and_pills_meet_4_5` (manager 15.17 and 7.00; the lowest pill 6.50 dark and 4.56 light); `manager_selected_name_meets_4_5_on_its_wash` (9.10 dark, 9.57 light); `typed_into_attention_and_error_pills_keep_their_usual_text` (on `light` white on `#835B10` is 6.05 and on `#A8323A` 6.6, never `manager` text); `auto_maps_hues_to_ansi` (teal cyan, violet magenta, slate `fg.muted`, orchid bright magenta); `mono_has_no_colour`.
- [ ] **Step 2: Implement** the tokens for `dark`, `light`, `auto`, mono with the exact hex values from UI §2.3; the pill table in UI §2.3 is the source of every pairing, including the lighter and darker tints used when the card is typed into.
- [ ] **Step 3: Run** `cargo test -p duohive-tui colour` (pass), `make check`, commit `feat: team hue, manager and pill tokens in the palette`.

### Task OB.3a: The tree's hue and selected row

**Files:**
- Modify: `crates/duohive-tui/src/render/org_pane.rs` (`draw_row` and the selected-row painting in `draw`)
- Test: `render/colour_tests.rs`, `render/auto_tests.rs`, `render/a11y_tests.rs`

**Interfaces:**
- Consumes: `Palette::{hue, tint, tint_manager}` (OB.3), `Org::colour_of` (OB.1).

- [ ] **Step 1: Failing tests.** `tree_glyphs_and_names_take_the_hue` (on rows that are not selected, the `▣` and `◇` cells and the team or role name have `fg` of `hue(c)` and are bold; connectors, state cells and replies keep their colours; Duo's row stays bold `fg`); `the_compact_tree_tints_too`; `the_selected_tree_row_has_the_bar_and_a_30_percent_wash` (`▌` `fg` is the hue, every cell of the row has `bg` of `tint(hue, 30)`, and Duo's row `tint_manager(26)`); `the_selected_tree_rows_text_is_fg` (the glyph, the state cell, `✉ n` and the reply on the row are `fg`, never the hue, `fg.muted` or a status colour; the team or role name is bold, white on `dark`), and `a_selected_tree_row_never_pairs_the_hue_with_its_own_wash` (every team or role row, all four hues, both themes: no text cell has the hue as `fg` on the hue wash); `auto_and_mono_paint_no_wash_and_keep_the_bar_and_bold`; `typing_leaves_the_tree_selection_unchanged`.
- [ ] **Step 2: Implement** the tint and the wash as UI §4.1.6 and §2.3 say (the bar's colour moves from `border.focus` to the hue, so `border.focus` colours only the focused pane's title). The tree's drag marker `▹` becomes `fg` like the boxes' (UI §4.4.2), since `border.focus` now colours only the focused pane's title. The text snapshots do not change; the colour assertions in `colour_tests.rs` that named the old bar colour are updated, including `only_auto_bolds_the_selected_row` (the selected row's `bg` is now the hue wash, not `selection`, and the name is bold under `dark` and `light`; under `auto` it stays as it was).
- [ ] **Step 3: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: team colour on the tree's icons and names, and a hue wash on its selected row`.

### Task OB.4: The wordmark font

**Files:**
- Create: `crates/duohive-tui/src/wordmark.rs` (declare it in `lib.rs`)
- Test: in the same file (`mod tests`)

**Interfaces:**
- Produces: `pub const GLYPH_COUNT: usize = 38;` `pub fn glyph(c: char) -> Option<[&'static str; 2]>` (each string 3 cells; `None` for a character outside the 38, case-insensitive); `pub fn wordmark(name: &str) -> [String; 2]` (the first three characters, one blank column between glyphs, padded on the right to 11 columns).

- [ ] **Step 1: Failing tests.** `duo_draws_its_wordmark` (`wordmark("Duo")` is `["█▀▄ █ █ ▄▀▄", "█▄▀ █▄█ ▀▄▀"]`); `case_does_not_matter` (`duo` equals `DUO`); `a_short_name_is_padded_to_eleven_columns` (`wordmark("a")`, each string 11 cells); `every_name_character_has_a_glyph` (A–Z, a–z, 0–9, `-`, `_`; 38 distinct upper-case glyphs); `the_chart_matches_the_ui_spec` (rendering the 38 glyphs in the spec's three rows equals the chart string pasted from UI §4.1.5).
- [ ] **Step 2: Implement** the const table from the chart in UI §4.1.5 (a `▀` is the top half, a `▄` the bottom half, `█` both); take the first three `char`s, never byte-slice.
- [ ] **Step 3: Run** `cargo test -p duohive-tui wordmark` (pass), `make check`, commit `feat: the wordmark font for the Manager card`.

### Task OB.5: The box rows

**Files:**
- Create: `crates/duohive-tui/src/boxes.rs`, `crates/duohive-tui/src/boxes_tests.rs` (declare both in `lib.rs`)

**Interfaces:**
- Consumes: `UiModel` (selection `m.selected`, fold state, the filter, views), `Org::colour_of`, `format::{status_word, format_age, truncate_middle, quoted}`.
- Produces: `enum BoxLineKind { ManagerTop, ManagerName, ManagerCounts, ManagerBottom, TeamTop, RoleTop, Agent, Detail, RoleBottom, TeamBottom, EmptyRole, FilterEmpty }`; `struct BoxLine { kind, key: Option<NodeKey>, hue: Option<TeamColour>, selected: bool, folded: bool, label: String, right: String, inbox: u32 }`; `pub fn box_rows(m: &UiModel, now: Millis, replies: bool) -> Vec<BoxLine>`, laid out against fixed inner widths (46 for a root box, 42 inside a team: the pane is 50 columns whenever the boxes are drawn), so it takes no width. `Detail` belongs to the agent above it (`key` is that agent's) and exists for the selected agent and for any agent with a problem; a problem wraps onto up to three `Detail` lines (UI §4.12). An org with no teams or roles is the four Manager lines alone.

- [ ] **Step 1: Failing tests** over the UI §4.1 sample org: `duo_is_a_four_line_card_whatever_the_state` (exactly `ManagerTop, ManagerName, ManagerCounts, ManagerBottom` first, for idle, working, waiting, errored, stopped, and a Duo that is `finished` shows idle); `a_long_duo_name_is_cut_at_16_columns_in_the_middle` (`platform-manager-and-coordinator-agent` gives `platform…r-agent`); `the_counts_line_matches_the_header` (`2 need you`, `2 need you · 1 card`, `3 need you · 2 cards`, and `nothing needs you` at zero, per UI §4.1.5); `a_duo_error_replaces_the_counts_with_its_short_reason` (the reason `opencode not found (code 127)` draws `opencode not found`, UI §4.1.5) and `the_short_reason_stops_at_the_first_paren_or_dot`; `boxes_stack_without_a_gap` (no blank line between a `TeamBottom` and the next `TeamTop`); `a_role_under_duo_is_its_own_box_with_agent_rows_inside`; `roll_ups_are_on_team_and_top_level_role_boxes_only` (Platform `● 1`, no `✓`; a `~` leads the roll-up of a top-level role box when a counted state is read from the screen); `a_selected_agent_gets_one_detail_line` (`working 2m · "Running go test ./..."`, none for unselected agents); `replies_off_leaves_the_reply_out_of_the_detail` (`working 2m`); `a_stopped_agent_detail_has_no_reply` (`stopped 1d`); `an_errored_agent_detail_names_the_reason` (`errored · opencode not found`); `a_problem_shows_its_detail_when_not_selected` (a missing directory, a failed worktree, an untested CLI notice); `a_selected_agent_with_a_problem_shows_the_problem_in_place_of_the_state_line` and `a_notice_follows_the_state_line` (UI §4.1.5: one second line only); `an_agent_name_is_cut_in_the_middle_and_keeps_its_inbox_marker`; `a_tilde_leads_the_icon_when_the_state_is_read_from_the_screen`; `a_long_last_reply_is_cut_at_the_box_edge`; `a_long_duo_error_reason_is_cut_with_an_ellipsis_on_line_2` (27 columns); `the_starting_pill_is_the_widest_and_still_fits_beside_a_16_column_name`; `the_manager_card_stays_under_a_filter`; `a_folded_team_or_role_is_its_title_line_only` (the line keeps its roll-up); `a_filter_keeps_matching_agents_and_their_boxes`; `a_filter_with_no_match_shows_its_line` (`no agent is working · esc clears the filter`); `an_empty_org_is_the_manager_card_alone`; `an_empty_role_shows_the_hint_line`.
- [ ] **Step 2: Implement** `box_rows` by walking the same org and fold state as `tree_rows` (reuse its filter and fold helpers rather than copying them; extract them from `tree.rs` if they are private). Duo's name is cut to 16 columns with `truncate_middle` here, not in the painter.
- [ ] **Step 3: Run** `cargo test -p duohive-tui boxes` (pass), `make check`, commit `feat: the pure row list for the org boxes`.

### Task OB.6: Drawing, scrolling, the wheel and hit-testing the boxes

**Files:**
- Create: `crates/duohive-tui/src/render/org_boxes.rs`, `render/manager_card.rs`, `render/org_boxes_tests.rs` (declare the test module in `render/mod.rs`)
- Modify: `render/org_pane.rs` (the `Boxes` arm of the dispatch from OB.0 now calls the box versions), `render/mod.rs` (`org_key_at`, `org_wheel`), `UiModel.org_scroll` use
- Test: `render/org_boxes_tests.rs`, snapshots under `render/snapshots/`

**Interfaces:**
- Consumes: `box_rows`, `wordmark`, `Palette` tokens (OB.3 to OB.5).
- Produces, with the same signatures as `org_pane`'s so the dispatch is one `if`: `pub(super) fn draw(buf: &mut Buffer, l: &Layout, m: &UiModel, ctx: &RenderCtx, replies: bool)`, `pub(super) fn key_at(l: &Layout, m: &UiModel, replies: bool, y: u16) -> Option<NodeKey>` and `pub(super) fn wheel(l: &Layout, m: &mut UiModel, replies: bool, up: bool, lines: usize)` (the same signature as `org_pane::wheel`: it returns `()` and writes `org_scroll`). The Manager card's name slot is 16 columns, its pill right-aligned, per UI §4.1.5.

- [ ] **Step 1: Failing snapshot tests** at 120×36 for `dark`: `boxes_screen_typing` (the org column above the Needs you separator equals the UI §4.1 diagram; OB.7 extends the check to the whole column), `boxes_screen_team_selected`, `boxes_screen_role_selected`, `boxes_screen_agent_selected` (the second line is there), `boxes_screen_manager_selected`, `boxes_screen_manager_typed_into`. Plus style assertions on the buffer: a selected team's top border `fg` is its hue and its title cell `bg` is the hue, and the roles inside it keep the 65% border; the Manager card's border is 40% idle and 100% selected; the typing row `bg` is the hue, its text `fg` is `bg`, the state icon cell has the same `bg` (no separate cell) and the `▌` is in the text colour; the selected agent's `▌` `fg` is the hue, its row `bg` is `tint(hue, 30)` and its icon `fg` is `fg`; the Manager card selected has no `▌`, and with Duo errored its line 2 is in the selected text colour, not `status.error`; a selected agent with a problem draws its second line in `fg`, and an unselected one in `status.error` (`fg.muted` for a notice); the status pill for each of idle, starting, working, waiting, quiet, errored and stopped has the background and text of UI §2.3, and the typed-into card uses the lighter or darker tints; under `auto` and mono the card's chip and fill carry `Modifier::REVERSED`.
- [ ] **Step 2: Failing behaviour tests.** `a_click_on_the_second_line_selects_the_agent`; `a_click_on_a_role_title_selects_the_role`; `a_click_on_the_manager_card_selects_duo`; `a_click_on_a_box_border_selects_its_box` (a team's or role's top or bottom line, not the agent above it); `compact_draws_the_tree_not_boxes_and_duo_stays_one_row`; `scroll_keeps_the_selected_agent_and_its_second_line_in_view` (a 100-agent role, `↓` through all); `the_wheel_scrolls_box_lines` (`org_scroll` moves by lines of the visible list); `a_drag_marks_a_valid_target_with_a_marker` (`▹` in `fg`, UI §4.4.2); `at_99_columns_draws_the_compact_tree_and_at_100_the_boxes` (the setting on `Boxes`); `at_110x24_the_screen_is_compact_so_it_draws_the_tree`; `the_tree_setting_at_120x36_still_draws_the_tree`; `a_resize_while_typing_keeps_typing`; `the_second_line_carries_the_reply_at_100x30` (the setting on; and `working 2m` alone with it off); `enter_folds_and_unfolds_a_box_and_a_click_does_not` (the title shows `▸` folded, and a click in Needs you on a folded group's agent unfolds it); `a_click_on_a_problem_agents_second_line_selects_it`; `a_long_problem_wraps_at_the_box_edge_keeping_the_hint` (UI §4.12; the text width is 38 in a role box inside a team (42 wide) and 42 at the root (46 wide), less borders and indent, and the test runs both; `errored · opencode not found · install it or change its CLI (e)` wraps onto two lines and `errored · worktree failed: the directory already exists and is not empty · remove it (e)` onto three, the hint `(e)` last; a longer one is cut with `…` on the third); `a_40_character_or_wide_name_is_cut_in_the_middle_beside_the_roll_up_and_fold_marker` (UI §2.4); `duos_error_on_the_card_is_the_short_reason_cut_at_27_columns`.
- [ ] **Step 3: Implement** the painter: each `BoxLine` is laid out against the box's width (46 at the root, 42 inside a team) and drawn with the Palette tokens as UI §4.1.5 and §2.3 say. Under `auto` and mono, paint no wash and use `Modifier::REVERSED` for chips and fills. The `Boxes` arm of `org_pane::draw`, `key_at` and `wheel` (OB.0) calls the box versions: the box painter replaces only the tree part of `org_pane::draw` (the frame, the title and Needs you stay as they are). The tree path stays whole: `WIDE_RIGHT`, the non-compact row painter and `tree_rows` are still used by the tree and for the arrow-key order in `input.rs` and `filter.rs`. The boxes pass `replies` as `settings.show_last_reply` alone (OB-D2), while `focus::shows_replies` is unchanged: `render/mod.rs` computes `replies` at three places (the draw call, `org_wheel` and `org_key_at`), and each takes the boxes' value when `drawn_layout` is `Boxes`. Correct the `Size::Compact` and `Layout.compact` doc comments in `focus.rs` (they still say 80–99 × 24; `Size::of` already takes the smaller of width and height).
- [ ] **Step 4: The existing snapshots stay as they are,** because `Settings::default()` is `Tree` (OB.0); only the new box snapshots set `org_layout` to `Boxes` in their model. Add one fixture, `five_agent_org()`, whose org is the UI §4.1 diagram's (Payments with Backend and QA, Platform with Infra, Duo idle, with the diagram's pending approval card and auth-service's `✉ 1`), for `boxes_screen_typing`, and run `cargo test -p duohive-tui` to confirm no existing snapshot changed.
- [ ] **Step 5: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: draw the org pane as grouped boxes with a Manager card`.

### Task OB.7: Needs you rows when the boxes are drawn

**Files:**
- Modify: `crates/duohive-tui/src/render/needs_you.rs`
- Test: `render/pane_tests.rs` (or the existing Needs you tests)

- [ ] **Step 1: Failing tests.** `wide_needs_you_rows_are_icon_name_reason` (`◆  billing-api  "Allow edit to api.go?"`, `◆  Duo  approval  Add pay-web`, `◷  squad-3  quiet`); `boxes_screen_typing_whole_column` (the whole org column of the UI §4.1 diagram, Needs you included); `compact_needs_you_keeps_glyph_and_time` (the existing 80×24 snapshot is unchanged). Re-accept the OB.6 `boxes_screen_*` snapshots, whose Needs you rows change here.
- [ ] **Step 2: Implement** the wide row format (UI §4.1.3 and §4.1.5) when `drawn_layout` is `Boxes`; the tree keeps its glyph, word and time rows (UI §4.1.6); the order, oldest first, is unchanged.
- [ ] **Step 3: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: Needs you rows without a state word or time at 100 columns`.

### Task OB.8: The Colour row in the forms

**Files:**
- Modify: `crates/duohive-tui/src/forms/{field,team,role,mod,org}.rs`, `render/form.rs`
- Test: `crates/duohive-tui/src/forms/tests/groups.rs` (and the existing `forms/tests` file for Edit org)

**Interfaces:**
- Consumes: `TeamColour`, `Org::colour_of`, `wordmark`, `TeamDraft.color`, `RoleDraft.color`.
- Produces: `FieldId::Colour` as a `FieldKind::Choice` of the four names (it draws all four, each in its hue, where other `Choice` fields draw only the selected one); present in `new_team`, `edit_team` and the role form only when it has no team.

- [ ] **Step 1: Failing tests.** `new_team_form_starts_on_the_next_free_colour`; `edit_team_form_shows_its_colour`; `a_role_in_a_team_has_no_colour_row`; `a_root_role_form_has_a_colour_row`; `left_right_choose_a_colour` (and a click); `save_sends_the_colour_in_the_draft`; `saving_a_new_colour_recolours_the_boxes`; `the_colour_row_shows_all_four_names_each_in_its_hue` (a render test); `a_card_edit_form_has_the_colour_row` (UI §4.6); `edit_org_shows_the_wordmark_of_the_typed_name` (typing `queen-bee` in Duo's name field shows `QUE`'s two rows beside it, and the name rules of UI §4.4.1 refuse `-bee`, a 41-character name and an existing agent's name, in `forms/org.rs`); `duo_name_is_read_only_unless_duo_is_stopped` (the field shows the reason, UI §4.4.1).
- [ ] **Step 2: Implement** the field, its default (`next_free` over the siblings), the draft mapping, the all-names draw path in `render/form.rs`, the card line and the Edit org preview.
- [ ] **Step 3: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: choose a team colour in the team and root role forms`.

### Task OB.8a: The first-run layout row

**Files:**
- Modify: `crates/duohive-core/src/command.rs` (`Command::CreateOrg` gains `org_layout: OrgLayout`), `engine/create_org.rs` (writes `settings.org_layout`, OB-D3), `crates/duohive-tui/src/first_run/{mod,steps,forms}.rs`, `crates/duohive-tui/src/input.rs` (`wizard_action`)
- Test: `first_run/tests.rs`, the engine tests for `create_org`, snapshots under `first_run/snapshots/`

**Interfaces:**
- Consumes: `OrgLayout` (OB.0).
- Produces: `Command::CreateOrg { template, your_name, duo_cli, org_layout }`; a wizard field `layout: OrgLayout` on step 1, `Boxes` by default.

- [ ] **Step 1: Failing tests.** `step_1_has_the_layout_row_with_boxes_preselected`; `left_right_and_a_click_choose_the_layout`; `the_wizard_sends_the_chosen_layout_in_create_org`; `create_org_saves_the_layout_in_settings_toml`; `the_chosen_layout_is_drawn_this_session_and_survives_a_later_settings_save` (in `crates/duohive/tests/app.rs`, which has the engine, the store and the TUI together: choose `Boxes`, finish first run: the pane draws boxes with no restart, then `v` and a Settings change both leave `org_layout = "boxes"` in `settings.toml`); `replace_mode_has_no_layout_row_and_leaves_the_setting_alone` (steps 2 and 3 only); `step_1_fits_80x24`.
- [ ] **Step 2: Implement** the field, the row of UI §4.3, the command field and the engine write; in `input.rs::wizard_action` (where the command is sent as a `UiEffect::Send`; `first_run::key` has no `UiModel`) set `m.settings.org_layout` with the command; update the step 1 snapshots (two more lines) and the other `CreateOrg` call sites (`crates/duohive/tests/app.rs`, the dispatch match in `duohive-core/src/engine/mod.rs`, the literals in `engine/create_org/tests.rs`).
- [ ] **Step 3: Run** `cargo test --workspace` (pass), `make check`, commit `feat: choose the org layout in first run`.

### Task OB.9: Accessibility, the setting's label, and docs

**Files:**
- Test: `render/a11y_tests.rs`, `render/auto_tests.rs`, `render/settings_tests.rs`, `boxes_tests.rs`
- Modify: `render/settings.rs` (the `LAST_REPLY` row and the `ORG_NOTE` text), `crates/duohive-core/src/settings.rs` (the `show_last_reply` doc comment, OB-D2), `input/menu.rs` (the right-click row, with `input/menu_tests.rs`), the matching wording in `forms/settings.rs` (the action name `Last reply` in `keymap.rs` stays: UI §4.11 keeps it), `CHANGELOG.md`, `docs/README.md`, `docs/plans/status.md`, `README.md`

- [ ] **Step 1: Failing tests.** `no_color_keeps_the_chip_the_bar_and_the_second_line` (mono: the chip and fills carry `Modifier::REVERSED`, the bar and the second line are present); `auto_paints_no_background_for_washes`; `box_rows_for_100_agents_is_linear` (the row count for 100 agents in one role is 100 + a fixed overhead); `settings_row_reads_last_reply_and_reduce_motion_follows_it` (OB-D2; the `settings_80x24` and `settings_160x48` snapshots change in that row and in the Org note (the Org layout row is already in them from OB.0), and `settings_matches_the_spec` and `org_note_sits_under_an_org_heading` in `settings_tests.rs` are updated to the UI §4.10 text, `e on the Manager card (Edit org)`); `the_right_click_row_reads_hide_reply_then_show_reply` (and the two menu snapshots).
- [ ] **Step 2: Make them pass** (fixes only in the painter, the tokens or the Settings label), then run `cargo test --workspace`.
- [ ] **Step 3: Docs.** `CHANGELOG.md` under `[Unreleased]`: `### Added` an org layout setting (first run, Settings, the key `l`) with grouped boxes at 100 columns and 30 rows or more, a colour per team, the Manager card, the Colour row; `### Changed` the tree's team icons and names take the team colour and its selected row a hue wash, and the last-reply setting also controls the boxes' second line (a settings file with no choice keeps the tree). `docs/README.md`: add `[Org boxes](plans/org-boxes-plan.md)` to the plans row. `docs/plans/status.md`: a line for this plan. `README.md`: the sample screen at the top and the *Highlights* bullet on the org show the boxes and a per-team colour. `docs/guide.md` is on the unmerged branch `docs/user-guide` (PR #73): if it has merged into `main` by the time this branch is cut, update its screen description the same way; otherwise leave it, and say so in the pull request.
- [ ] **Step 4: Gate and commit.** `make check`, then `git add docs/design docs/plans docs/product docs/ux docs/README.md README.md CHANGELOG.md crates && git commit -m "feat: org boxes accessibility checks and docs"`. The 100-agent benchmark gate is the author's run (`duohive bench --gate`); CI also reports it.
