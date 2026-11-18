#!/bin/sh
# Runs the tests, the slow ones only when asked: the window tests (bin/knotes/tests/window.rs)
# each launch the app, and all of them take minutes.
# Usage: scripts/test.sh             the fast ones: every crate's but the window's
#        scripts/test.sh deck card   the fast ones, then the window tests whose names hold a word
#        scripts/test.sh all         every test (before a commit that touches the window)
#        scripts/test.sh -p          how far a run of every test has got: tests passed, failed, time
set -eu
cd "$(dirname "$0")/.."
# A run of every test writes each test's line here, so its progress can be asked meanwhile.
log=target/test-all.log
passed() { grep -c ' \.\.\. ok$' "$log" || true; }
failed() { grep -c ' \.\.\. FAILED$' "$log" || true; }
if [ "${1:-}" = -p ]; then
    [ -f "$log" ] || { echo "no run of every test yet"; exit 1; }
    start=$(head -1 "$log")
    # The last run's count says how many there are to pass.
    of=$(cat target/test-all.count 2>/dev/null || echo "?")
    if grep -q '^took ' "$log"; then
        echo "done: $(passed) passed, $(failed) failed, $(grep '^took ' "$log")"
    else
        echo "running: $(passed) of $of passed, $(failed) failed, $(( $(date +%s) - start )) s so far"
    fi
    exit 0
fi
if [ "${1:-}" = all ]; then
    mkdir -p target
    start=$(date +%s)
    echo "$start" > "$log"
    status=0
    cargo test --workspace >> "$log" 2>&1 || status=$?
    echo "took $(( $(date +%s) - start )) s" >> "$log"
    [ "$status" -ne 0 ] || passed > target/test-all.count
    # What failed and why, then each test binary's time (the window's is the long one).
    grep -E ' \.\.\. FAILED$|panicked|left:|right:|^error' "$log" || true
    grep -E '^test result' "$log" | sed -E 's/.*finished in ([0-9.]+)s/\1/' | sort -rn | head -3 | paste -s -d' ' - | sed 's/^/longest test binaries, s: /'
    echo "passed $(passed), failed $(failed), $(grep '^took ' "$log")"
    exit "$status"
fi
cargo test -q --workspace --exclude knotes
cargo test -q -p knotes --bins
if [ $# -gt 0 ]; then
    cargo test -q -p knotes --test window -- "$@"
fi
