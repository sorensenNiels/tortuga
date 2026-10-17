#!/usr/bin/env bash
# One-time developer setup for macOS. The app draws with OpenGL and uses native dialogs, so only Xcode CLT is needed.
set -euo pipefail

if ! xcode-select -p >/dev/null 2>&1; then
  echo "==> Installing Xcode Command Line Tools (finish the dialog, then re-run this script)"
  xcode-select --install
  exit 0
fi

"$(dirname "$0")/setup-rust.sh"
