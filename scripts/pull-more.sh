#!/usr/bin/env bash
# Pull N more bare repos straight off the WACS Forgejo volume into dev-data/repositories.
#   WACS_HOST=<user>@<wacsdev-host> scripts/pull-more.sh 50 ['regex']
# Picks at random from dev-data/all-repos.txt (optionally grep -E filtered), skipping repos
# already pulled or listed in dev-data/failed.txt. Streams a tar over ssh through sudo, so
# nothing is written on the server and passwordless sudo isn't needed.
set -euo pipefail
cd "$(dirname "$0")/.."

N=${1:?usage: pull-more.sh <count> [regex]}
FILTER=${2:-.}
WACS_HOST=${WACS_HOST:?set WACS_HOST, e.g. <user>@<wacsdev-host>}
REMOTE_ROOT=${REMOTE_ROOT:-/var/lib/docker/volumes/wacs2_gitea_data/_data/git/repositories}
ALL=dev-data/all-repos.txt
DEST=dev-data/repositories
STAGE=dev-data/.incoming
FAILED=dev-data/failed.txt
mkdir -p "$DEST"; touch "$FAILED"

have() { (cd "$DEST" && find . -mindepth 2 -maxdepth 2 -name '*.git') | sed 's|^\./||; s|\.git$||'; }

batch=$(grep -E -- "$FILTER" "$ALL" \
  | awk 'FILENAME == ARGV[1] { skip[$0]; next } !($0 in skip)' <(have; cat "$FAILED") - \
  | awk 'BEGIN { srand() } { print rand() "\t" $0 }' | sort | cut -f2- | head -n "$N") || true

if [ -z "$batch" ]; then echo "nothing left to pull for filter '$FILTER'"; exit 0; fi
echo "pulling $(printf '%s\n' "$batch" | wc -l | tr -d ' ') repos from $WACS_HOST"

paths=$(printf '%s\n' "$batch" | while read -r r; do printf '%q ' "$r.git"; done)
progress() { if command -v pv >/dev/null; then pv; else cat; fi; }

# Log in once up front (prompts are clean here), then reuse that connection, so no
# password prompt ever lands underneath the progress bar.
CTL=(-o ControlPath=/tmp/pull-more-%C)
echo "ssh login to $WACS_HOST"
ssh "${CTL[@]}" -o ControlMaster=yes -o ControlPersist=15m -fN "$WACS_HOST"
trap 'ssh "${CTL[@]}" -O exit "$WACS_HOST" 2>/dev/null' EXIT

read -rsp "sudo password on $WACS_HOST: " pw; echo
printf '%s\n' "$pw" | ssh "${CTL[@]}" "$WACS_HOST" "sudo -S -p '' true" 2>/dev/null \
  || { unset pw; echo "sudo password rejected"; exit 1; }

rm -rf "$STAGE"; mkdir -p "$STAGE"
printf '%s\n' "$pw" \
  | ssh "${CTL[@]}" "$WACS_HOST" "sudo -S -p '' tar -C $REMOTE_ROOT --ignore-failed-read -cf - -- $paths" \
  | progress | tar -xf - -C "$STAGE" \
  || { unset pw; echo "transfer failed; nothing was moved, re-run to retry"; exit 1; }
unset pw

# Only a fully finished stream gets here, so a dropped connection never leaves half-copied repos in DEST.
(cd "$STAGE" && find . -mindepth 2 -maxdepth 2 -name '*.git') | while read -r p; do
  mkdir -p "$DEST/$(dirname "$p")"; mv "$STAGE/$p" "$DEST/$p"
done
rm -rf "$STAGE"

printf '%s\n' "$batch" | while read -r r; do
  [ -d "$DEST/$r.git" ] || { echo "missing $r (deleted/renamed since all-repos.txt?)"; echo "$r" >> "$FAILED"; }
done
echo "done: $(have | wc -l | tr -d ' ') repos in $DEST"
