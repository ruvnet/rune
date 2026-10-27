# ADR 011: Authenticated evaluation before promotion

Status: implemented; evidence in study-transfer and targeted promotion tests.

## Problem and decision

At commit 1db0b64 an unsigned, internally consistent fabricated Accept could
promote source. The same report could be relabelled for x86_64 and aarch64.
Approval key construction bound fields chosen at promotion, rather than proving
they matched the measured environment.

Require a detached Ed25519 evaluator receipt and a caller supplied trusted
public key for promotion. Never trust the embedded key as the trust anchor.
Use a distinct rune.evaluation/1 signature domain. Bind exact report bytes,
baseline source digest, rustc identity, native target descriptor, policy,
evaluator version, schema and whether isolation was degraded. The report binds
candidate and contract digests and the detailed evaluator evidence.

The native signing path observes the compiler before and after evaluation and
refuses changed source inputs. It requires the baseline to match the approved
plan, rejects local Cargo configuration overrides and keeps the supplied key
outside both source trees and the workspace. No key is generated automatically.
Cross target signed evaluation is unsupported until there is a trustworthy
target executor identity path. Native receipts cannot be relabelled for ARM or
WASM. CPU feature variants require their own evaluator integration.

Promotion snapshots report bytes once, validates the signature and identity,
and records that same digest and receipt. Context mismatches fail before plan
mutation. An explicit --allow-degraded-evidence is required for a receipt whose
evaluator reports unavailable isolation. Unsigned historical reports remain
readable as advisory memory, but cannot authorize promotion.

## Procedure

Use an evaluator owned private key and an independently distributed public key.
The signing account and key are part of the trust boundary.

```sh
rune eval --baseline BASE --candidate CAND --contract CONTRACT \
  --out report.json --signing-key /trusted/evaluator.key \
  --receipt-out receipt.json
rune promote --report report.json --receipt receipt.json \
  --trusted-key /trusted/evaluator.pub --contract CONTRACT --target x86_64
```

The approved plan in the eval workspace must describe BASE. After evaluation,
apply the candidate exactly to the promotion workspace and reference the same
baseline plan. The receipt authenticates measured source, not arbitrary build
outputs. A signature proves provenance, not semantic correctness or isolation.

## Boundaries and tradeoffs

This is integrity/authenticity enforcement, not a hardened multi user sandbox.
An attacker who controls the evaluator account or signing key can still forge
evidence. Candidate build scripts or proc macros running as that account can
reach the account's files; putting a key outside a source directory does not
prevent this. Production evaluators need a separate signer service or trusted
job attestation that keeps the key away from candidate processes. No claim that
this change alone safely executes hostile dependencies is permitted.

Promotion verifies source and compiler/target compatibility, not performance on
all CPUs with the same target triple. Hardware transfer still needs fresh
measurements. The current build plan does not prescribe every environment input
of Cargo; hermetic toolchain execution remains a separate requirement.

Cost is one signature per evaluation and one verification per promotion, with
no model call, new network dependency or paid service. Historical unsigned
reports must be re evaluated by the trusted runner. Rollback is unchanged.

## Acceptance

Reject unsigned Accept, changed report bytes, a wrong trust anchor, relabelled
target, signed compiler/policy/baseline/evaluator mismatches, and degraded
receipts without explicit opt in. A real accepted evaluator run must sign,
promote under its matching baseline, and roll back. Retain every raw receipt and
negative control. The signer is trusted; a compromised signer is an explicit
non goal of this patch.
