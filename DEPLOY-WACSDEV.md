# Deploying the hub to WACS dev (<wacsdev-host>)

A plan, not yet run. Nothing has been done on wacsdev; Will runs every command
there. WACS-local (`dev/wacs/`) mimics it and is where each step below was
tried first.

## What is there

| | |
|---|---|
| Host | `<wacsdev-host>`, <wacsdev-host>; 9 CPU, 18 GB RAM, 192 GB free |
| Forgejo | container `wacs2-gitea-1`, `wycliffeassociates/wacs:forgejo15` (15.0.6), public at `https://content.wacsdev.org` |
| Repositories | volume `wacs2_gitea_data` → `/var/lib/docker/volumes/wacs2_gitea_data/_data/git/repositories` |
| Database | `gitea` on `wacs2-db-1` (MariaDB 10.11). No `dashboard_ro` user yet |
| Repos | about 32,000 (dev-data holds 3,542 of them) |

## The shape

One container, `translation-hub`, beside `wacs2-gitea-1` on the same Docker
network:

- **Reads** the repositories from Forgejo's volume, mounted read-only (`:ro,z`).
- **Reads** the repo list from MariaDB as `dashboard_ro`, which can only SELECT.
- **Writes** only through Forgejo's API, as the service account `hub-bot`, and only into the `TextTranslation` org. Production WACS is refused in code, whatever the configuration.
- **Talks to Forgejo** over the Docker network (`http://wacs2-gitea-1:3000`), so API calls and the git push for Create need no TLS.
- **Is reached by browsers** through the same reverse proxy as Forgejo, on its own host name. Sign-in sends people to `https://content.wacsdev.org`.
- **Keeps its own state** (SQLite, logs) on two volumes. Losing them loses:
  - ignores, published choices, pointers and languages, which publishers would redo;
  - nothing that Forgejo holds.

## Decided (Will, 2026-10-15)

1. **No DNS for the demo.** The hub is reached by IP on the VPN: `http://<wacsdev-host>:8787`. That is also `HUB_PUBLIC_URL` and the OAuth redirect. Forgejo's sign-in page stays at `https://content.wacsdev.org`. The port must be open to VPN clients. Over http the session cookie is not `Secure`, which is fine for a demo. A name later goes through Cloudflare (bttdev).
2. **Built on wacsdev from the GitHub checkout.**
   - The web build installs Sefer's editor packages from GitHub, and Sefer is private, so the build needs a GitHub token as a BuildKit secret: `docker build --secret id=gh,env=GH_TOKEN …`.
   - The Dockerfile has to read that secret before deploy day.
   - The fallback is building on the Mac for `linux/amd64` and `docker save | ssh … docker load`.
3. **Publishers are Forgejo's site admins:** `HUB_PUBLISHERS_TEAM=admins`, so no team is needed.
4. **TextTranslation does not exist on wacsdev.** Creating it is part of the one-time setup below, run by Will, never by the hub.
5. **Joe's hub is a separate demo.**
6. **Nothing writes on `compose up`.** The hub writes only when a publisher presses a button. The one-time setup (org, `hub-bot`, OAuth app) is a manual step.

## Steps (Will, on wacsdev)

**1. The read-only database user.** It's the same SQL as local, with a real password:
```sh
PW=$(openssl rand -hex 16)
sed "s/'dev'/'$PW'/" dev/db-init/10-readonly-user.sql \
  | docker exec -i wacs2-db-1 sh -c 'exec mariadb -uroot -p"$MARIADB_ROOT_PASSWORD"'
echo "HUB_FORGEJO_DB=mysql://dashboard_ro:$PW@wacs2-db-1/gitea" >> hub.env
```

**2. Publishing on Forgejo:** what `dev/wacs/publishing.py` does locally, against content.wacsdev.org with an admin token.
- The `TextTranslation` org (it does not exist on wacsdev yet). No `publishers` team: publishers are the site admins (`HUB_PUBLISHERS_TEAM=admins`).
- A user `hub-bot` in TextTranslation's Owners team, and its token, made with the Forgejo CLI:
  ```sh
  docker exec -u git wacs2-gitea-1 forgejo admin user generate-access-token \
    --username hub-bot --token-name translation-hub --scopes all --raw
  ```
