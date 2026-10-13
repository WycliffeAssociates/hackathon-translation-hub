#!/usr/bin/env bash
# The hub against stock Forgejo, end to end. Run from anywhere:
#
#   dev/forgejo/check.sh          start Forgejo 11 (rootless) + MariaDB 10.11, check, tear down
#   KEEP=1 dev/forgejo/check.sh   leave Forgejo and the hub running afterwards
#
# Checks, in order:
#   1. A git push bumps repository.updated_unix (HANDOFF.md asked; the poll's
#      fast path relies on it).
#   2. hub-server built with --features mariadb reads Forgejo's DB as
#      dashboard_ro with production's grants (SELECT on repository, topic,
#      repo_topic, release, attachment and nothing else).
#   3. The public repo is indexed and the private one is not.
#   4. A push shows up in the hub as a new HEAD within one poll.
#   5. A repo made private disappears from the hub on the next poll.
#
# Everything lives in target/forgejo* and the scratch DB; no real server, no
# dev-data. Needs docker, git, curl, python3, cargo.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
COMPOSE=(docker compose -f "$ROOT/dev/forgejo/compose.yml")
WORK="$ROOT/target/forgejo-check"
FORGEJO=http://127.0.0.1:3300
PASS=0 FAIL=0 HUB_PID=""

ok()   { echo "ok   $*"; PASS=$((PASS + 1)); }
bad()  { echo "FAIL $*"; FAIL=$((FAIL + 1)); }
sql()  { "${COMPOSE[@]}" exec -T db mariadb -uroot -proot gitea -N -e "$1"; }
api()  { curl -sf -H "Authorization: token $TOKEN" -H "Content-Type: application/json" "$@"; }
hub()  { curl -sf "http://127.0.0.1:$HUB_PORT$1"; }
json() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

cleanup() {
  if [[ -n "$HUB_PID" && -z "${KEEP:-}" ]]; then
    kill "$HUB_PID" 2>/dev/null || true
    wait "$HUB_PID" 2>/dev/null || true
  fi
  if [[ -z "${KEEP:-}" ]]; then
    "${COMPOSE[@]}" down -v >/dev/null 2>&1 || true
    rm -rf "$ROOT/target/forgejo"
  else
    echo "left running: Forgejo $FORGEJO (hubcheck / token in $WORK/token), hub pid $HUB_PID on :$HUB_PORT"
  fi
}
trap cleanup EXIT

rm -rf "$WORK" "$ROOT/target/forgejo" && mkdir -p "$WORK" "$ROOT/target/forgejo"

echo "starting Forgejo + MariaDB…"
"${COMPOSE[@]}" up -d --wait db >/dev/null
"${COMPOSE[@]}" up -d forgejo >/dev/null
for _ in $(seq 90); do curl -sf "$FORGEJO/api/v1/version" >/dev/null && break; sleep 2; done
echo "forgejo $(curl -sf "$FORGEJO/api/v1/version" | json 'd["version"]')"

"${COMPOSE[@]}" exec -T forgejo forgejo admin user create --admin --username hubcheck \
  --password "hubcheck-$RANDOM-pw" --email hubcheck@example.invalid --must-change-password=false >/dev/null
TOKEN=$("${COMPOSE[@]}" exec -T forgejo forgejo admin user generate-access-token \
  --username hubcheck --token-name check --scopes all --raw | tail -1)
echo "$TOKEN" > "$WORK/token"
api -X POST "$FORGEJO/api/v1/user/repos" -d '{"name":"ulb_test","auto_init":true,"private":false}' >/dev/null
api -X POST "$FORGEJO/api/v1/user/repos" -d '{"name":"secret_test","auto_init":true,"private":true}' >/dev/null

# Production's read-only account, from the dev DB init script.
"${COMPOSE[@]}" exec -T db mariadb -uroot -proot < "$ROOT/dev/db-init/10-readonly-user.sql"
denied=$("${COMPOSE[@]}" exec -T db mariadb -udashboard_ro -pdev gitea -N -e "SELECT count(*) FROM user" 2>&1 || true)
[[ "$denied" == *"denied"* ]] && ok "dashboard_ro cannot read \`user\` (production grants)" || bad "dashboard_ro can read user: $denied"

