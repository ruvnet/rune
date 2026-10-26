# Preregistration: equal-budget random control study (ADR 004)

Written and committed BEFORE any benchmark-bearing evaluation of a study pair was run.
Generator + frozen workloads were committed first at `0c440392f9b59e1137c7d4d5f2babf88d994aac5`
(`study/manifest.json` sha256 `206300c4bb670c7c34d9c3162c7ec2769c360bb981fdf01c9479bd643c308330`).
Any deviation after this commit is recorded as a deviation in `study/RESULT.md`; the
result is reported whatever it says. If it is `NoDifference` or `Insufficient`, that is
the deliverable. No parameter below may be changed after seeing outcomes.

## 1. Question

ADR 004: "Demonstrate retrieval against an equal budget random proposal control on unseen
workloads before claiming learning improves search." Hypothesis H1: proposing the top-k recipes
ranked by the memory (`HashedFeatureRanker` over recipe fingerprints) yields a higher rate of
at least one evaluator-accepted proposal within k evaluations than k uniform-random distinct
recipes from the same library, on workloads the library never saw.
Null: no difference. The verdict logic is exactly `rune_memory::control::ControlStudy::run`
(`Better` iff retrieval's Wilson 95% lower bound exceeds random's upper bound AND the one-sided
paired sign-flip p < 0.05 AND retrieval rate > random rate; `Insufficient` if fewer than 20 unseen
workloads or library size <= k; else `NoDifference`).

## 2. Evaluator

The real `rune-eval` binary built from this repository (`cargo build --release --workspace`,
`rune-toolchain-eval` unchanged from base commit `95fed44`), driven only through its CLI with
`--expect-digest` set to each contract's `EvalContract::digest()` (recorded in `manifest.json`).
No `--allow-degraded` and no `--no-pin`: missing isolation layers (netns, prlimit) reject loudly.
Per-workload contract (all identical except name and workload): samples 40, warmup 5,
`min_improvement` 0.10 (CI lower bound of the paired speedup must exceed 1.10), primary bench at
length 1024, guard benches at length 7 and 1000003 with tolerance 0.10 (non-inferiority), `forbid_unsafe`,
`forbid_new_deps`, `forbid_new_capabilities`, 300 s / 8192 MB limits. These are the settings of the
`sqdist` worked example. Candidates must keep `tests/`, `src/bin/` and `Cargo.toml` byte-identical.

## 3. Workloads (34 generated, dependency-free, unsafe-free kernel crates)

`rune-study gen` builds, per workload: a baseline crate (`study/workloads/<w>/baseline`), a bench bin
printing `{"ns_per_op":..}`, visible tests (lengths multiples of 8, random data), a hidden holdout
(`study/workloads/<w>/holdout`, lengths 0,1,2,3,7,8,9,15,16,17,31,33,63,65,1023,1025,2040,2041,4099
(+65537 for integer kernels); random, spike-at-end, spike-at-start, constant and edge-value data
(floats: +-0, +-1e15 (f32) / +-1e150 (f64), subnormal; ints: MIN/MAX/-1/0; bytes: 0/255/needle/digits/
vowels/space); empty input; mismatched lengths must panic with "length mismatch"), and the contract.
Frozen semantics: float Add reductions accumulate in the element type, order not frozen, tolerance
`|got-ref| <= tol * sum|terms|` (tol 1e-4 f32, 1e-9 f64; ref in f64; absolute floor 1.2e-38 / 1e-300);
float max/min kernels, all integer and u8 kernels are EXACT (integer sums use wrapping arithmetic in
the declared accumulator type); argmin returns the FIRST minimal index (`None` if empty); `find_first`
searches for the const `NEEDLE = 0xA5`; `hist` returns `[u64;256]`; empty min/max return the init value.
Operations: f32 and f64 x {sum, dot, sqdist, l1, sumsq, saxpy_sum (0.5*x+y), maxabs, min, argmin};
i32 x {sum, dot, l1, max, min, count_eq, argmin}; u8 x {sum, count_eq, hamming, max, find_first,
count_digit, count_vowel, count_space, hist}. Baseline loop style (indexed `a[i]` vs iterator) is
`fnv1a("rune-study/style/v1:"+name) & 1`. Before this freeze all 145 crates (34 baselines, 111
candidates) were checked only for correctness (`rune-study verify`: visible + holdout pass), and
three hand-made mutants (dropped tail, argmin tie-break, SWAR index) were confirmed to fail the
holdout. Correctness bugs in generated code were fixed before the freeze; nothing was tuned on timing.

