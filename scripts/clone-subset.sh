#!/usr/bin/env bash
# Politely mirror-clone a list of Forgejo repos into the same layout Forgejo uses on disk.
#   FORGEJO_URL=https://<host> [PAUSE_MS=500] scripts/clone-subset.sh [list-file]
# List file: one owner/name per line. Re-runnable: already-cloned repos are skipped.
set -euo pipefail

FORGEJO_URL=${FORGEJO_URL:?set FORGEJO_URL, e.g. https://git.example.org}
LIST=${1:-dev-data/subset.txt}
DEST=${DEST:-dev-data/repositories}
PAUSE_MS=${PAUSE_MS:-500} # pause between clones: PAUSE_MS..2*PAUSE_MS
MAX_TRIES=${MAX_TRIES:-4}
FAILED=dev-data/failed.txt

export GIT_TERMINAL_PROMPT=0 # private/missing repo -> fail fast, never prompt for creds

while IFS= read -r repo || [ -n "$repo" ]; do
  [ -z "$repo" ] && continue
  target="$DEST/$repo.git"
  if [ -d "$target" ]; then echo "skip  $repo"; continue; fi
  mkdir -p "$(dirname "$target")"

  tries=0
  until git clone --mirror --quiet "$FORGEJO_URL/$repo.git" "$target.tmp"; do
    rm -rf "$target.tmp"
    tries=$((tries + 1))
    if [ "$tries" -ge "$MAX_TRIES" ]; then
      echo "FAIL  $repo"; echo "$repo" >> "$FAILED"; continue 2
    fi
    wait=$(( 2 * 2**tries + RANDOM % 3 )) # ~4s, 8s, 16s
    echo "retry $repo in ${wait}s"; sleep "$wait"
  done
  mv "$target.tmp" "$target" # half-finished clones never look complete
  echo "ok    $repo"

  ms=$(( PAUSE_MS + RANDOM % (PAUSE_MS + 1) ))
  sleep "$(( ms / 1000 )).$(printf %03d $(( ms % 1000 )))"
done < "$LIST"
