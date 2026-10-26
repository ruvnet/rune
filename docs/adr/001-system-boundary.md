# ADR 001: Rune architecture and Cargo compatibility

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Build a new Rust orchestration core with Cargo compatibility as an adapter. Own dependency planning, build graphs, discovery, validation, promotion, artifact retention and execution evidence. Reuse rustc and existing code generation initially.

Inputs: Cargo.toml, Cargo.lock, source tree, toolchain digest, target descriptor and policy. Outputs: immutable build plan, artifact digest and evidence receipt.

Separate deterministic build, exploratory improve, and authorized promote operations. Normal builds never call a model or rewrite source.

Alternatives: a Cargo plugin is cheaper but limits ownership of scheduling and lifecycle; an immediate compiler and package ecosystem rewrite creates unacceptable compatibility risk. Select the independent core with staged replacement.

Acceptance: build an unchanged fixture workspace, compare behavior against Cargo, then reproduce the promoted artifact with networking and agents disabled. Byte identity requires a pinned reproducible environment; otherwise report the exact remaining nondeterminism.

Rollback: retain Cargo invocation and previous approved plan. Compatibility is a tested matrix, not a universal claim.

