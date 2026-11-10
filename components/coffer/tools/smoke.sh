#!/bin/sh
# Coffer smoke test: builds everything, makes fixture books, boots the
# server on a scratch shelf, and checks the routes that matter.
#
#   sh tools/smoke.sh
#
# Needs: cargo, curl, and libzim (runtime for the server, headers for
# the fixture tool - see README). Exits nonzero on the first failure.

set -e
cd "$(dirname "$0")/.."

PORT="${COFFER_SMOKE_PORT:-27531}"
BASE="http://127.0.0.1:$PORT"
SCRATCH="$(mktemp -d)"
SRV_PID=""

cleanup() {
    [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null || true
    rm -rf "$SCRATCH"
}
trap cleanup EXIT

fail() { echo "smoke: FAIL - $1" >&2; exit 1; }
note() { echo "smoke: $1"; }

command -v cargo >/dev/null 2>&1 || PATH="$HOME/.cargo/bin:$PATH"
export PATH

# The library path incantation works on a stock install too.
if [ -z "$COFFER_LIBZIM_LIB" ]; then
    COFFER_LIBZIM_LIB="$(ldconfig -p | grep -m1 'libzim\.so' | awk '{print $NF}')"
    export COFFER_LIBZIM_LIB
fi
[ -n "$COFFER_LIBZIM_LIB" ] || fail "libzim runtime not found"

# Fixture tool: headers may live in the vendored extraction (this repo's
# own layout) or the system include path.
ZIM_INCLUDE=""
if [ -f third-party/libzim-dev/usr/include/zim/archive.h ]; then
    ZIM_INCLUDE="-I third-party/libzim-dev/usr/include"
fi
if command -v c++ >/dev/null 2>&1; then
    c++ -std=c++17 $ZIM_INCLUDE tools/make-test-zim.cpp -o "$SCRATCH/make-test-zim" \
        -l:libzim.so.8 2>/dev/null \
        || c++ -std=c++17 $ZIM_INCLUDE tools/make-test-zim.cpp -o "$SCRATCH/make-test-zim" -lzim
    mkdir -p "$SCRATCH/shelf"
    "$SCRATCH/make-test-zim" "$SCRATCH/shelf/test.zim" >/dev/null
    "$SCRATCH/make-test-zim" "$SCRATCH/shelf/test-noindex.zim" noindex >/dev/null
    note "fixtures built (one indexed, one not)"
else
    fail "no C++ compiler - install build-essential"
fi

cargo build 2>/dev/null || fail "cargo build"
note "workspace builds"

./target/debug/coffer-server --shelf "$SCRATCH/shelf" --port "$PORT" >"$SCRATCH/server.log" 2>&1 &
SRV_PID=$!
i=0
until curl -s -o /dev/null "$BASE/api/books" 2>/dev/null; do
    i=$((i + 1)); [ "$i" -gt 50 ] && fail "server did not come up: $(cat "$SCRATCH/server.log")"
    sleep 0.2
done
note "server up"

check() { # check <name> <needle> <url>
    body="$(curl -s "$BASE$3")"
    case "$body" in
        *"$2"*) note "ok - $1" ;;
        *) fail "$1: '$2' not in response from $3" ;;
    esac
}

count="$(curl -s "$BASE/api/books" | grep -o '"id"' | wc -l)"
[ "$count" -eq 2 ] || fail "expected 2 books aboard, saw $count"
note "ok - two fixture books aboard"

check "chrome served at /" 'id="addr"' "/"
check "shelf lists the fixture" 'Coffer Test Shelf' "/shelf"
check "article bytes served" 'whole household' "/book/1/coffer"
check "federated search finds indexed hit" 'Vesper, the Evening Star' "/api/search?q=star"
check "suggestion fallback covers the no-index book" '"book":0' "/api/search?q=star"

code="$(curl -s -o /dev/null -w '%{http_code}' "$BASE/book/1/nothere")"
[ "$code" = "404" ] || fail "missing article should 404, got $code"
note "ok - missing article 404s"

code="$(curl -s -o /dev/null -w '%{http_code}' -L "$BASE/book/1")"
[ "$code" = "200" ] || fail "book main redirect should land 200, got $code"
note "ok - book root redirects to its main entry"

echo "smoke: PASS"
