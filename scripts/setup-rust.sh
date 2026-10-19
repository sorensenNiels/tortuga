#!/usr/bin/env bash
# Cross-platform (Linux/macOS) Rust toolchain + cargo tools. Safe to re-run.
set -euo pipefail

if ! command -v rustup >/dev/null; then
  echo "==> Installing rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

cd "$(dirname "$0")/.."
echo "==> Installing toolchain from rust-toolchain.toml"
rustup show active-toolchain || rustup toolchain install

if ! command -v cargo-binstall >/dev/null; then
  echo "==> Installing cargo-binstall"
  curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
fi

echo "==> Installing cargo tools"
cargo binstall -y cargo-deny cargo-nextest cargo-packager trunk

echo "==> Done. Open a new terminal, then: make run"
