# ADR 003: Agent Booster as the local edit engine

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Use ruvnet's agent-booster package as the first local patch adapter. Pin 0.2.2 and preserve npm integrity and lockfile. Production Rust core communicates through a versioned local sidecar first; direct Rust linkage is a future adapter pending API inspection.

Observed API: new AgentBooster().apply({code, edit, language}). The returned edited text is result.output, not result.code. Require success=true, nonempty output, expected source hash, bounded file size, allowed path, and independent postconditions.

A confidence score is placement evidence, never correctness proof. Rust parsing, compilation and independent behavior checks remain mandatory. A failed application retains its diagnostics and rejects the candidate.

Live ruOS smoke: Rust add to sum edit succeeded through exact_replace, reported 22 ms, confidence 0.9420393705368042. JavaScript fixture succeeded, reported 1 ms. These are individual package reported timings, not representative benchmarks or proof of semantic refactoring.

Use the package to apply explicit candidate snippets; use OpenRouter to reason only when local approved recipes cannot propose the change. No model fallback on ordinary builds.

Acceptance: fixture output exactly matches the requested text; repeat with stale input hash and reject before editing; malformed or unsuccessful results cannot enter the candidate tree.

Alternative: LLM for every edit increases network dependency and variable cost. Handwritten transformations remain preferable where stronger language aware guarantees exist.

