# sqdist evaluation contract (frozen)

Kernel: `sq_euclid(a: &[f32], b: &[f32]) -> f32`, squared Euclidean distance over f32.

Frozen observable semantics (ADR 005):

- Result is the sum of `(a[i] - b[i])^2` accumulated in f32. Summation order is NOT frozen.
- Tolerance: `|got - ref| <= 1e-4 * |ref| + 1.2e-38`, where `ref` is the same sum in f64.
  Subnormal inputs may therefore flush their squares to zero; large magnitudes (1e15) must stay finite.
- Empty inputs return exactly `0.0`.
- **Mismatched lengths panic** (message contains `length mismatch`). Truncating to the shorter
  slice is NOT allowed.
- No allocation, no global state, no new `unsafe`, dependencies, build scripts or capabilities
  (`std::net`, `std::process`, `std::env`, filesystem writes, `extern "C"`, external `include_*!`).

Performance gate: primary bench is length 1024; guards are length 7 and length 1000003 (odd).
Paired interleaved samples (40 after 5 warmups). Accept only if the 95% CI lower bound of the
speedup exceeds 1.10; each guard must show non-inferiority (CI lower bound of speed ratio >= 0.90).

Layout: `sqdist/` is the baseline and the harness owner (`tests/`, `src/bin/`). `sqdist-holdout/`
is hidden from the proposer and lives outside every candidate tree. Candidates under
`sqdist-candidates/` must keep the harness files byte-identical to the baseline's.
