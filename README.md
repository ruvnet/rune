<p align="center"><img src="assets/rune-hero.svg" width="100%" alt="Rune. The learning toolchain for Rust. Architecture preview." /></p>

<p align="center"><strong>Source becomes software. Evidence earns promotion.</strong></p>

<p align="center"><a href="#the-contract">The contract</a> · <a href="#the-learning-loop">Learning loop</a> · <a href="#architecture-decisions">ADRs</a> · <a href="#try-the-validated-adapters">Validation</a></p>

Rune reimagines Cargo as a learning software toolchain. Declare behavior, hardware targets and constraints. Rune's proposed architecture explores implementations, validates candidates independently and freezes approved build plans for deterministic execution.

> **Phase 1 (v0.1.0).** `rune build`, `verify`, `explain` and `rollback` are implemented: frozen plans, offline replay, independent rebuild and rollback. `rune improve` runs one synthetic, schema-checked OpenRouter proposal and never applies code. Compiler integration, RuVector retrieval, isolated workers, MetaHarness evaluation and the autonomous optimizer are not implemented (ADR 008 Phases 2 to 4). The name is user selected; the crates.io package is `rune-toolchain` because `rune` belongs to the Rune language.

## The contract

<img src="assets/rune-build.svg" width="100%" alt="Proposed build graph: intent, locked plan, pinned build and independent verification." />

| Operation | Contract |
| --- | --- |
| Build | Replay approved inputs and toolchain without a model call |
| Improve | Explore candidates within explicit time, token and compute budgets |
| Verify | Evaluate against independently controlled correctness and security requirements |
| Promote | Bind approval to the exact tested artifact and policy |
| Explain | Show the recipe, measurements, cost and reasons behind a decision |
| Rollback | Restore the previous approved plan and artifact |

**CLI** (`cargo install rune-toolchain` installs the `rune` binary):

```text
rune build
rune improve
rune verify
rune explain
rune rollback
```

An example intent: build a Rust distance kernel for ARM and WASM, preserve its numerical contract, use at most 64 MB and spend at most $2 on reasoning. A candidate earns promotion only after independent evaluation. Cross compilation alone never establishes target performance.

## The learning loop

<img src="assets/rune-learning.svg" width="100%" alt="Proposed learning loop: RuVector recalls, OpenRouter proposes, Agent Booster applies, MetaHarness evaluates, a policy gate retains and memory records." />

| Component | Proposed Rune responsibility | Evidence boundary |
| --- | --- | --- |
| RuVector | Retrieve recipes by workload, hardware and policy fingerprint | Retrieval is not approval |
| OpenRouter | Generate bounded optimization proposals | Catalog queried; authenticated inference remains untested |
| Agent Booster | Apply explicit snippets locally through Rust and WASM | Rust fixture edit passed; semantics require separate checks |
| Ruflo | Coordinate bounded workers and job evidence | Integration proposed |
| AgenticOW | Supply isolated candidate workspace adapter | API and isolation semantics require validation |
| MetaHarness | Evaluate candidates independently | Integration proposed; optimizer cannot alter acceptance criteria |
| Autogenous | Govern candidate evolution under fixed policy | Integration proposed |

A recipe records where it worked, where it failed and what it cost. Transfer to another compiler, CPU or workload requires evaluation again. Ordinary builds use frozen recipes; expensive discovery runs separately.

## Artifacts that carry their evidence

<img src="assets/rune-artifact.svg" width="100%" alt="Illustrative evidence seal: source, dependencies, plan, toolchain, tests and artifact digest. RVF packaging and runtime adapters are proposed." />

Rune is the toolchain. RVF carries artifacts and evidence. RVForge authors and stages artifacts. RVM governs execution. These boundaries preserve the existing ecosystem rather than create a competing forge product.

| Target | Planned role | Current package status |
| --- | --- | --- |
| x86_64 | Native compilation and execution measurements | Not implemented |
| aarch64 | Native compilation and execution measurements | Not implemented |
| WASM | Portable build plus runtime validation | Not implemented |
| RVM | Execution adapter with explicit ABI and capability policy | Specification required |
| RVF | Portable evidence and artifact envelope | Integration proposed; ZIP is not RVF |

## Architecture decisions

* [ADR 001: Rune architecture and Cargo compatibility](docs/adr/001-system-boundary.md)

* [ADR 002: Bounded OpenRouter reasoning](docs/adr/002-openrouter-reasoning.md)

* [ADR 003: Agent Booster as the local edit engine](docs/adr/003-agent-booster.md)

* [ADR 004: RuVector evidence and transfer memory](docs/adr/004-learning-memory.md)

* [ADR 005: MetaHarness promotion and security contracts](docs/adr/005-independent-validation.md)

* [ADR 006: Hardware contracts and RVF artifacts](docs/adr/006-targets-artifacts.md)

* [ADR 007: AgenticOW isolation and safe cleanup](docs/adr/007-isolation-retention.md)

* [ADR 008: Delivery gates and economic acceptance](docs/adr/008-delivery-economics.md)

* [ADR 009: Rune identity and visual language](docs/adr/009-rune-identity.md)

## Try the validated adapters

Requirements: Node.js 18 or later for the Agent Booster fixture, Rust 1.88 or later for Rune itself.

```sh
npm install --ignore-scripts
npm run smoke
```

Agent Booster is pinned to version 0.2.2. Retain the generated dependency lockfile for repeatable installs. The Rust text replacement fixture passed on ruOS twice, reporting 22 ms and 33 ms. These are individual package reported observations, not latency guarantees or compiler speedups.

For one synthetic OpenRouter proposal, inject `OPENROUTER_API_KEY` through the host secret manager and run:

```sh
rune improve
```

The script never applies returned code. Missing credentials exit with status 2 before any request. Provider or schema failure exits with status 1. This is a smoke adapter; the full budget reservation controller and provider allowlist are future implementation work.

## Release gate

Build an unchanged Rust workspace. Independently confirm at least 10% improvement on a declared kernel metric without violating its correctness or security contract. Reproduce the approved artifact with agents and networking disabled. This remains the implementation acceptance gate.

Cleanup uses generated artifact ownership and reachability, with reversible quarantine. File age never authorizes source deletion. Proposal workers cannot modify evaluator policy or grant themselves permissions.

## Visual system

The geometric R mark, copper strokes and amber paths express composition and evaluation. The four SVGs are self contained and animate through CSS without scripts or external resources. Reduced motion disables animation; static compositions preserve the narrative. Animations are illustrative and are not connected to telemetry. Very narrow screens can open each SVG at full size for its labels.

[Validation receipts and source provenance](docs/VALIDATION.md) · [Visual identity decision](docs/adr/009-rune-identity.md)
