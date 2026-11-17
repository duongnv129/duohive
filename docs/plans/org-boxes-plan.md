# The org pane as grouped boxes, a colour per team and a Manager card — Implementation Plan

**Status:** Plan, 2026-11-17. The design was approved by the author on 2026-11-17 and is already written into the three living documents (commits `a8957dd`, `b6457a9`, `6100610` on `docs/org-boxes`). Not built.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox syntax. Work on a new branch `feat/org-boxes` cut from `docs/org-boxes`, so the specs travel with the code into one pull request; the author pushes it.

**Goal:** At 100 columns and wider the org pane draws grouped boxes (a Manager card for Duo, a box per team holding a box per role holding the agents), each team has a colour from a four-colour palette that is saved in `org.toml` and chosen in the team and top-level role forms, and the selected and typed-into states are marked as the spec says. Below 100 columns the compact tree stays.

**Architecture:** The core gains a pure `TeamColour` and a `color` on teams and top-level roles; the store reads and writes it. The TUI gets a pure row builder (`boxes.rs`) that turns the existing `UiModel` into box lines, a painter (`render/org_boxes.rs`, `render/manager_card.rs`) that draws them with new palette tokens, a const wordmark font (`wordmark.rs`) and a `Colour` form row. The compact tree, `tree_rows` and every command stay as they are.

**Tech Stack:** Rust 1.99.0 (edition 2024), ratatui and crossterm (pinned with `=`), serde and toml (store), insta snapshots (existing).

**Specs (the plan argues from these):** [UI/UX §2.3, §4.1, §4.1.5, §4.4.1, §4.12, §6](../ux/ui-spec.md) · [PRD: org model, *One screen*, decisions log](../product/prd.md) · [TD §7 *Team colour*, *Org boxes and the Manager card*](../design/technical-design.md).

## Global Constraints

