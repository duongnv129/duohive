# DuoHive vs herdr: comparison and roadmap input

**Status:** Research note, 2026-11-09 · Not a specification. Nothing here changes the PRD, the UI/UX spec or the technical design until the author accepts an item and it is edited into them (see `AGENTS.md`).
**Sources:** [herdr.dev/tui](https://herdr.dev/tui/), [Concepts](https://herdr.dev/tui/docs/concepts/), [docs index](https://herdr.dev/tui/docs/), the [repo](https://github.com/herdrdev/herdr), its [top open discussions](https://github.com/herdrdev/herdr/discussions?discussions_q=is%3Aopen+sort%3Atop) (pages 1–2, 35 entries) and the first page of its [issues](https://github.com/herdrdev/herdr/issues) (25 of 431 open). Compared against our [PRD](../product/prd.md), [v1 roadmap](../plans/v1-roadmap.md) and [v1.1 roadmap](../plans/v1.1-roadmap.md).

**Confidence.** The pages were read through a summarising fetch, not by hand. Herdr's own figures (about 49.7k stars, 2.1 million installs, 2,065 plugins, 22 agent CLIs) are self-reported and unverified; treat them as "very large". The issues page does not show reaction counts, so the issue list below is by page order and labels, not by demand. Discussion upvotes are shown and are the better demand signal.

---

## 1. What herdr is

A Rust terminal app that is **an agent runtime, built as a multiplexer**. Its own background server owns the terminals (a vendored Ghostty VT parser), and clients attach and detach. It replaces tmux rather than sitting on it.

| Area | Herdr |
|---|---|
| Model | Workspace → tab → pane (a real terminal). Flat and project-shaped, like tmux/zellij |
| Persistence | Server keeps panes alive on detach; layout restored after restart; supported agents resumed natively; named sessions |
| Status | `blocked`, `working`, `done` (unseen), `idle`, `unknown`; from foreground process, screen manifests and optional integrations; per-client "seen" state |
| Agents | 22 CLIs detected out of the box (Claude Code, Codex, OpenCode, Cursor, Grok, Copilot, Pi, Hermes …), not wrapped |
| Agent-facing API | CLI and local socket API: split panes, start and prompt other agents, wait until another is blocked. Any agent can call it |
| Machines | Saved SSH machines in one window, combined agent list, independent reconnects |
| Extensibility | Plugins (executable, manifest actions, event hooks), a marketplace, an "agent guide" prompt for onboarding |
| Input | `ctrl+b` prefix, terminal / prefix / navigate modes, mouse-native (click, drag borders, right-click menus), copy mode |
| Platforms | macOS, Linux, Windows (beta). Installs: curl script, Homebrew, mise, binaries, cargo |
| Licence | Apache-2.0 |

## 2. What DuoHive is

An **org chart of standing agents** on top of tmux: teams → roles → agents with stacked inherited prompts, a manager agent (Duo) with token-checked tools and approval cards, task-to-role dispatch, worktrees with merge clean-up, setup templates, hook-first honest status, and hard performance budgets (10–100 agents). Two CLIs (Claude Code, OpenCode), macOS and Linux.

## 3. Side by side

| Dimension | DuoHive (PRD) | Herdr | Read |
|---|---|---|---|
| Core idea | Team of *standing* agents; you manage people | Runtime where agents live; you manage panes | Different product. Herdr is infrastructure; we are management |
| Unit of work | Named agent with role, prompt, directory | Pane / workspace | Ours carries identity; theirs is generic |
| Instructions | Written once, stacked org→team→role→agent | None (each agent's own config) | **Our edge** |
| Manager agent | Duo: chat, see org, message, start/stop, propose changes with approval cards | None built in; any agent may use the API | **Our edge**; theirs is more open, ours safer |
| Agent-to-agent control | Messages only when receiver at rest; sender identified by secret token; no delegation chains | Open API: any agent splits, prompts, waits | Theirs is more powerful, ours is more trustworthy. Deliberate non-goal for us |
| Approvals | Cards in a dialog; we never auto-approve | Detects `blocked`; no approval workflow | **Our edge** |
| Status honesty | Hook-exact, `~` marks screen-read, `quiet` overlay | Detection by process and screen; mislabels reported as `idle` in several open bugs (see §5) | **Our edge**, and a headline claim worth keeping |
| Task routing | "Give work to a role": free agent takes it, else approval card | Not present | **Our edge** |
| Worktrees | Per task, auto-removed when merged | Mentioned in a bug; jj workspaces requested | Ours is more complete; jj is a gap |
| Templates / onboarding | Agent Squad, Product Team, Engineering Org, Blank | Quick start, agent-guide prompt | Ours is more product-shaped |
| Scale and weight | Budgets: <1% CPU at 10, <3% at 100; <10 MB binary | No stated budget; an open issue reports 16–30% idle CPU with one pane | **Our edge**, and measured in CI |
| Agent breadth | 2 CLIs | 22 CLIs, with a steady stream of "add X" requests | **Their edge**; our biggest adoption limiter |
| Remote machines | None (local only) | SSH machines in one window | **Their edge** |
| Windows | No (tmux) | Beta | **Their edge** |
| Terminal ownership | tmux (mature, but we inherit its limits) | Own VT parser and server | Theirs is more controllable, far more work, and the source of most of their bugs |
| Extensibility | None in v1 | Plugins and a marketplace | **Their edge**; a big part of their reach |
| Mouse and keys | Every action by key and mouse; rebindable Keys screen | Mouse-native; prefix configurable only by request | Roughly even |
| Session resume | v1.1: pick an old session, including ones started outside DuoHive | Native resume of supported agents after restart | Similar; ours is user-picked, theirs automatic |
| Terminal features | Whatever tmux gives | Copy mode, selection, Kitty/Sixel requests, scrollback | Their edge, and a heavy bug surface |
| Distribution | alpha pre-releases, crates.io and Homebrew pending | 2.1M installs claimed, Homebrew, mise, curl | Their edge |

## 4. Pros and cons

### Herdr: why it succeeded (what to learn)

1. **Zero-ceremony start.** Run it, it detects the agents you already use. No org to design. Our first-run path asks for a structure before value.
2. **Agent-agnostic.** Detecting 22 CLIs removes "does it support mine?" as a reason to say no. Each new CLI request is cheap for them and a blocker for us.
3. **The status answer is the hook.** "Which agent is waiting for me" is the same pain as our Needs you list; they shipped it first and it carries their pitch.
4. **Runs anywhere.** SSH machines plus detach/reattach means a rented box is a first-class place for agents.
5. **Agents drive the tool.** A socket API that agents call turned the product into a platform, and the plugin marketplace compounds it.
6. **Fast install and a single binary** across three OSes, with an agent-readable onboarding doc.

### Herdr: weaknesses (our opening)

1. **Detection is heuristic and it shows.** Open bugs: an agent reported idle behind a workspace-trust modal (#5890), first-run screens reported idle so prompts get typed into them (#5888), OpenCode idle during a background subagent (#4511), nix-wrapped Claude not detected (#803), `claude attach` not detected (#5849). Hooks avoid this class of bug.
2. **Owning the terminal is expensive.** The largest issue cluster is input and rendering: vim/nvim escape handling, IME, undercurl colours, OSC 52, clipboard on WSL2 and Wayland, Kitty graphics, iOS keyboards, 16–30% idle CPU. tmux absorbs all of that for us.
3. **No notion of a team.** Nothing stores standing instructions, roles or "who should do this". The user is the router.
4. **No safety model for agent-to-agent control.** Any agent can drive any pane; a worker can spawn work with no approval layer. Several requests are about quieting orchestrator-driven workers (#13 in the discussions).
5. **Session restore is fragile:** restored panes in the wrong folder (#5879, #5867, #3116), the original processes do not survive a restart.
6. **Environment leaks on macOS** (panes outside the launchd session, TCC identity, stale privacy entries: #5905, #808, #4228) and Codex shared-daemon breakage (#4649, #4814) that they cannot fix upstream.
7. **Supply chain:** `Cargo.lock` matches four RustSec advisories (#5870). We already gate this with `cargo deny` and CI advisories.

### DuoHive: pros

- A clear, different position: **manage a team**, not panes. Nothing in herdr overlaps Duo, roles, stacked prompts, task-to-role or approval cards.
- Hook-exact status with honest `~` and `quiet`: the thing herdr gets wrong most often.
- Performance budgets enforced in CI, binary under 10 MB.
- Safety model for agent messaging (token identity, no delegation chains, never auto-approve).
- tmux underneath: agents survive, users can `tmux attach` as an escape hatch, and the terminal bug surface is not ours.

### DuoHive: cons and risks

- **Two CLIs only.** The single biggest gap by demand: herdr's top discussions include Mistral Vibe (#3 and #6, 88 votes together), Kiro (18), IBM Bob Shell (18) and Step Code (15).
- **Setup before value.** Templates help, but a user who just wants "see my running agents" has no zero-config path.
- **Local only, macOS and Linux only.** No remote machines, no Windows.
- **tmux dependency** is a real install hurdle and caps terminal features (no Kitty/Sixel passthrough guarantees, clipboard behaviour is tmux's).
- **No extension surface**, so every integration is our work.
- **Org model may be heavy for task runners**: the PRD already flags this and Agent Squad is the mitigation.
- **Adoption gap:** alpha only; herdr has a large head start and community.

## 5. What herdr users ask for

Top open discussions by upvotes, grouped, and where each lands against us. Votes are upvotes shown on the listing.

| Theme | Requests (votes) | DuoHive today | Suggestion |
|---|---|---|---|
| **Attention / status** | Mark agent unread (55); show agent status on tabs (17); per-pane mute (17); silence done-notifications for orchestrator workers (16); keyboard-navigable agent selector (18) | `finished` clears on select; Duo never raises finished; `↑↓` moves; no per-agent mute; no mark-unread | **Add** mark-as-unread and per-agent mute. Cheap, and the top-voted ask |
| **More agents** | Mistral Vibe (50, 38); Kiro (18); IBM Bob (18); Step Code (15) | Deferred (PRD non-goal) | **Decide** how to open the CLI set (see §6) |
| **Layout and chrome** | Sidebar left/right (32); hide agents sidebar (16); hide tab bar (13); tab and workspace numbers (51, 15, 13); hierarchical groups (47); pane spacing and borders (14) | Fixed three-pane layout; teams already nest | Hierarchical groups are what our org tree already is: **say so in the README**. Sidebar placement and number hints are small polish |
| **Keys** | Overwrite the prefix (17); rebindable search key (16); repeatable bindings (13); configurable copy-mode keys (36) | Keys screen with clash refusal; no prefix | Already ahead. Copy mode is tmux's |
| **Copy and clipboard** | PRIMARY selection (43); triple-click line (16); H/M/L in copy mode (14); vim word boundary (#970) | tmux decides | Out of scope; document the tmux settings that fix it |
| **Remote** | Select saved machine via API (33); per-connection SSH agent (16); docker exec tracking (16) | None | Out of v1. Candidate later (see §6) |
| **VCS** | Jujutsu workspaces (37) | git worktrees only | Candidate after v1 |
| **Safety / undo** | Confirm closing a tab (13); undo close (13) | We confirm stopping a running agent | Already ahead |
| **Files and hygiene** | Logs out of `XDG_CONFIG_HOME` (17); follow XDG for state (14) | Logs under `~/.local/share/duohive/logs` | Check we honour `XDG_DATA_HOME` and `XDG_STATE_HOME` everywhere |
| **Graphics** | Kitty and Sixel passthrough (16) | Not planned | Skip |

Issues page, by area (counts not shown): input and keyboard (largest), agent detection and session restore (p1), macOS environment inheritance, Windows/WSL and iOS, install and update (PATH, TCC, macOS identity), performance (idle CPU), rendering. Almost nothing on the first page is a feature request; users there mostly want the basics to stop breaking.

**Lessons the bug list gives us for free:**

1. A status that says `idle` while a modal is open is worse than `unknown`. Keep `~` and never infer idle from silence.
2. Restored agents must reopen in the agent's own directory. We store the directory per agent, so check resume and restart paths against #5879 and #3116.
3. Run agents in the user's real login environment (launchd/DNS) and test it on macOS (#5905).
4. Gate idle CPU in CI. We do; keep it.
5. Keep the advisory gate (#5870).

## 6. Suggestions for the V1 roadmap

These are proposals for the author to accept or reject. None is decided.

### A. Cheap, high-signal, fits v1 as scoped

| # | Item | Why | Size |
|---|---|---|---|
| 1 | **Mark unread** on an agent (re-flag `finished`) | Top-voted herdr ask (55); small state change | S |
| 2 | **Mute an agent's alerts** (per agent, in `org.toml` or the node menu) | 17 votes; also fits "orchestrator workers" and Duo's own noise | S |
| 3 | **Agent-status roll-up in the header** when panes are zoomed or the tree is hidden | Mirrors "status beside each tab in sidebar-free mode" (17 votes, 19 comments) | S |
| 4 | **Environment and cwd audit** of start, restart and resume against herdr's #5905, #5879, #3116 | Their p1 bugs; ours to avoid | S |
| 5 | **README positioning**: "your org tree is the hierarchical grouping people ask herdr for", plus a short comparison | Positions us against the market leader without new code | S |

### B. Decide before V1 sign-off (the author's call)

| # | Question | Options | Recommendation |
|---|---|---|---|
| 1 | **Third CLI**: how do we open the CLI set? | (a) Codex only, as already planned; (b) a documented adapter interface so the community adds CLIs; (c) generic "any CLI, screen-read status only" adapter | (c) as a **fallback adapter in v1.1**, status always `~`. It answers the most-voted demand without breaking "two CLIs done well", and our honest-status rule already covers it |
| 2 | **Zero-config first run** | Keep templates only, or add "Blank + detect running agents" | Add a Blank-first path that lists agent CLIs found on `PATH`; templates stay |
| 3 | **Remote machines** | None, SSH-in-tmux by hand, or first-class | **Not v1.** Document `ssh` + tmux as the supported route; revisit after the 30-day test |

### C. After V1 (v1.1 candidates)

1. Fallback "generic CLI" adapter and the next named CLIs (Mistral Vibe, Kiro).
2. Jujutsu workspace option beside git worktrees.
3. Per-agent mute and unread (if not pulled into v1).
4. Plugin or hook surface: only after the core is proven. Herdr shows the reach, and also the maintenance cost.
5. Windows via WSL2 documentation (already "best effort").

### D. Do not copy

- **Owning the terminal** (own VT parser and server). It is the source of most of their open bugs; tmux is our moat on effort.
- **Open agent-to-agent API with no approval layer.** It conflicts with our non-goal (no delegation chains) and our token identity rule.
- **Flat panes with no identity.** It would erase the PRD's hypothesis.
- **Kitty/Sixel passthrough and copy-mode clones.** Low value for a manager screen.

## 7. Decisions (2026-11-09)

Walked through one by one with the author. These are agreed in principle; the PRD, UI/UX spec and technical design are not yet edited.

| # | Item | Decision |
|---|---|---|
| 1 | Mark unread | **In v1.** A key (proposed `u`) and a node-menu entry put the agent back to `finished`: the ✓ glyph returns, it clears on the next select. No alert, not in Needs you. Needs a free key checked against the Keys screen |
| 2 | Per-agent mute | **Later.** Ignored for now |
| 3 | Header status when zoomed or tree hidden | **Later.** Ignored for now |
| 4 | Environment and cwd audit | **Dropped.** The author has not hit these bugs; assume we don't have them |
| 5 | README positioning | **Dropped.** The README does not mention herdr |
| 6 | Third CLI | **No change.** Claude Code and OpenCode in v1, Codex after v1. Revisit with real user requests after launch |
| 7 | Zero-config first run | **No change.** Agent Squad is the quick start. No new template (the author, after the first walk-through chose one, reversed it) |
| 8 | Remote machines | **Not v1, nothing said** in the README or elsewhere |
| 9 | XDG paths | **Checked, no change.** Config (`org.toml`, `settings.toml`) is under `XDG_CONFIG_HOME/duohive`; data, state, logs, socket, lock, worktrees and the model cache are under `XDG_DATA_HOME/duohive` (`crates/duohive-store/src/paths.rs`). Logs are not in the config directory, so herdr's complaint doesn't apply. There is no `XDG_STATE_HOME` use; cosmetic, left alone |
| 10 | CI gates | **No change.** Idle-CPU and advisory gates already run in CI |

**What goes into the v1 roadmap:** one small item: mark unread (item 1). Everything else is dropped, deferred, or already true.

## 8. Open items for the author

1. Confirm the key for mark unread (`u` is proposed; check it is free).
2. Later list, for the v1.1 roadmap if wanted: per-agent mute, header status when zoomed.
3. Re-run the issue sweep with reaction counts (`gh issue list --repo herdrdev/herdr --state open --json number,title,reactionGroups,comments`) before treating the issue-side findings as demand data.

*Not verified:* herdr's usage figures; which of their open bugs are fixed on `master` since the pages were read.

<!-- Superseded: the original open-items list below is kept for the record. -->

## Original open items (superseded by §7 and §8)

1. Accept or reject §6 A (five items), in or out of v1 scope.
2. Pick an option for §6 B1 (the third-CLI path), since it changes the PRD's "What We're NOT Building".
3. Should the comparison go into the README, or stay an internal note?
3. Re-run the issue sweep with reaction counts (`gh issue list --repo herdrdev/herdr --state open --json number,title,reactionGroups,comments`) before treating the issue-side findings as demand data.

*Not verified:* herdr's usage figures; which of their open bugs are fixed on `master` since the pages were read.
