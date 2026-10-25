# Name checks

Task 5.5a of the [phase 5 plan](phase-5-plan.md): read-only lookups an agent can make. Nothing was registered, claimed or bought. Task 5.5b (legal searches, registration) was removed by the author on 2026-10-25 (P5-D12), so the open items below are kept as a record and nothing waits on them.

## Lookups made on 2026-10-23

| Name | Where | Result |
|---|---|---|
| `duohive` | crates.io (`/api/v1/crates/duohive`) | 404: not published |
| `duohive-core`, `-cli`, `-store`, `-tmux`, `-tui`, `-mcp` | crates.io | 404 for each: not published |
| GitHub user or organisation `duohive` | `api.github.com/users/duohive` | 404: free as a user or organisation name |
| `duongnv129/homebrew-tap` (the tap, P5-Q5) | `api.github.com/repos/…` | 404: does not exist (or is private to someone else) |
| `duongnv129/duohive` | `gh repo view` | private when checked (P5-Q1); made public by the author on 2026-10-23 |

The domain and DNS lookups were removed by the author on 2026-10-25 (P5-D12).

## Open items (author)

- [x] USPTO and WIPO trademark searches in classes 9 and 42. Dropped by the author on 2026-10-25 (P5-D12).
- [ ] Claim the `duohive` GitHub organisation (if wanted) and publish nothing under the crate names until the tag (a placeholder publish is a decision for the author, not a step of this plan).
- [ ] Create `homebrew-tap` under the owner the formula will live in (P5-Q5).
- [x] Make the repository public (P5-Q1). Done 2026-10-23.
- [ ] If any check finds a conflict: decide a rename with the author before Task 5.3b publishes anything (PRD Branding considered Duocrew and DuoHQ).