# --- 1. does a push bump updated_unix? --------------------------------------
push() {  # push a commit that adds $1 to ulb_test
  local dir="$WORK/clone"
  [[ -d "$dir" ]] || git clone -q "http://hubcheck:$TOKEN@127.0.0.1:3300/hubcheck/ulb_test.git" "$dir"
  printf '\\id %s\n\\c 1\n\\p\n\\v 1 One.\n' "$2" > "$dir/$1"
  git -C "$dir" add . && git -C "$dir" -c user.name=check -c user.email=check@example.invalid commit -qm "add $1"
  git -C "$dir" push -q origin HEAD 2>/dev/null
  git -C "$dir" rev-parse HEAD
}
before=$(sql "SELECT updated_unix FROM repository WHERE lower_name='ulb_test'")
sleep 2
sha1=$(push 08-RUT.usfm RUT)
sleep 1
after=$(sql "SELECT updated_unix FROM repository WHERE lower_name='ulb_test'")
(( after > before )) && ok "push bumps repository.updated_unix ($before → $after)" \
  || bad "push left updated_unix at $before"

# --- 2–3. the hub reads Forgejo as dashboard_ro -----------------------------
echo "building hub-server --features mariadb…"
cargo build -q --release -p hub-server --features mariadb --manifest-path "$ROOT/Cargo.toml"
HUB_PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1])")
HUB_DB="$WORK/hub.sqlite" HUB_LISTEN="127.0.0.1:$HUB_PORT" HUB_POLL_SECS=3 HUB_CATALOG_URL=off \
  HUB_REPO_ROOT="$ROOT/target/forgejo/git/repositories" \
  HUB_FORGEJO_DB="mysql://dashboard_ro:dev@127.0.0.1:3307/gitea" \
  HUB_LOG_DIR="$WORK/logs" "$ROOT/target/release/hub-server" > "$WORK/hub.jsonl" 2>&1 &
HUB_PID=$!
for _ in $(seq 60); do grep -q '"event":"poll"' "$WORK/hub.jsonl" 2>/dev/null && break; sleep 0.5; done
grep -q '"poll failed"' "$WORK/hub.jsonl" && bad "poll failed: $(grep '"poll failed"' "$WORK/hub.jsonl" | head -1)"

repos=$(hub "/v1/repos" | json '" ".join(r["owner"]+"/"+r["name"] for r in d["repos"])')
[[ "$repos" == *"hubcheck/ulb_test"* ]] && ok "public repo indexed through dashboard_ro" || bad "public repo missing: [$repos]"
[[ "$repos" != *"secret_test"* ]] && ok "private repo not indexed" || bad "private repo leaked: [$repos]"
hub "/v1/repos/hubcheck/secret_test" >/dev/null 2>&1 && bad "private repo answers" || ok "private repo is a 404"
head=$(hub "/v1/repos/hubcheck/ulb_test" | json 'd["head_sha"]')
[[ "$head" == "$sha1" ]] && ok "hub HEAD matches the push ($sha1)" || bad "hub HEAD $head, pushed $sha1"

# --- 4. a new push reaches the hub within a poll ----------------------------
sha2=$(push 31-OBA.usfm OBA)
seen=""
for _ in $(seq 40); do
  seen=$(hub "/v1/repos/hubcheck/ulb_test" | json 'd["head_sha"]')
  [[ "$seen" == "$sha2" ]] && break; sleep 0.5
done
[[ "$seen" == "$sha2" ]] && ok "new push picked up by the poll" || bad "hub still at $seen after pushing $sha2"
books=$(hub "/v1/repos/hubcheck/ulb_test" | json '",".join(sorted(b["book"] for b in d["books"]))')
[[ "$books" == "OBA,RUT" ]] && ok "books after the push: $books" || bad "books: $books"

# --- 5. made private → gone --------------------------------------------------
api -X PATCH "$FORGEJO/api/v1/repos/hubcheck/ulb_test" -d '{"private":true}' >/dev/null
gone=""
for _ in $(seq 40); do
  hub "/v1/repos/hubcheck/ulb_test" >/dev/null 2>&1 || { gone=1; break; }; sleep 0.5
done
[[ -n "$gone" ]] && ok "repo made private disappears on the next poll" || bad "private repo still served"

grep -q '"event":"request"' "$WORK"/logs/*.jsonl && ok "wide events written" || bad "no wide events"
echo
echo "$PASS passed, $FAIL failed · evidence in ${WORK#$ROOT/}"
(( FAIL == 0 ))
