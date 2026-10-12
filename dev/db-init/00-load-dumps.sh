#!/bin/bash
# Load the mariadb-dump files pulled from prod (dev-data/sql, mounted at /dumps).
set -e
for f in /dumps/*.sql.gz; do
  echo "loading $f"
  zcat "$f" | mariadb -uroot -p"$MARIADB_ROOT_PASSWORD" "$MARIADB_DATABASE"
done
