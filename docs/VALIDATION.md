# Rune validation and provenance

The original eight proposed ADRs define a complete replacement of Cargo orchestration with a Rust core, compatibility adapters, bounded discovery and deterministic approved builds.

Read docs/adr in numeric order. Initially prepared as a standalone architecture package, then published to ruvnet/rune on 2026-10-26. ADR numbers are scoped to Rune.

## Verified in this session

Agent Booster 0.2.2 was installed in an isolated ruOS project with lifecycle scripts disabled. Its WASM backed apply API returned the requested Rust fixture edit successfully, reporting 22 ms and confidence 0.9420393705368042. A JavaScript fixture reported 1 ms. These are single observations with no performance generalization and no semantic correctness proof.

OpenRouter's live public models endpoint returned moonshotai/kimi-k3.1-pro. The execution process has no injected OPENROUTER_API_KEY. Authenticated model reasoning was not run. The supplied OpenRouter script sends only a synthetic prompt, validates a narrow response schema and never applies its result. It is a one request smoke tool, not the proposed full budget and provider allowlist controller.

## Repeat the local edit smoke

```sh
npm install --ignore-scripts
npm run smoke
```

Pin and retain the resulting package-lock.json before production integration. The original ruOS validation directory retains its resolved lockfile; it is not embedded in this document package.

## OpenRouter smoke

Inject OPENROUTER_API_KEY through the host secret manager, then run:

```sh
rune improve
```

Missing credentials exit 2 without making a network request. Provider or schema failure exits 1. No global ruOS route changes are needed.

## Sources and evidence

1. https://github.com/ruvnet/agentic-flow/blob/main/docs/guides/AGENT-BOOSTER.md fetched through GitHub, blob 1f5c30fed4c7563d181fc3a23aa7b91cef8cfe3d. Documentation describes a local Rust and WASM transformation engine; published benchmark claims were not adopted as measurements.
2. npm registry metadata observed 2026-10-26: agent-booster 0.2.2, integrity sha512-9X8vNsx/Qnr7Bo6NrNji7Ccy9gUr5r0vTAm07dNYod/fefBy0HD/kyBFECST/sdbE6Kha5e/N3FOV9sTy22XCw==.
3. https://openrouter.ai/api/v1/models queried live on ruOS on 2026-10-26. Model discovery only, no paid inference.
4. https://github.com/OpenRouterTeam/docs/blob/main/guides/routing/provider-selection.mdx and https://openrouter.ai/blog/insights/ai-data-residency/ for routing policy fields. Endpoint availability under the proposed policy remains to be tested.
5. https://rustc-dev-guide.rust-lang.org/overview.html and https://doc.rust-lang.org/rustc/profile-guided-optimization.html for compiler boundaries and PGO.

## Implementation acceptance (0.2.0)

Rune 0.2.0 implements ADR 008 Phases 1 to 4 as far as this environment allows. Evidence comes from `cargo test --workspace` and from the studies named below, on one x86_64 Linux machine (Rust 1.98.1). "Met" means a test or measurement produced the number; fixture means a small purpose-built crate.

