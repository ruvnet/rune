# ADR 007: AgenticOW isolation and safe cleanup

Status: Proposed
Date: 2026-10-26
Scope: Rune, the proposed learning toolchain for Rust

## Decision and rationale

Use an AgenticOW adapter for copy on write candidate workspaces after verifying its real API and isolation semantics. A filesystem fork alone is not a security sandbox.

Candidate execution receives restricted filesystem mounts, explicit CPU, memory and wall time quotas, no host secrets and network denied unless the declared job requires it. Build scripts and procedural macros are executable untrusted dependencies.

Track generated objects through an ownership manifest and content addressed store. Cleanup considers reachability, leases, active jobs and retention policy. Never classify source as disposable because it is old.

Quarantine unreachable generated objects for seven days before deletion. Source removal is a separate reviewed code change with reference checks and behavior validation. Resolve paths safely and reject symlink escapes and concurrent ownership changes.

Acceptance: cleanup preserves active build artifacts, source, user files and rollback plans; path traversal and symlink fixtures cannot escape the artifact root.

Current status: design only. No user files deleted and no AgenticOW sandbox claim made.

