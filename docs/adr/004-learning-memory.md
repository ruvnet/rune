# ADR 004: RuVector evidence and transfer memory

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Store recipe descriptors and references to evidence, not unconditional instructions. Retrieval uses source semantics, target architecture, CPU features, compiler version, ABI, workload distribution and policy fingerprint.

Every record includes baseline and candidate hashes, parent recipe, proposer, evaluation version, raw sample locations, failures, confidence intervals, resource cost and retention classification.

RuVector ranks candidate recipes. It cannot mark a candidate safe or promote it. Learned recommendations must pass the current evaluator even if they succeeded elsewhere. Rejected recipes remain searchable with failure scope.

Freeze an immutable memory snapshot per search campaign. New evidence enters a separate ingestion queue after validation. Private source stays tenant scoped and is not sent to public federation.

Acceptance: a recipe learned on x86 cannot become approved for ARM from similarity alone; compiler or policy changes invalidate approval keys. Demonstrate retrieval against an equal budget random proposal control on unseen workloads before claiming learning improves search.

Current status: interface decision only; no RuVector adapter implemented or benchmarked in this package.

