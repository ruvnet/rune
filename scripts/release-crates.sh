#!/usr/bin/env bash
# Run on the trusted release host. Credentials must be injected by the host.
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-check}"
expected="${2:-}"
if [[ "$mode" != check && "$mode" != publish ]]; then
  echo 'Usage: bash scripts/release-crates.sh check|publish [expected-commit]' >&2
  exit 2
fi
if [[ -n "$(git status --porcelain)" ]]; then
  echo 'Release requires a clean checkout.' >&2
  exit 1
fi
actual="$(git rev-parse HEAD)"
if [[ "$mode" == publish && ( -z "$expected" || "$actual" != "$expected" ) ]]; then
  echo 'Publishing requires the exact approved commit as argument 2.' >&2
  exit 1
fi
if [[ "$mode" == publish ]]; then
  : "${CARGO_REGISTRY_TOKEN:?Inject the crates.io publishing token through the trusted host secret manager}"
fi
env -u CARGO_REGISTRY_TOKEN -u CARGO_REGISTRIES_CRATES_IO_TOKEN cargo package --workspace --exclude rune-study --locked
if [[ "$mode" == check ]]; then
  echo "Package verification passed for $actual; nothing published."
  exit 0
fi
for crate in rune-toolchain-core rune-toolchain-cargo rune-toolchain-edit rune-toolchain-eval rune-toolchain-memory rune-toolchain-target rune-toolchain-gc rune-toolchain; do
  # Exact package contents were verified above. Keep credentials out of builds.
  cargo publish -p "$crate" --locked --no-verify
done
echo "Published release from $actual. Run the registry install smoke check."
