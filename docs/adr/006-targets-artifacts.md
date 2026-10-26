# ADR 006: Hardware contracts and RVF artifacts

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Define native x86_64 and aarch64 plus WASM adapters first. Each target descriptor pins triple, CPU features, OS, linker, sysroot, runtime, ABI and test executor.

Cross compilation is compile evidence only. Performance and runtime correctness claims require execution on the target or clearly labeled emulator evidence. CPU flags cannot be reused across incompatible hosts.

RVM is a runtime adapter pending an explicit ABI, instruction and capability specification. RVF is an artifact envelope, not a hardware ISA. Until the actual RVF API and format are integrated, use an ordinary development evidence directory and never label a ZIP as a signed RVF.

Artifact envelope will bind source, dependencies, build plan, toolchain, tests, profiles, policy, signatures and executable digests. Signatures prove provenance and integrity, not program correctness.

Acceptance: mismatched ABI or missing CPU feature blocks loading; tampered artifact fails verification; missing target executor yields unvalidated target status.

Current status: target and RVF architecture proposed; no runtime integration or hardware optimization validated.

