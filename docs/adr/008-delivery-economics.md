# ADR 008: Delivery gates and economic acceptance

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Phase 1 builds an independent CLI and immutable plan format with Cargo adapter and offline replay. Phase 2 adds measured Agent Booster edit application and synthetic OpenRouter proposal flow. Phase 3 integrates RuVector retrieval, isolated workers and independent MetaHarness evaluation. Phase 4 adds target hardware execution and RVF packaging.

Own source builds and source transformations before experimenting with MIR passes or a rustc fork. Native Rust implementation is the end state; Node sidecar is an explicit temporary compatibility choice.

Requirements: compatibility maps to unchanged fixture builds; deterministic execution maps to offline replay; privacy maps to blocked egress tests; local edits map to exact output and stale hash rejection; learning maps to held out equal budget control; cleanup maps to traversal and active lease tests.

Economic decision uses search cost divided by measured recurring savings. A $10 search returning $1 daily savings has an approximate ten day payback; exclude speculative improvements and include evaluation cost.

Latency objective: recipe selection p95 below 10 ms excluding process startup and compilation. This is a target requiring measurement, not a current capability.

Exit gate for a first release: one unchanged workspace builds; one real kernel improves at least 10% under the independent gate; frozen replay works with agents disabled; cost ledger reconciles; all rejection cases pass.

Current package scope is ADRs, source grounded adapter smoke scripts and evidence. It is not a rebuilt Cargo, full compiler, proof of security, or completed multi hardware optimizer.