| ADR 008 gate | Status |
| --- | --- |
| One unchanged workspace builds; matches plain Cargo | Met on a fixture crate: identical artifact SHA-256 |
| Frozen replay works with agents disabled | Met: receipt records 0 model calls; passes with the network proxy pointed at a closed port; a test asserts the build path never reads credentials or writes the ledger |
| Rejection cases | Met: stale source, tampered plan, path-traversal ids, zero budget, missing plan, missing credential, stale edit hash, path and symlink escape, oversize edit, forged-looking report with mismatched digests |
| Rollback | Met: previous plan restored; rollback plans keep their artifacts through cleanup |
| Cost ledger reconciles | Met with a caveat: two live `rune improve` campaigns (`moonshotai/kimi-k3.1-pro`, total $0.0221 including one call that returned an unusable proposal) reconciled. The check compares the ledger with a log written from the same provider response, so it is not an independent billing check |
| Local edits: exact output and stale-hash rejection | Met for the exact-replace applier and for Agent Booster 0.2.2 (confidence 0.942, reported as placement evidence only) |
| One real kernel improves at least 10% under the independent gate | Met on a fixture: squared Euclidean distance over f32, 3.51x geometric mean, 95% CI [3.511, 3.513], 40 paired samples after 5 warmups, pinned core, load 1.66 on 32 CPUs. The CI reflects one session. This is a purpose-built kernel, not a real-world workspace |
| Hidden-edge-case candidate is rejected | Met: a candidate that drops the final partial chunk passes visible tests and fails the holdout |
| Faster candidate that expands permissions is rejected | Met for new unsafe, std::net use and a new dependency, rejected before the candidate is built |
| Marginal candidate is judged by its CI lower bound | Met, with a machine dependence: the fixture measured 1.083x (CI lower bound below 1.10, rejected) on the development machine and 1.117x (CI [1.101, 1.132], accepted) on a GitHub runner. Both outcomes follow the rule; the test asserts the rule, not a fixed verdict. The straddle case is also covered by a synthetic statistics test |
| Recipe selection p95 below 10 ms | Met on synthetic data: 10,000 records, p50 0.74 ms, p95 0.77 ms, p99 0.86 ms |
| Retrieval beats an equal-budget random control on unseen workloads | **Not met.** Preregistered study (`study/PREREGISTRATION.md`, sha256 `7f772787110f42ff89fdec09f9bdd406ffe6dd13975ba6601d0e05adf2df6ef6`): 34 kernels, 8 recipes, 111 real evaluations. The learned library collapsed to 3 recipes, equal to the budget k=3, so both arms evaluate the whole library and the verdict is `Insufficient`. Exploratory looks at k=1 and k=2 showed no distinguishable difference (n=24). See `study/RESULT.md` |
| Approval does not transfer across targets by similarity | Met: an x86_64 approval cannot become an aarch64 approval at similarity 1.0; compiler, policy, contract and evaluator changes invalidate approval keys |
| Mismatched ABI or missing CPU feature blocks loading; tampered artifact fails verification; missing executor is unvalidated | Met |
| aarch64 and WASM execution | Emulated only: aarch64 under `qemu-aarch64-static`, wasm32-wasip1 under Node WASI. Performance claims for them are refused. No aarch64 hardware peer was reachable |
| RVM execution | Not met: no ABI or capability specification exists; reports `unvalidated` |
| RVF packaging | Not met: the published `rvf-runtime` 0.3.2 cannot store an arbitrary payload. The envelope is an ordinary evidence directory, optionally ed25519-signed, and is not RVF |
| Cleanup preserves active artifacts, source, user files and rollback plans; traversal and symlink fixtures cannot escape | Met, including a seeded property loop that never collects anything reachable. Crash recovery between a rename and the manifest commit is not implemented |
| MetaHarness and AgenticOW integration | Not met. MetaHarness exposes no performance-evaluation API; AgenticOW is a copy-on-write vector store for agent memory, not a workspace sandbox. Rune ships its own evaluator and an `Evaluator` trait for a future adapter |
| Large real-world workspace with dependencies and build scripts | Not met: build parity and byte identity were shown on fixtures only |

Historical 0.2.0 limit, now addressed by ADR 011: promotion previously trusted unsigned reports; it now requires authenticated evaluator receipts and a separate trusted public key. Remaining limits: the candidate workspace is a plain copy on filesystems without reflinks; isolation relies on `timeout`, `prlimit` and `unshare` and the report lists the layers actually applied; build scripts and proc-macros are untrusted code and this is not a hardened sandbox.

`rune improve` was exercised live twice. Attempt one returned a 404 because the sole endpoint does not accept a `temperature` parameter under `require_parameters`; the request was changed without weakening the privacy fields. A later attempt spent $0.0102 and returned a proposal that failed the schema because reasoning consumed the token cap. Both are recorded in the ledger.