- An OAuth2 application, `Translation Hub`, with the redirect `http://<wacsdev-host>:8787/v1/auth/callback`.

`publishing.py` can take the URL, admin token and container name as arguments, and print the env lines instead of writing `target/wacs/hub.env`. That's a small change, to make before deploy day.

**3. `hub.env`** (kept on the server, mode 600):
```sh
HUB_FORGEJO_DB=mysql://dashboard_ro:…@wacs2-db-1/gitea
HUB_BASE_URL=https://content.wacsdev.org            # clone links shown to people
HUB_FORGEJO_URL=http://wacs2-gitea-1:3000           # API and push, inside Docker
HUB_FORGEJO_PUBLIC_URL=https://content.wacsdev.org  # sign-in, in the browser
HUB_FORGEJO_TOKEN=…                                 # hub-bot
HUB_OAUTH_CLIENT_ID=…
HUB_OAUTH_CLIENT_SECRET=…
HUB_PUBLIC_URL=http://<wacsdev-host>:8787
HUB_PUBLISHERS_TEAM=admins
HUB_WRITE_HOSTS=wacs2-gitea-1,content.wacsdev.org
HUB_POLL_SECS=300
```

**4. Run it**, as a compose file beside wacs2's (the network name is wacs2's; check with `docker network ls`):
```yaml
services:
  hub:
    image: translation-hub:<git sha>
    restart: unless-stopped
    env_file: hub.env
    volumes:
      - /var/lib/docker/volumes/wacs2_gitea_data/_data/git/repositories:/repos:ro,z
      - hub-data:/data
      - hub-logs:/logs
    ports: ["8787:8787"]   # reached by IP on the VPN
    networks: [wacs2_default]
volumes: { hub-data: {}, hub-logs: {} }
networks: { wacs2_default: { external: true } }
```

**5. No proxy route for the demo:** browse `http://<wacsdev-host>:8787` on the VPN.

**6. Watch the first poll.**
- **Locally:** 3,542 repos scanned in 41 s and proofread in 14 s.
- **For about 32,000, expect:**
  - roughly 6–10 minutes for the first scan and the proofreading;
  - a few GB of SQLite, mostly findings;
  - the pages to work while it runs, filling in as the poll goes.
- **To follow it:** `docker logs -f` shows one `event:"poll"` line per round. `scripts/latency.py` reads the log volume.

**7. Verify:**
- `scripts/verify.py --base http://<wacsdev-host>:8787 --no-catalog`, if the dev-data-specific checks are skipped there. It needs a `--live` switch first, which is a small change.
- The browser smoke test against the host.
- Sign in as a publisher.
- On a throwaway language: create a published repo, pull a book, and publish a release. Check that the release has `SHA256SUMS` and `release.json`.

## Rollback

`docker compose down`, which leaves Forgejo untouched. To undo test writes, delete the `TextTranslation/*` repos made while testing, in Forgejo's UI. The hub never wrote anywhere else.

## Changes to make before deploy day (small)

- `publishing.py`: take a URL, an admin token and a container name, and print the env instead of writing locally.
- `verify.py --live`: drive a hub on real data without dev-data's fixed expectations.
- The Dockerfile reads a GitHub token as a BuildKit secret, for Sefer's private packages.
- The person's Forgejo tokens are kept with the session in the hub's SQLite (for starring). Encrypt them, or drop starring, before anyone outside the team signs in.
- The first poll on 32,000 repos is untested at that size. The local run suggests minutes, not hours. To be sure, run one locally on a larger dev-data slice first.

## Not in this plan

- Production WACS: the hub refuses to write there in code. Reading production is a later decision.
- The Port and BIEL feed of "published": BIEL still reads Primary from the catalog.
- Translating bulletins with NLLB: an idea only.
