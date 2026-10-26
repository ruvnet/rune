# ADR 002: Bounded OpenRouter reasoning

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Use OpenRouter only in discovery for algorithm proposals and architecture critique. Model output is untrusted proposal data, never shell authority.

The live public catalog on 2026-10-26 returned moonshotai/kimi-k3.1-pro and moonshotai/kimi-k2.7-code. Pin explicit IDs per run and record returned model, provider, request ID, usage, latency and price snapshot. Catalog availability does not prove provider availability or reasoning quality.

Initial pilot policy: one request at a time, at most 4 requests, at most 20,000 input and 4,000 output tokens per request, 60 second timeout, $2 total reasoning allocation. These are proposed limits, not measured costs. Reserve conservative cost before dispatch and reconcile actual usage. Uncertain pricing or context size blocks dispatch.

K3.1 Pro catalog prices observed: $2 per million input and $10 per million output tokens. A 20,000 input plus 4,000 output request is approximately $0.08 excluding other charges; four such requests approximately $0.32. Refresh pricing before execution.

Provider policy requires data_collection=deny, zdr=true and require_parameters=true. Fail closed when no compliant endpoint exists. Restrict to explicitly approved provider IDs at deployment. Private source egress requires project policy; start with synthetic public fixtures. Never send credentials or evaluator holdouts.

Credential comes from host secret injection; do not retrieve secret values through chat or persist them in receipts. Do not alter the tenant's global model route just to run this project.

Acceptance: a synthetic prompt produces a schema validated proposal; missing credential produces an explicit blocked result; disallowed routing cannot fall back silently. Replay uses stored proposal bytes, never a fresh model response.

Current evidence: public catalog queried successfully. Authenticated inference blocked because no OPENROUTER_API_KEY is injected into the execution process, despite a stored provider slot. No claim that OpenRouter authored or reviewed these ADRs.

