#!/usr/bin/env bash
# Reproducibly build the bundled backend and record its hash.
#
# The output binary is byte-for-byte reproducible regardless of where the
# checkout or the cargo registry live, because the only machine-specific
# paths that would otherwise leak into the artifact (the registry path and
# the workspace path) are remapped to fixed relative prefixes.
#
# Usage: scripts/build-bundle.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
target="x86_64-unknown-linux-musl"

export RUSTFLAGS="${RUSTFLAGS:-} \
  --remap-path-prefix=${cargo_home}/registry/src=./registry/src \
  --remap-path-prefix=${repo_root}=."

cd "$repo_root"
cargo build --release --locked --target "$target"

out="$repo_root/target/$target/release/coolify-qs"
install -Dm755 "$out" "$repo_root/omarchy/bin/coolify-qs"
(
  cd "$repo_root/omarchy/bin"
  sha256sum coolify-qs > coolify-qs.sha256
  srcid="$("$repo_root/scripts/bundle-source-id.sh")"
  printf '%s  src Cargo.toml Cargo.lock rust-toolchain.toml\n' "$srcid" > coolify-qs.srcid
)

echo "bundled: omarchy/bin/coolify-qs"
sha256sum "$repo_root/omarchy/bin/coolify-qs"
echo "source-id: $(awk '{print $1}' "$repo_root/omarchy/bin/coolify-qs.srcid")"
