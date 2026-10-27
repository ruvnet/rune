# Transfer study 2: learned algorithm selection for u16 sorting

This is a new, prospective synthetic study, not a revision of study/RESULT.md.
Scope is selection of existing algorithms within one sorting workload family.
It cannot establish autonomous algorithm invention or general compiler improvement.

Freeze this protocol and generator before any training or test evaluation.

Baseline is Rust stable slice sort, including input clone/allocation. Three
universally applicable, semantically equivalent candidates are unstable slice
sort, counting sort over the observed min/max, and two-pass byte radix sort.
No applicability filter can trivially choose the answer: all three handle every
u16 input and are tested on empty, singleton, odd, random and extreme inputs.

Training: all 12 combinations of length {64,1024,8192}, range {32,65536},
order {random,near}; seed 1701. Test: all 36 combinations of length
{96,1536,12288}, range {64,32768}, order {random,near}, seeds {2718,3141,5772}.
These are new lengths, ranges and seeds, with deliberately familiar structural
contexts. They are within-family transfer, not 36 independent algorithm families.
The inference population is these sorting contexts and input draws on this host.

The generator and candidate source are independent of outcomes. Fingerprint tags
are size bucket (<256, <4096, otherwise large), range bucket (<=256 or wide),
and input disorder (random or near). Values describe known benchmark context;
this study does not implement or benchmark automatic runtime feature extraction.

For each training context evaluate all candidates with the real rune-eval.
Retain only its fastest Accept, breaking exact ties alphabetically. A context
with no Accept adds no recipe. Store its accepted evidence summary and origin
fingerprint. Use the unchanged HashedFeatureRanker over sealed recipe records.
Freeze memory and all test choices before revealing test outcomes. k=1.

Controls use the same three applicable algorithms and one evaluation per test:
uniform random (exact expected success, not inflated Monte Carlo sample size),
and the fixed algorithm with most training Accepts (alphabetical tie break).
Compare memory against both. A real test report is required for every outcome.
Never substitute labels or predicted performance for measured evaluations.

Each EvalContract requires 40 paired samples after 5 warmups, strict lower
95% CI speedup >1.10, and a guard at n+7 with <=10% slowdown. Each printed timing
is the median of five batches, each calibrated to >=200us. This shorter timing
protocol trades precision for study duration; measured confidence gates still
apply, and raw pairs are retained. Same correctness holdout for all candidates.
All static policy restrictions remain enabled. Missing namespaces are explicitly
reported with --allow-degraded; no sandbox or production security claim follows.
Three workers, each pinned to a distinct allowed logical CPU; comparisons are
paired on that CPU. Shared-host drift is a limitation, not averaged away.

Primary success: >=20 test contexts, >=2 distinct retained strategies, memory
hit rate >= random +0.10 and >= training-selected fixed control +0.10;
one-sided paired sign-flip p<0.05 against each control (100000 draws, seed 8675309),
and memory's Wilson 95% lower bound above each control's upper bound. Otherwise
report NoDifference or Insufficient. No additional tuning, workload exclusions,
threshold relaxation or test-driven recipe changes in this study.

Report per-context outcomes so related cases and fixed-control strengths remain
visible. Any later tuning requires a new protocol and fresh test contexts.
