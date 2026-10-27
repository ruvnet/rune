# 0.3.0 release candidate

Status: prepared, not published. All eight published packages move together to
0.3.0; rune-study remains non-publishable.

## Migration

Promotion now requires --receipt and --trusted-key. Reevaluate old unsigned
reports on a trusted runner; their advisory recipe data remains readable.
Generate a receipt with rune eval --signing-key KEY --receipt-out RECEIPT --out
REPORT. Keep the private key and trusted public key outside candidate workspaces.
Explicit opt in is required at both evaluation and promotion for degraded
isolation. See ADR 011 for the signer trust boundary.

Workspace product selection is deterministic and relative -C paths are resolved
once. See ADR 012. Learning evidence is scoped to the confirmation described in
docs/validation/PROOF.md; the version change does not broaden that claim.

## Release gate

Require green tests, Rust 1.88 compatibility, cargo audit and cargo deny for the
exact merged release source. Modern Cargo is used to package the workspace into
a temporary registry and verify the eight unpublished interdependent packages.
Run bash scripts/release-crates.sh check on a clean checkout before publishing.

Inject CARGO_REGISTRY_TOKEN through the trusted host secret manager. Never put
the value in source, command arguments, screenshots or logs. Run bash
scripts/release-crates.sh publish APPROVED_COMMIT on the trusted ruOS release
host. The script rejects a dirty tree or a different commit, verifies all
packages, then publishes in dependency order without rebuilding with the token.
Registry authentication and permission are still required; preparation cannot
manufacture a publishing credential. Partial publication is possible; if a
registry upload fails, inspect published versions before retrying.

After publishing, install rune-toolchain exactly at 0.3.0 from crates.io in a
fresh environment. Require rune --version to report 0.3.0, fixture build and
verify to pass, a genuine signed evaluation to promote and roll back, and the
unsigned-report regression to remain rejected. Only then mark this release
published and create the release tag against the approved commit.
