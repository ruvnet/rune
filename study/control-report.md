## Equal-budget random control (ADR 004)

24 unseen workloads, 3 library recipes (training workloads only; 0 recipes excluded), budget k=3 evaluations per workload, 10000 random trials per workload, seed 5932734143788242244.

| arm | hit@k | Wilson 95% | mean evals to first accept |
|---|---|---|---|
| retrieval top-k | 37.5% | [21.2%, 57.3%] | 2.33 |
| uniform random k | 37.5% | [21.2%, 57.3%] | 2.44 |

Paired sign-flip p-value (one-sided): 1.0000

Verdict: **Insufficient**: insufficient data: do NOT claim learning improves search.

Evaluations-to-first-accept counts a miss as k. The random arm's interval treats its per-workload mean hit rate x n as successes.

## Descriptive statistics

| recipe | applicable | accepted (train) | accepted (unseen) | unseen applicable | origin (training workload) |
|---|---|---|---|---|---|
| unroll4 | 29 | 2/10 | 9/19 | 19 | f32_sqdist |
| unroll8 | 29 | 4/10 | 6/19 | 19 | f32_sqdist |
| iter-fold | 15 | 0/7 | 0/8 | 8 | none (not in library) |
| bounds-hoist | 15 | 0/7 | 0/8 | 8 | none (not in library) |
| branchless | 12 | 0/5 | 0/7 | 7 | none (not in library) |
| swar-u64 | 4 | 0/1 | 2/3 | 3 | none (not in library) |
| table | 4 | 2/2 | 1/2 | 2 | u8_count_vowel |
| fused-pass | 3 | 0/0 | 3/3 | 3 | none (not in library) |

Library of 3 recipes; 9 of 24 unseen workloads have at least one library recipe accepted (the ceiling for any proposer with this library). Mean number of accepting library recipes per unseen workload: 0.67 of 3. Budget k = 3.

### Exploratory, post hoc (NOT preregistered; cannot change the verdict)

The preregistered k = 3 equals the library size, so both arms evaluate the whole library and are identical by construction. Smaller budgets on the same outcomes:

| k | verdict | retrieval hit@k (Wilson 95%) | random hit@k (Wilson 95%) | p |
|---|---|---|---|---|
| 1 | NoDifference | 29.2% [14.9%, 49.2%] | 22.2% [10.1%, 42.0%] | 0.1349 |
| 2 | NoDifference | 37.5% [21.2%, 57.3%] | 33.3% [18.0%, 53.3%] | 0.1229 |

### Sensitivity (not the preregistered analysis)

Using first-run outcomes without the confirmation re-runs (0 borderline Accepts, 0 not confirmed): verdict Insufficient, retrieval 37.5% vs random 37.5%, p = 1.0000.

## Outcome matrix

| workload | split | unroll4 | unroll8 | iter-fold | bounds-hoist | branchless | swar-u64 | table | fused-pass |
|---|---|---|---|---|---|---|---|---|---|
| f32_sum | unseen | g 4.77 | g 8.32 | i 0.99 | i 0.99 | . | . | . | . |
| f32_dot | unseen | A 4.69 | A 7.33 | i 0.99 | i 0.99 | . | . | . | . |
| f32_sqdist | train | A 4.53 | A 7.21 | . | . | . | . | . | . |
| f32_l1 | train | A 4.53 | g 7.22 | i 1.00 | i 1.00 | . | . | . | . |
| f32_sumsq | train | g 4.76 | g 8.16 | i 0.99 | i 1.01 | . | . | . | . |
| f32_saxpy_sum | unseen | A 4.52 | g 7.22 | i 1.01 | i 0.99 | . | . | . | . |
| f32_maxabs | unseen | g 4.77 | g 7.93 | . | . | g 1.79 | . | . | . |
| f32_min | unseen | g 4.82 | g 8.16 | . | . | g 1.78 | . | . | . |
| f32_argmin | unseen | . | . | . | . | . | . | . | A 1.88 |
| f64_sum | unseen | g 4.01 | g 6.21 | . | . | . | . | . | . |
| f64_dot | unseen | A 3.60 | A 3.68 | . | . | . | . | . | . |
| f64_sqdist | unseen | A 3.65 | A 3.58 | i 1.00 | i 0.99 | . | . | . | . |
| f64_l1 | unseen | A 3.69 | g 3.73 | . | . | . | . | . | . |
| f64_sumsq | unseen | g 3.98 | g 6.91 | . | . | . | . | . | . |
| f64_saxpy_sum | unseen | A 3.69 | g 3.70 | . | . | . | . | . | . |
| f64_maxabs | train | g 3.51 | g 4.50 | i 1.00 | i 0.99 | i 0.89 | . | . | . |
| f64_min | train | g 4.04 | g 5.67 | i 1.00 | i 1.00 | i 0.88 | . | . | . |
| f64_argmin | unseen | . | . | . | . | . | . | . | A 1.88 |
| i32_sum | unseen | i 1.05 | i 1.02 | . | . | . | . | . | . |
| i32_dot | unseen | i 1.03 | g 1.11 | . | . | . | . | . | . |
| i32_l1 | train | i 0.72 | A 1.75 | . | . | . | . | . | . |
| i32_max | unseen | i 0.55 | i 1.02 | i 1.00 | i 1.00 | i 1.00 | . | . | . |
| i32_min | unseen | i 0.55 | i 1.02 | i 1.00 | i 1.00 | i 1.00 | . | . | . |
| i32_count_eq | train | i 1.08 | A 1.13 | i 0.99 | i 1.00 | i 1.00 | . | . | . |
| i32_argmin | unseen | . | . | . | . | . | . | . | A 1.97 |
| u8_sum | train | g 1.60 | i 0.94 | i 0.99 | i 0.97 | . | g 6.10 | . | . |
| u8_count_eq | unseen | A 1.88 | A 2.09 | . | . | i 0.99 | A 3.70 | . | . |
| u8_hamming | unseen | A 1.78 | A 2.67 | i 1.00 | i 1.00 | . | g 10.79 | . | . |
| u8_max | unseen | i 0.22 | i 0.53 | . | . | i 0.99 | . | . | . |
| u8_find_first | unseen | . | . | . | . | . | A 7.59 | . | . |
| u8_count_digit | unseen | A 1.66 | A 2.00 | i 1.00 | i 1.00 | i 1.00 | . | A 2.48 | . |
| u8_count_vowel | train | i 0.94 | i 1.09 | i 1.02 | i 0.98 | i 0.84 | . | A 2.09 | . |
| u8_count_space | train | i 0.98 | A 1.19 | . | . | i 0.81 | . | A 1.35 | . |
| u8_hist | unseen | . | . | . | . | . | . | i 0.62 | . |

Cell = outcome code and first-run speedup CI lower bound. A accept; c accept in the first run but not confirmed by the re-run; i insufficient improvement; g guard regression; H holdout failed; V visible tests failed; B build failed; U new unsafe; P dependency/capability; T timeout; M missing evidence; . recipe not applicable (not evaluated).

Compiler: rustc 1.98.1 (48a229cea 2026-09-01). Ranker: HashedFeatureRanker. Evaluations in matrix: 111.
