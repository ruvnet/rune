# ADR 005: MetaHarness promotion and security contracts

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Use MetaHarness as the proposed independent evaluation adapter. Optimizers cannot edit tests, evaluator settings, thresholds, secrets, signing policy or promotion permissions.

Control flow: freeze baseline and holdouts; reserve budget; retrieve or propose; fork isolated workspace; verify preconditions; apply; compile; test; benchmark; independently validate; retain evidence; promote or reject. Timeouts and missing evidence reject.

Freeze exact observable semantics including overflow mode, floating point tolerances, ordering, error behavior, ABI, allocation bounds and adversarial input requirements. Tests are evidence, not universal equivalence proofs. Apply formal checking to supported transformations when feasible.

Initial performance gate: paired measurements after warmup, at least 30 samples, report a 95% confidence interval for the chosen metric. Require the lower bound of speed improvement to exceed 10%, and reject any preregistered regression beyond its noise aware tolerance. Set multiplicity correction or a fresh confirmation set before testing many candidates.

Security gates are explicit properties and applicable checks, never a single confidence score. No new unsafe code, dependency, network capability or filesystem permission without a separately reviewed policy decision.

Acceptance: inject a candidate that passes visible tests but fails a hidden edge case; promotion must reject it. A faster candidate that expands permissions must also reject.

Rollback: atomically restore prior approved artifact and plan. Evaluator owner is distinct from proposal worker; stronger deployment isolation remains necessary.

