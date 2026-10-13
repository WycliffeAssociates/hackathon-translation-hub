#!/usr/bin/env bash
# Build translation-hub-handoff.zip: everything a teammate needs to unzip and
# run the hub, including what git does not hold.
#
#   scripts/handoff-zip.sh            → ./translation-hub-handoff.zip (gitignored)
#   scripts/handoff-zip.sh --test     … then unzip it into a fresh folder, build
#                                       and run scripts/verify.py there
#
# In: the working tree with .git (history, branch hub-core), dev-data/ (the
# 1,441 repos and the Forgejo dump; read-only), var/data/hub.sqlite (filled, so
# the first start skips the scan; it is refreshed first).
# Out: build output (target/, dist/), node_modules, logs, smoke screenshots,
# older zips.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
OUT="$ROOT/translation-hub-handoff.zip"
NAME=translation-hub

if [[ -n "$(git status --porcelain)" ]]; then
  echo "commit or stash first: the zip should match a commit" >&2
  git status --short >&2
  exit 1
fi

echo "refreshing var/data/hub.sqlite (one scan + catalog)…"
mkdir -p var/data
cargo build -q --release -p hub-server
PORT=$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1])")
HUB_DB=var/data/hub.sqlite HUB_LISTEN=127.0.0.1:$PORT ./target/release/hub-server > /dev/null 2>&1 &
PID=$!
trap 'kill $PID 2>/dev/null || true' EXIT
python3 - "$PORT" <<'EOF'
import json, sys, time, urllib.request
base = f"http://127.0.0.1:{sys.argv[1]}/v1/status"
for _ in range(240):
    try:
        s = json.load(urllib.request.urlopen(base, timeout=5))
        if s["state"].get("polled_unix") and s["languages"] > 0 and s["proofread"] > 0:
            print(f"  {s['scanned']} repos, {s['proofread']} proofread, {s['languages']} languages")
            sys.exit(0)
    except Exception:
        pass
    time.sleep(1)
sys.exit("hub did not finish its first poll and catalog in 4 minutes")
EOF
kill $PID; wait $PID 2>/dev/null || true
trap - EXIT
# Fold the WAL into the main file so the zip holds one consistent database.
python3 -c "import sqlite3; c=sqlite3.connect('var/data/hub.sqlite'); c.execute('PRAGMA wal_checkpoint(TRUNCATE)'); c.close()"

echo "zipping…"
rm -f "$OUT"
STAGE=$(mktemp -d)
ln -s "$ROOT" "$STAGE/$NAME"   # zip follows the link, so paths start with translation-hub/
(cd "$STAGE" && zip -qr "$OUT" "$NAME/" \
  -x "$NAME/target/*" "$NAME/var/logs/*" "$NAME/var/data/*-wal" "$NAME/var/data/*-shm" \
     "$NAME/*/node_modules/*" "$NAME/*/*/node_modules/*" "$NAME/web/dist/*" \
     "$NAME/spikes/*/dist/*" "$NAME/spikes/*/dist-shots/*" "$NAME/web/smoke-shots/*" \
     "$NAME/*.zip" "$NAME/.DS_Store" "$NAME/*/.DS_Store")
rm -rf "$STAGE"
echo "wrote $OUT ($(du -h "$OUT" | cut -f1)) at $(git rev-parse --short HEAD)"

if [[ "${1:-}" == "--test" ]]; then
  T=$(mktemp -d)
  echo "testing in ${T}…"
  (cd "$T" && unzip -q "$OUT")
  cd "$T/$NAME"
  git status --short | head -5
  (cd spikes/viewer && pnpm install | tail -1)
  (cd web && pnpm install | tail -1 && pnpm build | tail -1)
  python3 scripts/verify.py
  echo "the unzipped copy passed; remove it with: rm -rf $T"
fi
