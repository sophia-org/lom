#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 -B -m unittest discover -s "$root/tools/tests" -p 'test_*.py'
python3 -B "$root/tools/audit_source_layout.py"

cd "$root"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-D warnings"
RUSTDOCFLAGS="${RUSTDOCFLAGS:+$RUSTDOCFLAGS }-D warnings"
export RUSTFLAGS RUSTDOCFLAGS
unset LOM_UPDATE_SNAPSHOTS DISPLAY WAYLAND_DISPLAY XAUTHORITY
cargo fmt --all -- --check
python3 -B tools/audit_dependencies.py
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
