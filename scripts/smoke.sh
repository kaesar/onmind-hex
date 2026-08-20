#!/usr/bin/env bash
# Hex smoke test: builds with the `db` feature, boots the server with a temp SQLite
# DB and JWT_ENABLED, then exercises the hex4w-parity routes. Exits 1 on any failure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/hex"
PORT="${PORT:-3002}"
BASE="http://127.0.0.1:$PORT"
DB_PATH="$(mktemp -d)/hex-smoke.db"
LOG="$DB_PATH-hex.log"
SECRET="smoke-secret"
TOK=""
PASS=0
FAIL=0

check() {
  local name="$1" expect="$2" got="$3"
  if [[ "$got" == "$expect" ]]; then
    PASS=$((PASS + 1)); printf '  ok   %-28s %s\n' "$name" "$got"
  else
    FAIL=$((FAIL + 1)); printf '  FAIL %-28s expect=%s got=%s\n' "$name" "$expect" "$got"
  fi
}

# --- HS256 JWT {sub, role, exp} signed with $SECRET ---------------------------
gen_token() {
  python3 - "$SECRET" <<'EOF'
import hmac, hashlib, base64, json, sys, time
b = lambda d: base64.urlsafe_b64encode(d).rstrip(b"=").decode()
h = b(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
p = b(json.dumps({"sub": "smoke", "role": "ADMIN", "exp": int(time.time()) + 3600}).encode())
content = f"{h}.{p}".encode()
sig = b(hmac.new(sys.argv[1].encode(), content, hashlib.sha256).digest())
print(f"{h}.{p}.{sig}")
EOF
}

code() { curl -s -o /dev/null -w "%{http_code}" "$@"; }
body() { curl -s "$@"; }

echo "==> build (feature db)"
cargo build --manifest-path "$ROOT/Cargo.toml" --features db || exit 1

echo "==> boot server on :$PORT"
pkill -f "$BIN" 2>/dev/null
env PORT="$PORT" HEX_DB_PATH="$DB_PATH" JWT_SECRET="$SECRET" "$BIN" >"$LOG" 2>&1 &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null; rm -f "$DB_PATH" "$LOG"' EXIT
for _ in $(seq 1 20); do curl -sf "$BASE/api/v1/health" >/dev/null 2>&1 && break; sleep 0.25; done

echo "==> public utility routes (no auth)"
check "GET /api/v1/health"   200 "$(code "$BASE/api/v1/health")"
check "GET /api/v1/info"     200 "$(code "$BASE/api/v1/info")"
check "GET /health"          200 "$(code "$BASE/health")"

echo "==> JWT enforcement"
check "roles without token"  401 "$(code "$BASE/api/v1/roles")"
check "roles bad token"      401 "$(code -H 'Authorization: Bearer nope' "$BASE/api/v1/roles")"

TOK="$(gen_token)"
check "roles with JWT"       200 "$(code -H "Authorization: Bearer $TOK" "$BASE/api/v1/roles")"

echo "==> roles CRUD"
UNIQ="QA-$(date +%s)"
check "create $UNIQ"         201 "$(code -X POST -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d "{\"name\":\"$UNIQ\"}" "$BASE/api/v1/roles")"
check "duplicate -> 409"     409 "$(code -X POST -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d "{\"name\":\"$UNIQ\"}" "$BASE/api/v1/roles")"
check "duplicate ADMIN -> 409" 409 "$(code -X POST -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d '{"name":"ADMIN"}' "$BASE/api/v1/roles")"
check "short name -> 400"    400 "$(code -X POST -H "Authorization: Bearer $TOK" -H 'Content-Type: application/json' -d '{"name":"a"}' "$BASE/api/v1/roles")"
check "search?name=AD"       200 "$(code -H "Authorization: Bearer $TOK" "$BASE/api/v1/roles/search?name=AD")"
check "GET roles/1"          200 "$(code -H "Authorization: Bearer $TOK" "$BASE/api/v1/roles/1")"
check "GET roles/9999 -> 404" 404 "$(code -H "Authorization: Bearer $TOK" "$BASE/api/v1/roles/9999")"

echo "==> feature-gated routes stay 404 when disabled"
check "store/items -> 404"   404 "$(code -H "Authorization: Bearer $TOK" "$BASE/api/v1/store/items?bucket=b")"
check "store/items no token -> 401" 401 "$(code "$BASE/api/v1/store/items?bucket=b")"
check "email -> 401 (auth)"  401 "$(code -X POST "$BASE/api/v1/notifications/email")"

echo
echo "PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" -eq 0 ]]