## 4. Train / unseen split (fixed, computed from names only)

Within each element type, the first `n` workloads ordered by `fnv1a("rune-study/split/v1:"+name)` are
TRAINING: f32 3, f64 2, i32 2, u8 3 (10 training). The remaining 24 are UNSEEN. The coverage of
this split (which families land in training) was inspected before any evaluation and not changed:
no argmin workload is in the training set, so `fused-pass` can have no training evidence (see 7).

```
f32_sum unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f32_dot unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f32_sqdist TRAIN iter recipes=unroll4,unroll8
f32_l1 TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f32_sumsq TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f32_saxpy_sum unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f32_maxabs unseen iter recipes=unroll4,unroll8,branchless
f32_min unseen iter recipes=unroll4,unroll8,branchless
f32_argmin unseen iter recipes=fused-pass
f64_sum unseen iter recipes=unroll4,unroll8
f64_dot unseen iter recipes=unroll4,unroll8
f64_sqdist unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist
f64_l1 unseen iter recipes=unroll4,unroll8
f64_sumsq unseen iter recipes=unroll4,unroll8
f64_saxpy_sum unseen iter recipes=unroll4,unroll8
f64_maxabs TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless
f64_min TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless
f64_argmin unseen indexed recipes=fused-pass
i32_sum unseen iter recipes=unroll4,unroll8
i32_dot unseen iter recipes=unroll4,unroll8
i32_l1 TRAIN iter recipes=unroll4,unroll8
i32_max unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless
i32_min unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless
i32_count_eq TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless
i32_argmin unseen indexed recipes=fused-pass
u8_sum TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,swar-u64
u8_count_eq unseen iter recipes=unroll4,unroll8,branchless,swar-u64
u8_hamming unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,swar-u64
u8_max unseen iter recipes=unroll4,unroll8,branchless
u8_find_first unseen iter recipes=swar-u64
u8_count_digit unseen indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless,table
u8_count_vowel TRAIN indexed recipes=unroll4,unroll8,iter-fold,bounds-hoist,branchless,table
u8_count_space TRAIN iter recipes=unroll4,unroll8,branchless,table
u8_hist unseen iter recipes=table
```

(The full table with tags is `rune-study describe`; tags are in `manifest.json`.)

## 5. Recipes (8, hand-authored candidate generators in `crates/rune-study/src/recipes.rs`)

| id | transformation | applicable iff (mechanical) |
|---|---|---|
| unroll4 | 4 accumulator lanes via `chunks_exact(4)` + scalar tail | kernel is a scalar reduction |
| unroll8 | 8 lanes via `chunks_exact(8)` + scalar tail | kernel is a scalar reduction |
| iter-fold | indexed loop rewritten as `iter().zip().fold` | scalar reduction with indexed baseline |
| bounds-hoist | re-slice inputs to one length before an indexed loop | scalar reduction with indexed baseline |
| branchless | `if cond {acc+=1}` -> `acc += cond as u64`; `if t>acc` -> `acc.max(t)` | reduction whose combine is count, max or min |
| swar-u64 | u64 word-at-a-time (SWAR) with scalar tail | u8 and op in {sum, count_eq, hamming, find_first} |
| table | 256-entry const class table (predicate counts) / 4 sub-histograms (hist) | u8 and op in {count_digit, count_vowel, count_space, hist} |
| fused-pass | two-pass min-then-position fused into one pass | op is argmin |

