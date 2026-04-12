#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
VERSION=$(python3 "$ROOT_DIR/scripts/bump_version.py" --manifest-path "$ROOT_DIR/Cargo.toml" --print-current)
ARCH=${TARGET_ARCH:-$(uname -m)}
ARTIFACT_BASENAME="etr-v${VERSION}-linux-${ARCH}"
STAGE_ROOT="$ROOT_DIR/target/release-bundle/${ARTIFACT_BASENAME}"
ARTIFACT_PATH="$ROOT_DIR/target/release-bundle/${ARTIFACT_BASENAME}.tar.gz"

mkdir -p "$ROOT_DIR/target/release-bundle"
rm -rf "$STAGE_ROOT"
mkdir -p "$STAGE_ROOT/bin" "$STAGE_ROOT/lib/etr" "$STAGE_ROOT/config" "$STAGE_ROOT/share/etr"

cargo build --release -p etrd

RUSTFLAGS="-C debuginfo=2 -C link-arg=--btf" \
CARGO_TARGET_BPFEL_UNKNOWN_NONE_LINKER=bpf-linker \
cargo +nightly build -p etr-ebpf \
  --target bpfel-unknown-none \
  -Z build-std=core \
  --release

cp "$ROOT_DIR/target/release/etrd" "$STAGE_ROOT/bin/etrd"
cp "$ROOT_DIR/target/bpfel-unknown-none/release/etr-ebpf" "$STAGE_ROOT/lib/etr/etr-ebpf"
cp "$ROOT_DIR/config/etr.toml.example" "$STAGE_ROOT/config/etr.toml.example"
cp "$ROOT_DIR/packaging/install.sh" "$STAGE_ROOT/share/etr/install.sh"
chmod 755 "$STAGE_ROOT/bin/etrd" "$STAGE_ROOT/share/etr/install.sh"
chmod 644 "$STAGE_ROOT/lib/etr/etr-ebpf" "$STAGE_ROOT/config/etr.toml.example"

tar -C "$ROOT_DIR/target/release-bundle" -czf "$ARTIFACT_PATH" "$ARTIFACT_BASENAME"

printf '%s\n' "$ARTIFACT_PATH"
