#!/usr/bin/env bash
# Build the workspace and print the five mock-engine scenes.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "${root}"
cargo build --workspace --locked
"${root}/target/debug/dj" demo
