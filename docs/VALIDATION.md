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
python3 scripts/openrouter-proposal.py
```

Missing credentials exit 2 without making a network request. Provider or schema failure exits 1. No global ruOS route changes are needed.

## Sources and evidence

1. https://github.com/ruvnet/agentic-flow/blob/main/docs/guides/AGENT-BOOSTER.md fetched through GitHub, blob 1f5c30fed4c7563d181fc3a23aa7b91cef8cfe3d. Documentation describes a local Rust and WASM transformation engine; published benchmark claims were not adopted as measurements.
2. npm registry metadata observed 2026-10-26: agent-booster 0.2.2, integrity sha512-9X8vNsx/Qnr7Bo6NrNji7Ccy9gUr5r0vTAm07dNYod/fefBy0HD/kyBFECST/sdbE6Kha5e/N3FOV9sTy22XCw==.
3. https://openrouter.ai/api/v1/models queried live on ruOS on 2026-10-26. Model discovery only, no paid inference.
4. https://github.com/OpenRouterTeam/docs/blob/main/guides/routing/provider-selection.mdx and https://openrouter.ai/blog/insights/ai-data-residency/ for routing policy fields. Endpoint availability under the proposed policy remains to be tested.
5. https://rustc-dev-guide.rust-lang.org/overview.html and https://doc.rust-lang.org/rustc/profile-guided-optimization.html for compiler boundaries and PGO.

## Implementation acceptance

Build an unchanged workspace, independently validate a kernel improvement of at least 10%, then replay the promoted build with agents and networking disabled. This package does not yet satisfy that implementation gate.

## Rune visual update

All four SVGs passed XML parsing, local reference checks, absence of scripts and external resources, and presence of accessible titles and reduced motion rules. All four static compositions were rendered with Sharp and visually inspected. Browser animation playback could not be tested because the local Playwright browser executable is absent. CSS animation behavior in the destination GitHub renderer remains unverified.
