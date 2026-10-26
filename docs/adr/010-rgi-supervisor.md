# ADR 010: rGi as an optional campaign supervisor

Status: Proposed
Date: 2026-10-26
Scope: Rune, the learning toolchain for Rust

## Decision and rationale

Treat [rGi](https://github.com/ruvnet/rGi) as an optional, external supervisor for long-running improvement campaigns. rGi calls Rune through the `rune` CLI; no Rune crate depends on rGi, and Rune builds, verifies and promotes with rGi absent.

Why: `rune improve` is a single bounded call. A real search loop (recall, propose, edit, evaluate, retain, repeat) must survive restarts, stop on request, and treat an interrupted step as uncertain rather than successful. rGi provides a SQLite journal with a single-writer lease, budget reservations, checkpoints, stop checks and an uncertain-outcome state. Rune should not grow a second supervisor.

Observed rGi facts (README and docs/INTEGRATIONS.md read 2026-10-26; rGi not executed): supervisor in TypeScript on Node 24; policy kernel `rgi-core` in Rust with exact capability matching and checked integer budgets; adapters for AgenticOW, MetaHarness, Autogenous, rvCSI tested against fixtures only; the project describes itself as an experimental foundation, not demonstrated AGI; plugins are trusted code.

## Boundary

Exposed to rGi as capabilities, each with an explicit validator and cost ceiling:

| Capability | Backed by | Authority |
| --- | --- | --- |
| `rune.improve` | `rune improve --campaign <id>` | Budget reserved in the Rune ledger first; output is data |
| `rune.eval` | `rune eval --contract … --baseline … --candidate …` | Read-only on the candidate tree; verdict from Rune's evaluator |
| `rune.recipes.search` | `rune recipes search` | Ranking only |

Not exposed: `rune promote`, `rune rollback`, `rune cleanup --apply`, `rune edit` on the source tree, signing keys. Promotion authority stays with Rune's evaluator, approval keys and hidden holdout (ADR 004, ADR 005). A supervisor that can promote its own candidates would defeat independent evaluation.

Budgets: the Rune ledger is the source of truth for money and tokens. rGi's reservation is a pre-check that must be no larger than the remaining Rune policy; it never replaces the ledger entry. A mismatch rejects the step.

## Memory branching

rGi's AgenticOW adapter wraps copy-on-write vector branches. That maps to ADR 004's frozen per-campaign snapshot of recipe memory, not to ADR 007's candidate workspace. ADR 007 stays an in-repo copy workspace with an ownership manifest; AgenticOW is not a filesystem sandbox. A campaign may use one AgenticOW branch per snapshot only after its API and isolation semantics are verified against the pinned commit.

## Alternatives

* Build a supervisor inside Rune: duplicates journal, lease and recovery logic and enlarges the Rust core for a feature many users will not run.
* Depend on `rgi-core` for budgets and authority: attractive, but couples release cadence and toolchain (rGi pins Rust 1.90; Rune's baseline is 1.88). Revisit if `rgi-core` is published with a stable API and a compatible MSRV.
* Let rGi drive promotion: rejected, see Boundary.

## Acceptance

* A spike plugin runs `rune.improve` and `rune.eval` on the sqdist fixture through rGi; killing rGi mid-step leaves the Rune ledger with an open reservation that `rune improve --reconcile` reports, never a silent success.
* A mismatched budget (rGi larger than the remaining Rune policy) rejects before any request.
* A plugin attempt to call `promote`, `cleanup --apply` or `edit` is rejected by the capability allowlist.
* Rune's own CI and release path pass with rGi uninstalled.

Rollback: remove the plugin; Rune is unchanged.

Current status: design only. No rGi integration code exists and rGi has not been run against Rune.