Inapplicable (recipe, workload) pairs get NO candidate and are recorded as not accepted without
evaluation (rule above is the declared applicability rule). Applicable pairs: 111, all evaluated,
training workloads included. Floating-point reordering by the unroll recipes is permitted by the
contract tolerance and is not an evaluator bypass.

## 6. Evaluation protocol

- Every applicable pair is evaluated once with `rune-eval` (first run). Exit 0 = Accept, 1 = Reject (any
  reason: build, visible, holdout, gate, guard, insufficient improvement, timeout), 2 = internal error
  (retried once; if still an error it counts as NOT accepted and is listed in RESULT.md).
- Borderline rule: a first-run Accept whose CI lower bound lies in [1.08, 1.12] is re-run once on the same
  settings. The final outcome is Accept only if BOTH runs Accept (both reports kept). A first-run Reject
  is final (a re-run could only confirm it under the AND rule). This is the "fresh confirmation re-run"
  multiplicity control; no other multiplicity correction is applied.
- Concurrency: at most 4 evaluations at a time, worker i pinned with `taskset -c N,N+16` (the SMT
  sibling pair; cores 2,3,4,5 -> pairs (2,18),(3,19),(4,20),(5,21)); rune-eval additionally pins its
  benchmark runs to the idler of the two. Calibration done before this freeze with the unrelated
  `tests/fixtures/kernels/sqdist` example (not a study pair): 4-way CI widths 0.0030-0.0034 vs
  0.0027-0.0038 serial (3 serial runs), no inflation, so 4-way is used. Host `/proc/loadavg` is recorded at
  sweep start/end and per evaluation (`study/reports/run-log.jsonl`); the sweep is started only if the
  1-minute load average is below 4.0 on the 32-logical-CPU host. Each report's own `load_before/after`
  and pinned core are kept.
- Reports: `study/reports/<workload>/<recipe>.json` (full `rune-eval` JSON), re-runs as `*.rerun.json`.

## 7. Library, fingerprints, analysis

- Workload fingerprint (query and record), mechanical from the workload spec: source tags
  `elem:<t>`, `shape:<reduce|argreduce|search|histogram>`, `combine:<add|max|min|count>` (reductions),
  `fp:nonassociative` (float add) or `assoc:exact`, `loop:<indexed|iter>`; workload tags `op:<name>`,
  `inputs:<1|2>`; constant target/cpu/compiler/abi/policy fields.
- Library = recipes accepted (final outcome) on at least one TRAINING workload. A recipe's ORIGIN is the
  training workload on which it was accepted with the highest first-run CI lower bound (ties: name).
  Its fingerprint is the origin workload's fingerprint, with no hand-added tags. A recipe with no
  accepted training workload is not in the library (reported as such). Consequence declared in advance:
  `fused-pass` has no applicable training workload and cannot enter the library. Library size must
  exceed k for a verdict other than `Insufficient`.
- Study: k = 3, trials = 10000, seed `0x5255_4E45_5354_5544`,
  ranker `HashedFeatureRanker`, verdict by `ControlStudy::run` on the 24 UNSEEN workloads (outcomes of all
  library recipes on them; inapplicable = not accepted). Reported: n workloads, n recipes, hit@3 for both
  arms with Wilson 95% intervals, sign-flip p-value, mean evaluations to first accept (miss counts as k),
  the ceiling (unseen workloads with any library recipe accepted), per-recipe accept counts, the full
  outcome matrix, and a sensitivity analysis using first-run outcomes without confirmation (labelled
  non-primary).
- Not claimed regardless of outcome: anything about workloads outside this synthetic kernel family, other
  machines, other compilers, LLM-proposed recipes, or any recipe being safe; ADR 004 forbids similarity
  from approving anything and this study does not test that.

## 8. Disclosure of what was observed before freezing

Only: (a) the sqdist-fixture `good` candidate evaluated serially and 4-way (calibration); (b) identity
(baseline-as-candidate) smoke runs on `u8_find_first` and `u8_hist` to exercise the pipeline (both rejected
as insufficient improvement, as expected); (c) correctness runs of tests. No study candidate was benchmarked.
