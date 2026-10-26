# Result: ADR 004 equal-budget random control study

**Verdict: `Insufficient`. Do NOT claim that learning improves search.**
Preregistration: `study/PREREGISTRATION.md` (sha256 `7f772787110f42ff89fdec09f9bdd406ffe6dd13975ba6601d0e05adf2df6ef6`,
committed at `111aba2` before any study pair was evaluated; no deviations from it).

## Numbers (preregistered analysis: k = 3, 10000 trials, seed 0x5255_4E45_5354_5544, `HashedFeatureRanker`)

24 unseen workloads, 3 library recipes (unroll4, unroll8, table), 111 evaluations by the real `rune-eval`.

| arm | hit@3 | Wilson 95% | mean evaluations to first accept (miss = 3) |
|---|---|---|---|
| retrieval top-3 | 37.5% (9/24) | [21.2%, 57.3%] | 2.33 |
| uniform random 3 | 37.5% | [21.2%, 57.3%] | 2.44 |

One-sided paired sign-flip p = 1.0000. The verdict is `Insufficient` because the library (3 recipes)
is not larger than the budget k = 3 (`ControlStudy` requires library > k): both arms then evaluate the
whole library and have identical hit rates by construction. Only the evaluation *order* differs
(2.33 vs 2.44 evaluations to first accept), which is not tested. The ceiling for any proposer with this
library is 9/24 unseen workloads (at least one library recipe accepted); mean accepting recipes per
unseen workload 0.67 of 3.

Exploratory and post hoc (added after seeing the outcomes; NOT preregistered, cannot change the verdict;
two extra looks at the same data): k = 1: retrieval 29.2% [14.9, 49.2] vs random 22.2% [10.1, 42.0], p = 0.135,
`NoDifference`; k = 2: 37.5% [21.2, 57.3] vs 33.3% [18.0, 53.3], p = 0.123, `NoDifference`. Intervals overlap
heavily; there is no evidence either way. Full tables and the 34 x 8 outcome matrix: `study/control-report.md`
(rendered), `study/outcome-matrix.json`, `study/control-study.json`, per-evaluation `rune-eval` reports in
`study/reports/<workload>/<recipe>.json`, run log `study/reports/run-log.jsonl`.

## Why the library is so small (all from the evaluator, none tuned)

Of 111 evaluations: 29 Accept, 56 insufficient improvement, 26 guard regression (all 26 at the length-7 guard;
the 4-8x faster unrolled/SWAR kernels are 10-40% slower on 7 elements), 0 holdout/visible/build/gate rejections
(the candidates are correct), 0 internal errors, 0 borderline Accepts needing the confirmation re-run
(first-run CI lower bounds were never in [1.08, 1.12]). iter-fold, bounds-hoist and branchless were never
accepted anywhere (LLVM already produces that code); swar-u64 had one training workload (u8_sum) and was
guard-rejected there, so it never entered the library even though it was accepted on 2 of 3 unseen
applicable workloads (and fused-pass on 3 of 3, with no argmin in the training set, as declared in advance).
Library membership is "accepted on a training workload", so these structural gaps are part of the result:
the memory can only transfer what it saw succeed.

## Protocol facts

4 evaluations at a time, worker i pinned to SMT pair (2,18)..(5,21) with `taskset`; rune-eval pinned its
benchmark runs to the idler core of the pair (core idle fraction at pinning: mean 0.96, min 0.72).
Host load average (1 min) when each evaluation started: 3.62 to 6.37 on 32 logical CPUs (includes the 4
workers; the preregistered start gate was < 4.0 and the load was 3.62 at start). Pre-sweep calibration with
the sqdist fixture showed no CI-width inflation 4-way vs serial. Mean wall time 17.5 s per evaluation.
rune-eval binary sha256 `8eae48d50dbf0ff14638dc9e911a1eefd274e00c14c1330cf35f631be0208361`, rustc 1.98.1.

## What this does and does not show

It shows that, with this synthetic kernel family and these hand-authored recipes, the preregistered
comparison cannot be made: the learned library collapsed to 3 recipes, no larger than the equal budget, so
the retrieval arm has nothing to select among, and at smaller budgets the difference is not distinguishable
from zero (n = 24). It does not show that retrieval is useless or that it helps; it is not evidence about
learning in general.

## Caveats

Small synthetic kernel family (34 workloads, 24 unseen, strongly clustered: f32/f64 twins, same ops across
types); one machine (Ryzen 9 9950X, shared host, load 3.6-6.4); one compiler (rustc 1.98.1); eight
hand-authored recipes, where the first two (unroll4/8) account for most acceptances so the study mostly
measures "float reductions are accepted by unrolling"; applicability was declared mechanically, so tags that
mirror applicability (elem, shape, op) make the retrieval problem easy in principle yet the library too
small to exercise it; guard behaviour at length 7 decided 26 outcomes; a single evaluation per pair
(no repeated measurement beyond the borderline rule); the sign-flip test and the Wilson-overlap rule are the
library's, used as is. A study with a larger library (more training workloads covering every recipe, or more
recipes accepted at training) is the clear next step; it would need a new preregistration.

## Audit and reproducibility

- All 111 reports were checked against `study/manifest.json`: `baseline_digest`, `candidate_digest` and
  `contract_digest` equal the frozen `baseline_tree_digest`, per-candidate `tree_digest` and
  `contract_struct_digest` for every pair (111/111; `rune-eval` also enforced the contract digest via
  `--expect-digest`).
- Candidates are not committed. `rune-study gen --study study --candidates <dir>` regenerates them
  byte-for-byte (tree digests in `manifest.json`; covered by the determinism test).
- Code changes after the preregistration commit `111aba2`: the preregistered analysis code is unchanged.
  `crates/rune-study/src/analyze.rs` gained the post hoc exploratory k = 1, 2 block, and its markdown helpers
  were then moved to `render.rs` (no behaviour change; `control-report.md` regenerates identically apart from
  that block). No workload, recipe, contract, split, k, seed or rule was changed.
- Not done: repeated measurement beyond the borderline rule (one evaluation per pair); any claim for k < 3
  beyond the exploratory table; the next step is a NEW preregistration (larger training set covering every
  recipe family, or a recipe set that yields a library larger than k), not a tweak of this one.
