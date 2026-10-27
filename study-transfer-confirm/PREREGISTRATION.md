# Confirmation of sorting transfer, fixed memory and fresh seeds

Study-transfer retained its NoDifference result: 50% vs 36.1% random and 33.3%
training selected fixed, but the required Wilson intervals overlapped (n=36).
This confirmation does not change that verdict or reuse its test outcomes.

Freeze the exact seven training memory records from study-transfer/selection.json
without adding any test observations. Keep all algorithms, features, ranker,
thresholds, guard and holdout tests unchanged. Repeat the same twelve test
contexts on 24 new seeds: 100003 + 104729*i for i=0..23, giving 288 input cases.
No source, feature, memory or decision rule is tuned on confirmation outcomes.
The population is new input draws within those twelve synthetic sorting
contexts, not new algorithms, independent kernel families or real projects.

For execution efficiency invoke the prebuilt target/release/bench directly
instead of starting Cargo for every timing sample. Both sides are built by the
same evaluator first. This changes process startup overhead outside the printed
timing, not the timed operation, iteration calibration or five-batch median.
Run six sequential queues, each on its own allowed logical CPU. Every baseline
and candidate pair stays on its queue's CPU. Shared-host effects remain a limit.
Use 40 paired samples, five warmups, strict 95% CI lower bound >1.10 and the
same n+7 guard with at most 10% slowdown. Report missing network namespaces.

Freeze choices for all 288 inputs before evaluating any of them. All three
algorithms remain applicable to all cases. Random control is uniform over the
three algorithms with budget one. Fixed control remains counting, which won
the original TRAINING accept-count rule; report every other fixed algorithm's
outcome descriptively too, so the control's scope is clear.

Primary acceptance requires: >=2 distinct strategies in frozen memory; all 864
real evaluations and digest audits complete; memory success exceeds each of
random and training-selected fixed by >=0.10 absolute; its Wilson 95% lower bound
exceeds each control's upper bound; one-sided paired sign-flip p<0.0125 against
each control (100000 draws, seed 8675309). Also require p<0.0125 with each seed's
twelve contexts averaged into one block (24 blocks), so seeds are not presented
as independent algorithm families. No optional stopping or sample extension:
NoDifference remains possible. This is the final confirmation in this request;
if it fails, report failure rather than continuing to sample until significant.

The smaller confirmation alpha (0.025 split across two controls) is fixed before
confirmation evaluation. All earlier results, including the original study's
Insufficient and study-transfer's NoDifference, remain visible. Positive evidence
supports only measured memory based selection in this synthetic sorting setting.