[AGENTS.md](../../AGENTS.md) and [Phase 0's](phase-0-plan.md#global-constraints) apply. In particular:

- `make check` (format check, clippy, tests, `cargo deny`, `typos`) passes before every commit. Conventional commits, no `Co-Authored-By`, never push.
- Toolchain 1.99.0, edition 2024. No `unwrap`/`expect` outside tests. No `unsafe`.
- `duohive-core` does no I/O. `duohive-tui` and `duohive-store` depend only on `duohive-core`.
- Test first, except infrastructure.
- `org.toml` tables deny unknown keys; a parse error names the 1-based line.
- Every text colour meets 4.5:1 on `dark` and `light` (UI §6). Colour is never the only signal. No hover: pointer motion is not read.
- Names are ASCII letters, digits, `-` and `_`, starting with a letter or digit, 1 to 40 characters.

## Decisions

| # | Decision |
|---|---|
| OB-D1 | **Siblings for colour** are the teams and the roles directly under Duo together, teams first, each list in file order. A new one takes the first palette colour no sibling uses, else `ALL[siblings.len() % 4]`. A colour missing from a hand-edited file is derived as `ALL[position % 4]` over that same list and not written until the next save, which writes every effective colour. |
| OB-D2 | **Boxes whenever `Layout.compact` is false**, that is `Size::Large` and `Size::Medium` (100 columns and 30 rows or more, org pane 50 wide); `Size::Compact` keeps `tree_rows`. Width and height each pick a row and the smaller applies (UI §3), so 110×24 is compact too. |
| OB-D3 | **One visible list.** `box_rows` returns every line of the pane, including the selected agent's `Detail` line; scrolling, hit-testing and drawing all read it, so they cannot disagree about heights. |
| OB-D4 | **Needs you's no-word, no-time rows** apply at 100 columns and wider only; the compact layout keeps its glyph-and-time rows (UI §4.1 compact layout). |
| OB-D5 | **A drag over boxes** marks a valid target by a `▹` before its title text, the glyph the tree already uses. |
| OB-D6 | **The Manager card never opens a second line;** selecting Duo is the chip, the solid border and the wash. |

## Review Focus

1. **A 100-agent squad** (Agent Squad, one role of 100): the box is taller than the pane, the selected agent and its opened second line stay fully in view as `↓` moves, and `box_rows` stays linear (OB.5, OB.6).
2. **Renaming Duo** to `a`, `-`, `_`, `7-eleven` and a 40-character name: the card is four rows, the wordmark is 11 columns, the name is cut at 16 columns with `…` in the middle, nothing slices a string mid-character (OB.4, OB.6).
3. **A hand-edited `org.toml`**: no `color`, an unknown colour, `color` on a role inside a team, two teams with the same colour. Each gives the specified result, and a file that loads is never rewritten until a save (OB.1, OB.2).
4. **The 100-column edge and resizes:** 99 columns draws the compact tree, 100 the boxes; a resize while Typing keeps Typing; a selected agent at the bottom edge with its second line cut off scrolls into view (OB.6).
5. **Selection in awkward places:** an agent in a folded role, under an active filter, with a very long name or last reply, stopped or errored (no reply, so `stopped 1d` / `errored · <reason>`), and a click on the second line (OB.5, OB.6).

## File structure

| File | Responsibility |
|---|---|
| `crates/duohive-core/src/colour.rs` (new) | `TeamColour`, the palette order, `next_free`, `parse`, the error text |
| `crates/duohive-core/src/org.rs`, `command.rs`, `engine/org_edit.rs`, `templates.rs`, `validate.rs` | the `color` field, its assignment and its validation |
| `crates/duohive-store/src/org_file.rs` | `color` in the file; `TopRoleFile` for roles under Duo |
| `crates/duohive-tui/src/theme.rs` | hue, tint, manager and pill tokens for `dark`, `light`, `auto`, mono |
| `crates/duohive-tui/src/wordmark.rs` (new) | the 38-glyph font and `wordmark(name)` |
| `crates/duohive-tui/src/boxes.rs` (new) | `box_rows`: the pure list of box lines |
| `crates/duohive-tui/src/render/org_boxes.rs`, `render/manager_card.rs` (new) | drawing, scrolling and hit-testing the boxes and the card |
| `crates/duohive-tui/src/render/needs_you.rs` | the no-word, no-time rows (OB-D4) |
| `crates/duohive-tui/src/forms/{field,team,role,mod}.rs`, `render/form.rs`, `render/card_fields.rs` | the `Colour` row and the card's `color` line |

---

### Task OB.1: `TeamColour` and the `color` field in the core

**Files:**
- Create: `crates/duohive-core/src/colour.rs` (declare it in `lib.rs`)
- Modify: `org.rs` (`Team` and `Role` gain `pub color: Option<TeamColour>`; fix every constructor, about 31 `Team {` sites, the compiler lists them), `command.rs` (`TeamDraft` and `RoleDraft` gain `#[serde(default)] pub color: Option<TeamColour>`), `engine/org_edit.rs`, `templates.rs`, `validate.rs`
- Test: `colour.rs` (`mod tests`), `engine/org_edit/tests.rs`, `templates.rs` tests

**Interfaces:**
- Produces: `enum TeamColour { Teal, Violet, Slate, Orchid }` (`Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize`); `TeamColour::ALL: [TeamColour; 4]`; `fn name(self) -> &'static str`; `fn parse(s: &str) -> Result<TeamColour, ColourError>`; `fn next_free(used: &[TeamColour]) -> TeamColour`; `Org::colour_of(&self, key: &NodeKey) -> Option<TeamColour>` (a team or top-level role: its own or derived per OB-D1; a role inside a team and an agent: the enclosing team's or top-level role's; Duo: `None`).

- [ ] **Step 1: Failing tests in `colour.rs`.** `parse_accepts_the_four_names_and_refuses_others` (`"tael"` fails with text `unknown colour "tael" · teal, violet, slate or orchid`); `next_free_takes_the_first_unused_then_wraps` (`&[]` → Teal; `&[Teal, Slate]` → Violet; four used → Teal; five used → Violet); `colour_of_derives_from_position_when_unset` (teams A, B and top-level role G, all `None`: Teal, Violet, Slate; a role in A and an agent in it: Teal; Duo: `None`).
- [ ] **Step 2: Implement `colour.rs` and `Org::colour_of`;** add the `color` fields and make the workspace compile (`cargo build --workspace --all-targets`). Expected: no errors.
- [ ] **Step 3: Failing engine tests.** `add_team_without_colour_takes_the_next_free`, `add_team_with_colour_keeps_it`, `edit_team_sets_the_colour`, `a_draft_without_colour_still_parses` (a `TeamDraft` JSON with no `color`, as Duo's `propose_org_change` sends today, deserializes with `color: None`), `add_root_role_counts_teams_as_siblings` (a team Teal, a new root role without colour: Violet), `a_role_in_a_team_refuses_a_colour` (refused with `a role inside a team takes its team's colour`). In `templates.rs` tests: `product_team_colours_follow_the_palette_order` and `agent_squad_generalist_is_teal`.
- [ ] **Step 4: Implement** the assignment in `engine/org_edit.rs` (`AddTeam`, `AddRole` with `team: None`, `EditTeam`, `EditRole`), the refusal in `validate.rs`, and the template colours. A draft without a colour on edit keeps the current one.
- [ ] **Step 5: Run** `cargo test -p duohive-core` (all pass), then `make check`, then commit `feat: a colour per team and top-level role in the core`.

### Task OB.2: `color` in `org.toml`

**Files:**
- Modify: `crates/duohive-store/src/org_file.rs`
- Test: `crates/duohive-store/tests/org_file.rs`

**Interfaces:**
- Consumes: `TeamColour::{parse, name}`, `Org::colour_of` (OB.1).
- Produces: `[[team]]` and top-level `[[role]]` accept `color = "teal" | "violet" | "slate" | "orchid"`; a role inside a team does not.

- [ ] **Step 1: Failing tests.** `color_round_trips_on_a_team_and_a_root_role`; `color_on_a_nested_role_fails_with_its_line` (error mentions the field and the 1-based line); `an_unknown_colour_names_the_four` (`unknown colour "tael"` and the four names, with the line); `a_file_without_color_loads_with_derived_colours_and_is_not_rewritten` (load, assert `colour_of` is Teal, Violet; assert the file text is unchanged); `a_save_writes_every_effective_colour` (after `save_org`, the text has `color = "teal"` and `color = "violet"`).
- [ ] **Step 2: Implement.** Add `color: Option<TeamColour>` (custom `deserialize_with` that calls `TeamColour::parse`, `skip_serializing_if = "Option::is_none"`) to `TeamFile`, and a new `TopRoleFile` (the `node_file!` role shape plus `color`) used only by `OrgFile.role`; nested `RoleFile` stays without it. `team_file` and `top_role_file` write `org.colour_of(..)`.
- [ ] **Step 3: Run** `cargo test -p duohive-store` (pass), `make check`, commit `feat: color in org.toml`.

### Task OB.3: Palette tokens

**Files:**
- Modify: `crates/duohive-tui/src/theme.rs`
- Test: `crates/duohive-tui/src/render/colour_tests.rs`

**Interfaces:**
- Consumes: `TeamColour` (OB.1).
- Produces: on `Palette`: `fn hue(&self, c: TeamColour) -> Color`; `fn tint(&self, c: TeamColour, percent: u8) -> Color` (the hue over `bg`; under `auto` and mono it returns the hue's ANSI colour, and the painter then paints no background); `manager: Color`; `fn tint_manager(&self, percent: u8) -> Color`; `enum Pill { Idle, Starting, Working, Waiting, Quiet, Errored, Stopped }` and `fn pill(&self, p: Pill, typed_into: bool) -> (Color, Color)` (background, text). Values are the tables in UI §2.3.

- [ ] **Step 1: Failing tests.** `team_hues_meet_the_contrast_floor` (dark title text on `bg` and chip text on the hue: teal 7.92, violet 6.53, slate 6.13, orchid 7.36; light: 4.91, 6.88, 7.00, 5.83, each within 0.01 and at least 4.5); `selected_agent_text_meets_4_5_on_the_wash` (`fg` and white on the 30% tint: 7.26 and 10.73 on dark, 8.89 on light); `manager_and_pills_meet_4_5` (manager 15.17 and 7.00; the lowest pill 6.50 dark and 4.56 light); `auto_maps_hues_to_ansi` (teal cyan, violet magenta, slate `fg.muted`, orchid bright magenta); `mono_has_no_colour`.
- [ ] **Step 2: Implement** the tokens for `dark`, `light`, `auto`, mono with the exact hex values from UI §2.3; the pill table in UI §2.3 is the source of every pairing, including the lighter and darker tints used when the card is typed into.
- [ ] **Step 3: Run** `cargo test -p duohive-tui colour` (pass), `make check`, commit `feat: team hue, manager and pill tokens in the palette`.

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
- Create: `crates/duohive-tui/src/boxes.rs` (declare it in `lib.rs`)
- Test: `crates/duohive-tui/src/boxes_tests.rs`

**Interfaces:**
- Consumes: `UiModel` (selection `m.selected`, fold state, the filter, views), `Org::colour_of`, `format::{status_word, format_age, truncate_middle, quoted}`.
- Produces: `enum BoxLineKind { ManagerTop, ManagerName, ManagerCounts, ManagerBottom, TeamTop, RoleTop, Agent, Detail, RoleBottom, TeamBottom, EmptyRole, EmptyOrg, FilterEmpty }`; `struct BoxLine { kind, key: Option<NodeKey>, hue: Option<TeamColour>, selected: bool, folded: bool, label: String, right: String, inbox: u32 }`; `pub fn box_rows(m: &UiModel, now: Millis) -> Vec<BoxLine>`. `Detail` belongs to the agent above it (`key` is that agent's).

- [ ] **Step 1: Failing tests** over the UI §4.1 sample org: `duo_is_a_four_line_card_whatever_the_state` (exactly `ManagerTop, ManagerName, ManagerCounts, ManagerBottom` first, for idle, working, waiting, errored, stopped); `boxes_stack_without_a_gap` (the sequence for Payments then Platform has no blank line between a `TeamBottom` and the next `TeamTop`); `a_selected_agent_gets_one_detail_line` (`working 2m · "Running go test ./..."`, and none for unselected agents); `a_stopped_agent_detail_has_no_reply` (`stopped 1d`); `an_errored_agent_detail_names_the_reason` (`errored · opencode not found`); `a_folded_team_is_its_title_line_only` (the line keeps its roll-up); `a_filter_keeps_matching_agents_and_their_boxes`; `an_empty_role_shows_the_hint_line`; `the_counts_line_matches_the_header` (`2 need you · 1 card`, and `nothing needs you` at zero); `a_duo_error_replaces_the_counts_with_its_reason` (`ManagerCounts` text `opencode not found (code 127) · e Edit org`, UI §4.12).
- [ ] **Step 2: Implement** `box_rows` by walking the same org and fold state as `tree_rows` (reuse its filter and fold helpers rather than copying them; extract them from `tree.rs` if they are private). Duo's name is cut to 16 columns with `truncate_middle` here, not in the painter.
- [ ] **Step 3: Run** `cargo test -p duohive-tui boxes` (pass), `make check`, commit `feat: the pure row list for the org boxes`.

### Task OB.6: Drawing, scrolling and hit-testing the boxes

**Files:**
- Create: `crates/duohive-tui/src/render/org_boxes.rs`, `render/manager_card.rs`
- Modify: `render/org_pane.rs` (`draw` and `key_at` dispatch on `l.compact`), `render/mod.rs` (`org_key_at` unchanged in signature)
- Test: `render/org_boxes_tests.rs`, snapshots under `render/snapshots/`

**Interfaces:**
- Consumes: `box_rows`, `wordmark`, `Palette` tokens (OB.3 to OB.5).
- Produces: `pub(super) fn draw(buf: &mut Buffer, l: &Layout, m: &UiModel, ctx: &RenderCtx)` and `pub(super) fn key_at(l: &Layout, m: &UiModel, y: u16) -> Option<NodeKey>`; the Manager card's name slot is 16 columns, its pill right-aligned, per UI §4.1.5.

- [ ] **Step 1: Failing snapshot tests** at 120×36 for `dark`: `boxes_screen_typing` (the org column equals the UI §4.1 diagram), `boxes_screen_team_selected`, `boxes_screen_role_selected`, `boxes_screen_agent_selected` (the second line is there), `boxes_screen_manager_selected`, `boxes_screen_manager_typed_into`. Plus style assertions on the buffer: a selected team's top border `fg` is its hue and its title cell `bg` is the hue; the typing row `bg` is the hue, its text `fg` is `bg`, and the state icon cell has the same `bg` (no separate cell); the selected agent's `▌` `fg` is the hue and its row `bg` is `tint(hue, 30)`; the Manager card selected has no `▌`.
- [ ] **Step 2: Failing behaviour tests.** `a_click_on_the_second_line_selects_the_agent`; `a_click_on_a_role_title_selects_the_role`; `a_click_on_the_manager_card_selects_duo`; `scroll_keeps_the_selected_agent_and_its_second_line_in_view` (a 100-agent role, `↓` through all); `a_drag_marks_a_valid_target_with_a_marker` (OB-D5); `99_columns_draws_the_compact_tree_and_100_the_boxes`; `110x24_is_compact_so_it_draws_the_tree`; `a_resize_while_typing_keeps_typing`.
- [ ] **Step 3: Implement** the painter: each `BoxLine` is laid out against the 46-column box and drawn with the Palette tokens; borders at `tint(hue, 65)` (100% on the selected box), titles in the hue and bold, the chip only on the selected box's title, the washes and the typing fill as UI §4.1.5 and §2.3 say, the icon in the row's own text colour on a typed-into row. Under `auto` and mono, paint no wash and use `Modifier::REVERSED` for chips and fills. `org_pane::draw` and `key_at` call the box versions when `!l.compact`.
- [ ] **Step 4: Run** `cargo test -p duohive-tui` (new snapshots reviewed with `cargo insta review`, then accepted), `make check`, commit `feat: draw the org pane as grouped boxes with a Manager card`.

### Task OB.7: Needs you rows at 100 columns and wider

**Files:**
- Modify: `crates/duohive-tui/src/render/needs_you.rs`
- Test: `render/pane_tests.rs` (or the existing Needs you tests)

- [ ] **Step 1: Failing tests.** `wide_needs_you_rows_are_icon_name_reason` (`◆  billing-api  "Allow edit to api.go?"`, `◆  Duo  approval  Add pay-web`, `◷  squad-2  quiet`); `compact_needs_you_keeps_glyph_and_time` (the existing 80×24 snapshot is unchanged).
- [ ] **Step 2: Implement** the wide row format (OB-D4); the order, oldest first, is unchanged.
- [ ] **Step 3: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: Needs you rows without a state word or time at 100 columns`.

### Task OB.8: The Colour row in the forms

**Files:**
- Modify: `crates/duohive-tui/src/forms/{field,team,role,mod}.rs`, `render/form.rs`, `render/card_fields.rs`
- Test: `crates/duohive-tui/src/forms/tests/groups.rs`

**Interfaces:**
- Consumes: `TeamColour`, `Org::colour_of`, `TeamDraft.color`, `RoleDraft.color`.
- Produces: `FieldId::Colour` as a `FieldKind::Choice` of the four names; present in `new_team`, `edit_team` and the role form only when it has no team.

- [ ] **Step 1: Failing tests.** `new_team_form_starts_on_the_next_free_colour`; `edit_team_form_shows_its_colour`; `a_role_in_a_team_has_no_colour_row`; `a_root_role_form_has_a_colour_row`; `left_right_choose_a_colour` (and a click); `save_sends_the_colour_in_the_draft`; `an_org_change_card_lists_the_colour` (`+ color: teal`); `edit_org_shows_the_wordmark_of_the_typed_name` (typing `queen-bee` in Duo's name field shows `QUE`'s two rows beside it, and the name rules refuse `-bee` and a 41-character name, in `forms/org.rs`).
- [ ] **Step 2: Implement** the field, its default (`next_free` over the siblings, OB-D1), the draft mapping, the names drawn each in its own hue in `render/form.rs`, and the card line.
- [ ] **Step 3: Run** `cargo test -p duohive-tui` (pass), `make check`, commit `feat: choose a team colour in the team and root role forms`.

### Task OB.9: Accessibility, performance and docs

**Files:**
- Test: `render/a11y_tests.rs`, `render/auto_tests.rs`, `boxes_tests.rs`
- Modify: `CHANGELOG.md`, `docs/README.md`, `docs/plans/status.md`, `docs/guide.md`, `README.md`

- [ ] **Step 1: Failing tests.** `no_color_keeps_the_chip_the_bar_and_the_second_line` (mono: the chip and fills carry `Modifier::REVERSED`, the bar and the second line are present); `auto_paints_no_background_for_washes`; `box_rows_for_100_agents_is_linear` (the row count for 100 agents in one role is 100 + a fixed overhead).
- [ ] **Step 2: Make them pass** (fixes only in the painter or the tokens), then run `cargo test --workspace`.
- [ ] **Step 3: Docs.** `CHANGELOG.md` under `[Unreleased]`: `### Added` a colour per team, the Manager card, the Colour row; `### Changed` the org pane as boxes at 100 columns and wider, a selected agent opens a second line, and Needs you rows lose their state word and time at that width. `docs/README.md`: add `[Org boxes](plans/org-boxes-plan.md)` to the plans row. `docs/plans/status.md`: a line for this plan. `docs/guide.md` and `README.md`: the Org pane description and the sample screen show the boxes and say a team's colour is chosen in its form.
- [ ] **Step 4: Gate and commit.** `make check`, then `git add -A docs README.md CHANGELOG.md crates && git commit -m "feat: org boxes accessibility checks and docs"`. The 100-agent benchmark gate is the author's run (`duohive bench --gate`); CI also reports it.
