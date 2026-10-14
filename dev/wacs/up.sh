#!/usr/bin/env bash
# Start WACS-local (dev/wacs/compose.yml) and, the first time, fill it with
# dev-data's repos the way they sit in production: copied into Forgejo's repo
# folder, adopted through the admin API under their real owners, then given
# their real visibility, default branch, description and topics from the dump.
# Safe to re-run: a repo Forgejo already has is skipped.
#
#   dev/wacs/up.sh              start + fill (first run ~5 min)
#   dev/wacs/up.sh --reset      throw WACS-local away (DB, Forgejo data) and start over
#
# dev-data is only read (APFS clones or reflinks where the disk allows).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
COMPOSE=(docker compose -f "$ROOT/dev/wacs/compose.yml")
DATA="$ROOT/target/wacs"
URL=http://127.0.0.1:3400

if [[ "${1:-}" == "--reset" ]]; then
  "${COMPOSE[@]}" down -v --remove-orphans
  rm -rf "$DATA"
fi
mkdir -p "$DATA/gitea/git/repositories" "$ROOT/var/wacs/data" "$ROOT/var/wacs/logs"

"${COMPOSE[@]}" up -d --wait db redis
"${COMPOSE[@]}" up -d gitea
for _ in $(seq 90); do curl -sf "$URL/api/v1/version" >/dev/null && break; sleep 2; done
echo "forgejo $(curl -sf "$URL/api/v1/version")"

# Production's read-only account. Table grants need the tables, which Forgejo
# creates on its first start, so this runs here rather than in the DB's init.
if ! "${COMPOSE[@]}" exec -T db mariadb -uroot -proot -N -e "SELECT 1 FROM mysql.user WHERE user='dashboard_ro'" | grep -q 1; then
  "${COMPOSE[@]}" exec -T db mariadb -uroot -proot < "$ROOT/dev/db-init/10-readonly-user.sql"
fi

if [[ ! -s "$DATA/admin" ]]; then
  PW="wacsadmin-$(openssl rand -hex 8)"
  "${COMPOSE[@]}" exec -T gitea forgejo admin user create --admin --username wacsadmin \
    --password "$PW" --email wacsadmin@example.invalid --must-change-password=false >/dev/null
  TOKEN=$("${COMPOSE[@]}" exec -T gitea forgejo admin user generate-access-token \
    --username wacsadmin --token-name setup --scopes all --raw | tail -1)
  (umask 077; printf 'password=%s\ntoken=%s\n' "$PW" "$TOKEN" > "$DATA/admin")
fi
TOKEN=$(sed -n 's/^token=//p' "$DATA/admin")

# Copy what is not there yet. -c clones on APFS (instant, no extra space).
echo "copying dev-data repos into Forgejo's folder…"
for owner in "$ROOT"/dev-data/repositories/*/; do
  o=$(basename "$owner")
  mkdir -p "$DATA/gitea/git/repositories/$o"
  for repo in "$owner"*.git; do
    dest="$DATA/gitea/git/repositories/$o/$(basename "$repo")"
    [[ -e "$dest" ]] && continue
    cp -cR "$repo" "$dest" 2>/dev/null || cp -R --reflink=auto "$repo" "$dest" 2>/dev/null || cp -R "$repo" "$dest"
  done
done

# The dump rows for exactly the repos on disk, as TSV.
"${COMPOSE[@]}" exec -T db mariadb -uroot -proot wacsdump -B -N -e "
  SELECT owner_name, name, is_private, default_branch, is_archived, IFNULL(description,''), IFNULL(topics,'[]')
  FROM repository" > "$DATA/dump.tsv"

python3 -I "$ROOT/dev/wacs/adopt.py" "$URL" "$TOKEN" "$DATA/dump.tsv" "$DATA/gitea/git/repositories"
echo "WACS-local: $URL (wacsadmin; password in target/wacs/admin)"
