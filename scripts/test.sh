#!/bin/sh
# Runs the tests, the slow ones only when asked: the window tests (bin/knotes/tests/window.rs)
# each launch the app, and all of them take minutes.
# Usage: scripts/test.sh             the fast ones: every crate's but the window's
#        scripts/test.sh deck card   the fast ones, then the window tests whose names hold a word
#        scripts/test.sh all         every test (before a commit that touches the window)
set -eu
cd "$(dirname "$0")/.."
if [ "${1:-}" = all ]; then
    exec cargo test -q --workspace
fi
cargo test -q --workspace --exclude knotes
cargo test -q -p knotes --bins
if [ $# -gt 0 ]; then
    cargo test -q -p knotes --test window -- "$@"
fi
