#!/usr/bin/env bash
# Verify that the committed bundled backend is exactly the reproducible build
# of the tracked Rust source. Exits non-zero on any mismatch, so CI can gate
# the bundle and reviewers can confirm the binary against this exact checkout.
#
# Usage: scripts/verify-bundle.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
target="x86_64-unknown-linux-musl"

expected_file="$repo_root/omarchy/bin/coolify-qs.sha256"
[[ -f "$expected_file" ]] || {
  echo "verify-bundle: missing $expected_file" >&2
  exit 1
}
expected="$(awk '{print $1}' "$expected_file")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

export RUSTFLAGS="${RUSTFLAGS:-} \
  --remap-path-prefix=${cargo_home}/registry/src=./registry/src \
  --remap-path-prefix=${repo_root}=."
export CARGO_TARGET_DIR="$tmp/target"

cd "$repo_root"
cargo build --release --locked --target "$target"

actual="$(sha256sum "$tmp/target/$target/release/coolify-qs" | awk '{print $1}')"
if [[ "$expected" != "$actual" ]]; then
  echo "verify-bundle: bundled binary does not match the reproducible build of the tracked source" >&2
  echo "  expected: $expected" >&2
  echo "  actual:   $actual" >&2
  echo "Run 'scripts/build-bundle.sh' to regenerate the bundle, then commit it." >&2
  exit 1
fi

echo "verified: omarchy/bin/coolify-qs matches the reproducible build ($actual)"
