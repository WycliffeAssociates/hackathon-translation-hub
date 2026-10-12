#!/usr/bin/env bash
# Pull release attachment files for the repos already in dev-data/repositories.
#   WACS_HOST=<user>@<wacsdev-host> scripts/pull-attachments.sh
# Needs the local db running with the release + attachment dumps loaded. Only attachments of
# published (is_draft=0) releases on public repos. Same ssh+sudo tar stream as pull-more.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

WACS_HOST=${WACS_HOST:?set WACS_HOST, e.g. <user>@<wacsdev-host>}
# Verify on the server first: Forgejo's [attachment] PATH, inside the wacs2_gitea_data volume.
REMOTE_ATTACH_ROOT=${REMOTE_ATTACH_ROOT:-/var/lib/docker/volumes/wacs2_gitea_data/_data/data/attachments}
REPOS=dev-data/repositories
DEST=dev-data/attachments  # same <uuid[0]>/<uuid[1]>/<uuid> layout Forgejo uses
STAGE=dev-data/.incoming-attachments
mkdir -p "$DEST"

have_repos() { (cd "$REPOS" && find . -mindepth 2 -maxdepth 2 -name '*.git') | sed 's|^\./||; s|\.git$||'; }
rel() { printf '%s/%s/%s\n' "${1:0:1}" "${1:1:1}" "$1"; }

# repo <tab> uuid <tab> size, for every published attachment on a public repo
rows=$(docker compose exec -T db mariadb -udashboard_ro -pdev gitea -N <<'SQL'
SELECT CONCAT(LOWER(r.owner_name), '/', r.lower_name), a.uuid, a.size
FROM attachment a
JOIN `release` rl ON rl.id = a.release_id
JOIN repository r ON r.id = rl.repo_id
WHERE rl.is_draft = 0 AND r.is_private = 0;
SQL
)

want=$(awk -F'\t' 'FILENAME == ARGV[1] { have[$0]; next } ($1 in have)' <(have_repos) <(printf '%s\n' "$rows") \
  | while IFS=$'\t' read -r _ uuid size; do [ -f "$DEST/$(rel "$uuid")" ] || printf '%s\t%s\n' "$uuid" "$size"; done)

if [ -z "$want" ]; then echo "no new attachments for the repos you have"; exit 0; fi
n=$(printf '%s\n' "$want" | wc -l | tr -d ' ')
mb=$(printf '%s\n' "$want" | awk -F'\t' '{ s += $2 } END { printf "%.0f", s / 1048576 }')
read -rp "$n attachments, ~${mb} MB (~$(( mb / 3 / 60 )) min at 3.5 MB/s). Pull? [y/N] " ok
[ "$ok" = y ] || exit 0

paths=$(printf '%s\n' "$want" | while IFS=$'\t' read -r uuid _; do printf '%q ' "$(rel "$uuid")"; done)
progress() { if command -v pv >/dev/null; then pv; else cat; fi; }

CTL=(-o ControlPath=/tmp/pull-more-%C)
echo "ssh login to $WACS_HOST"
ssh "${CTL[@]}" -o ControlMaster=yes -o ControlPersist=15m -fN "$WACS_HOST"
trap 'ssh "${CTL[@]}" -O exit "$WACS_HOST" 2>/dev/null' EXIT

read -rsp "sudo password on $WACS_HOST: " pw; echo
printf '%s\n' "$pw" | ssh "${CTL[@]}" "$WACS_HOST" "sudo -S -p '' true" 2>/dev/null \
  || { unset pw; echo "sudo password rejected"; exit 1; }

rm -rf "$STAGE"; mkdir -p "$STAGE"
printf '%s\n' "$pw" \
  | ssh "${CTL[@]}" "$WACS_HOST" "sudo -S -p '' tar -C $REMOTE_ATTACH_ROOT --ignore-failed-read -cf - -- $paths" \
  | progress | tar -xf - -C "$STAGE" \
  || { unset pw; echo "transfer failed; nothing was moved, re-run to retry"; exit 1; }
unset pw

# A finished stream only: copy staged files into place, then drop the stage.
(cd "$STAGE" && find . -type f) | while read -r f; do
  mkdir -p "$DEST/$(dirname "$f")"; mv "$STAGE/$f" "$DEST/$f"
done
rm -rf "$STAGE"
echo "done: $(find "$DEST" -type f | wc -l | tr -d ' ') attachment files in $DEST"
