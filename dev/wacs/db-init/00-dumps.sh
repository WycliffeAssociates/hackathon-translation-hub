#!/bin/bash
# dev-data's dumps of production's `repository` and topic tables go into their
# own database, `wacsdump`, never into Forgejo's `gitea`: dev/wacs/up.sh reads
# them to adopt each repo with its real owner, visibility, branch and topics.
set -e
mariadb -uroot -p"$MARIADB_ROOT_PASSWORD" -e "CREATE DATABASE wacsdump"
for f in /dumps/*.sql.gz; do
  echo "loading $f into wacsdump"
  zcat "$f" | mariadb -uroot -p"$MARIADB_ROOT_PASSWORD" wacsdump
done

