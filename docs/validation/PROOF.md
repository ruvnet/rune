# Rune validation: authenticated promotion, workspace replay and memory selection

## Decision

Retain the authenticated promotion and workspace adapter fixes. The prospective
confirmation supports memory based algorithm selection on fresh input draws in
twelve synthetic u16 sorting contexts. Broad autonomous compiler improvement,
algorithm invention and transfer to independent kernel families are not proved.

Validation was completed locally on branch `proof/authenticated-evaluation`.
Publication was subsequently authorized by the user. The exact validated source
snapshot has tree `83c9f76c02a734fed40c05cf4a0d2c3cec72abef` and local tip
`9ccbbcd`. The original local commit history, including preregistrations, is
preserved in `history.bundle`. No release, deployment or merge is included.

## Measured selection result

Training used 12 contexts and three universally applicable sorting algorithms:
unstable sort, counting sort and two-pass radix sort. Every candidate was run
through the independent evaluator. Seven successful training memory records
covering three strategies were frozen. The unchanged HashedFeatureRanker chose
one algorithm per test before any test outcomes were exposed. Feature metadata
describes known size/range/order context; runtime feature extraction is not
implemented or included in these measurements. This is a study harness using
Rune's sealed recipe snapshot/ranking API, rather than a new autonomous proposer.

The original 34-kernel study's Insufficient and the new 36-case pilot's
NoDifference remain in the record. The pilot measured 50.0% versus 36.1% random
and 33.3% fixed, but its required Wilson intervals overlapped. Its acceptance
threshold was not relaxed. The separately preregistered confirmation used the
same frozen memory, twelve contexts and 24 fresh seeds, with no further training.

| Arm, budget one evaluation per case | Success | Wilson 95% |
| --- | --- | --- |
| Frozen training memory | 154/288 = 53.47% | [47.70%, 59.15%] |
| Uniform random among three applicable algorithms | Expected 35.53% | [30.23%, 41.22%] |
| Counting, selected by training accept count | 106/288 = 36.81% | [31.44%, 42.52%] |

Memory clears random by 17.94 percentage points and the fixed control by 16.67.
That is 50.49% and 45.28% relative success uplift. These are selection success
rates, not an application runtime speedup. Random is the exact expected rate
from all three measured outcomes, not thousands of independent Monte Carlo
workloads. Other fixed controls were radix 33.68% and unstable 36.11%; counting
was also the strongest fixed strategy observed in confirmation.

Both paired case and seed-block sign-flip tests returned Monte Carlo p about
0.00001 at 100000 draws (resolution floor), below the preregistered 0.0125
threshold for each control. Each seed block averages its twelve contexts.
All 24 seed blocks favored memory over both controls. Required interval
separation and >=10 percentage point margins also passed. The verdict is Better
under the frozen rule; no optional sample extension or test driven tuning occurred.

There were 36 training, 108 pilot and 864 confirmation evaluator runs: 1008
fresh reports in total. Independent auditing matched baseline source, candidate
source, contract structure, contract file, holdout and evaluator executable
digests for every report. Every accepted report had 40 paired samples, a strict
lower 95% speedup bound above 1.10 and a passing n+7 guard with at most 10%
slowdown. Correctness is checked against Rust stable sort on empty, singleton,
odd, random and extreme inputs. All strategies apply to every case, so simple
applicability filtering cannot account for the result.

Protocols and raw evidence:

* `study/RESULT.md`: original study, unchanged.
* `study-transfer/PREREGISTRATION.md`, `result.json`, `audit.json` and `reports/`.
* `study-transfer-confirm/PREREGISTRATION.md`, `result.json`, `audit.json`,
  `selection.json`, `selection-freeze.json`, `freeze.json` and `reports/`.

The pilot inputs were hash frozen before evaluation; its Git commit followed
the first training outputs. Confirmation inputs and choices were committed at
`e7dfc44` before any confirmation evaluation. Generated confirmation source is
reconstructed by the frozen generator and verified against freeze.json.

## Authenticated promotion

Previously, an invented unsigned Accept promoted an unchanged hello fixture;
the identical report could authorize x86_64 and aarch64. ADR 011 now requires a
domain separated Ed25519 receipt and a separately supplied trusted public key
outside the candidate workspace. Report bytes are read once and authenticated.
The measured baseline, compiler, target, policy and evaluator must match.

Executable regressions now reject unsigned reports, changed reports, tampered
receipt identity, wrong keys, candidate supplied trust anchors, ARM relabelling
and trusted-signer context mismatches. A real sqdist evaluator run produced an
Accept at 6.756x, 95% CI [6.455, 7.071], 40 paired samples; its receipt promoted
the exact candidate and rollback restored the original plan. CLI controls
against that real receipt rejected altered bytes, ARM relabelling, a wrong
public key, and degraded evidence without explicit opt in. The ephemeral private
key was removed; the retained public key and signature allow verification.

This proves authenticity checks assuming an uncompromised trusted signer. It
does not protect a signing key from malicious processes sharing its account.
Hostile build scripts require a separate signer service and stronger job
isolation. Keys are never generated or trusted automatically by Rune.

## Workspace replay

Two failing-before/passing-after regressions fixed completion-order dependent
artifact selection and duplicated relative -C target paths (ADR 012).

A snapshot of Rune's own workspace at e7dfc44 built offline with real locked
dependencies. Plain Cargo and Rune selected `rune-study` with SHA256
`46274af1e47e4dbfaac3fd24d3c38815289784de4a02a589d06b9ce512c82162`.
A fresh target-directory rebuild reproduced the same digest. The corrected
relative -C command used the intended target directory and matched that digest.
This is one explicitly selected workspace product, not the complete artifact set.

Targeted checks: 63 library unit tests (including four Cargo adapter tests),
23 CLI/promotion regression tests and 13 artifact envelope tests passed, 99
unique selected tests in total. Clippy passed for changed crates and all targets
with warnings denied. This was not a full platform or adversarial sandbox audit.

## Limits and reproduction

Host: AMD EPYC 9V74, x86_64 Linux, rustc 1.99.1 / LLVM 23.1.1, shared environment.
Network namespaces were unavailable. The strict evaluator correctly refused;
known fixture/study runs explicitly used --allow-degraded, reporting environment
scrubbing, plain copies, CPU/memory limits and timeouts. Production isolation is
not established. Ordinary Cargo offline mode restricts dependency resolution,
while existing build scripts remain trusted host code. No paid model call was used.

Training/pilot ran three core-pinned queues; confirmation used six. Timing
calibration, algorithms, memory and gate thresholds were frozen. Confirmation
invoked the prebuilt benchmark directly to avoid Cargo process startup per
sample. The timed operation and five-batch median were unchanged. Shared-host
load and within-family context reuse limit generalization.

```sh
cargo build -p rune-toolchain -p rune-toolchain-eval -p rune-toolchain-memory --locked
cargo test --workspace --lib --locked
cargo test -p rune-toolchain --test cli --test promote --test proof_boundary --locked
cargo test -p rune-toolchain-target --test envelope --locked
python3 study-transfer-confirm/run.py generate
python3 study-transfer-confirm/run.py analyze
python3 study-transfer-confirm/audit.py
```

The last three commands reconstruct frozen source, analyze retained reports and
audit their bindings. A fresh performance rerun requires preserving the original
reports first, then running `run.py run`; new machine outcomes may differ. Do not
replace the committed confirmation result with a new run or reinterpret this as
a general self improvement benchmark.

Next acceptance gate: freeze an independent real-project workload family and
demonstrate the same improvement over both random and training selected fixed
controls, with trusted job signing isolated from candidate execution.
