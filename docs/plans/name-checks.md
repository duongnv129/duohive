# Name checks

Task 5.5a of the [phase 5 plan](phase-5-plan.md): read-only lookups an agent can make. Nothing was registered, claimed or bought. Task 5.5b (legal searches, registration) is the author's.

## Lookups made on 2026-10-23

| Name | Where | Result |
|---|---|---|
| `duohive` | crates.io (`/api/v1/crates/duohive`) | 404: not published |
| `duohive-core`, `-cli`, `-store`, `-tmux`, `-tui`, `-mcp` | crates.io | 404 for each: not published |
| GitHub user or organisation `duohive` | `api.github.com/users/duohive` | 404: free as a user or organisation name |
| `duongnv129/homebrew-tap` (the tap, P5-Q5) | `api.github.com/repos/…` | 404: does not exist (or is private to someone else) |
| `duohive.dev` | RDAP via `rdap.org` | 404 "not found": appears unregistered; no NS records |
| `duohive.ai` | RDAP via `rdap.org` | 404 "not found": appears unregistered |
| `duongnv129/duohive` | `gh repo view` | **private** (P5-Q1: the plan assumed public) |

An RDAP "not found" is not a reservation: a registrar can still refuse a premium or reserved name, and a name can be taken between this check and a purchase.

## Open items (author)

- [ ] USPTO and WIPO trademark searches in classes 9 and 42; record the dates and results here.
- [ ] Register `duohive.dev` or `duohive.ai`.
- [ ] Claim the `duohive` GitHub organisation (if wanted) and publish nothing under the crate names until the tag (a placeholder publish is a decision for the author, not a step of this plan).
- [ ] Create `homebrew-tap` under the owner the formula will live in (P5-Q5).
- [ ] Make the repository public (P5-Q1).
- [ ] If any check finds a conflict: decide a rename with the author before Task 5.3b publishes anything (PRD Branding considered Duocrew and DuoHQ).
