#!/usr/bin/env bash
# Shared Rust workspace checks. Independent apps have their own verification commands.
# LANPATH_RUST_GATE_FAST=1 skips workspace tests for the existing pre-commit hook.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
if [[ "${LANPATH_RUST_GATE_FAST:-0}" != 1 ]]; then
    cargo test --workspace
fi
