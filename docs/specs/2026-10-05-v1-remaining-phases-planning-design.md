# Planning the remaining v1 phases — design

**Status:** Approved 2026-10-05 · **Scope:** how phases 2–5 of the [v1 roadmap](../plans/v1-roadmap.md) are planned and checked for consistency. It adds no product requirement.

## Goal

Every remaining v1 phase has a plan that follows the [PRD](../product/prd.md), the [UI/UX spec](../ux/ui-spec.md), the [technical design](../design/technical-design.md) and the roadmap, and the five documents agree with each other.

- **Success:**
  - each plan can be executed task by task without re-reading the specs;
  - every task cites the spec sections it implements;
  - every spec requirement has exactly one owning phase;
  - no contradiction between the documents is left unresolved.

## Decisions

| Topic | Choice | Why |
|---|---|---|
| Depth | Phase 2 gets a full test-first plan now. Phases 3–5 get one outline plan each, expanded to full detail when the phase starts | The roadmap's rule ("each phase gets its own detailed plan when it starts"). Phase 0–1 folded many execution-time decisions back into its plan, so a detailed plan for phases 3–5 written now would rest on code that doesn't exist yet |
| Format | The full plan follows `phase-0-1-plan.md`: header, Global Constraints, Review Focus, File Structure, decisions where the specs are silent, then tasks with failing tests first and checkbox steps | One shape for every plan, and it is the shape the execution skills expect |
| Outline content | Goal, done-when (from the roadmap), spec coverage, tasks (files touched, test intent, spec references), risks, open decisions | Enough to sequence the work and check coverage, little that can go stale |
| Traceability | One matrix, `docs/plans/v1-traceability.md`: every PRD capability row, every UI spec screen and rule, and every TD section, mapped to a phase and a task | A requirement with no owner, or with two, is visible in one place |
| Conflicts | Settled by the precedence in `docs/README.md`: the PRD decides *what*, the UI/UX spec *how it looks*, the technical design *how it's built*. A change to a PRD requirement goes to the author first; wording and status fixes are made in place | One rule, and the product owner keeps the product decisions |
| Decision numbering | Phase 2 decisions are `P2-D1…`; outlines list open decisions as `P3-Q1…`, `P4-Q1…`, `P5-Q1…`; phase 0–1's `D1–D32` keep their numbers and stay in force | No clash with the decisions already cited from code and docs |
| Verification | `ecc:santa-method`: two independent reviewers, one rubric (traceability, consistency, feasibility against the code and `AGENTS.md`), with fixes made and re-reviewed for up to three rounds, then anything open goes to the author | Two reviewers without shared context catch what the author of the plans misses |

## Work order

1. **Audit the five documents against each other.** Check phase scope, states, glyphs, keys, terms and status lines, and fix conflicts by the precedence rule.
2. **Write the full phase 2 plan.** It opens with spike S3 on the live agent pane, because `no-output` has no per-pane exception (TD §4.1, §4.3).
3. **Write the outlines for phases 3–5 and the traceability matrix.**
4. **Run santa-method verification,** then fix the findings and repeat for up to three rounds.
5. **Link the plans** from the roadmap and `docs/README.md`, then commit.

## Out of scope

- Changing what v1 delivers or the order of the phases.
- Detailed plans for phases 3–5 (each is written when its phase starts).
- Any code change.
