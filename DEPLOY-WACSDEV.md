# Deploying the hub to WACS dev

A plan, not yet run. Nothing has been done on wacsdev; Will runs every command
there. WACS-local (`dev/wacs/`) mimics it and is where each step below was
tried first.

## What is there

| | |
|---|---|
| Host | `<wacsdev-host>` (the VPN address; not kept in the repo); 9 CPU, 18 GB RAM, 192 GB free |
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
   - The web build installs Sefer's editor packages and scripture-kitchen from GitHub; both are public, so no token is needed.
   - The fallback is building on the Mac for `linux/amd64` and `docker save | ssh … docker load`.
3. **Publishers are Forgejo's site admins:** `HUB_PUBLISHERS_TEAM=admins`, so no team is needed.
4. **TextTranslation does not exist on wacsdev.** Creating it is part of the one-time setup below, run by Will, never by the hub.
5. **Joe's hub is a separate demo.**
6. **Nothing writes on `compose up`.** The hub writes only when a publisher presses a button. The one-time setup (org, `hub-bot`, OAuth app) is a manual step.

## Checked on wacsdev (Will, 2026-10-15)

Ubuntu; docker runs without sudo for the admin user; networks `wacs2_default` and `wacs2_backend` (the hub joins both); Forgejo runs as uid 1001, gid 1002 (the image is built with those); the database's root password is `MARIADB_ROOT_PASSWORD`; python3 3.12 and git are installed; port 8787 is free; 187 GB free.

## Steps (Will, on wacsdev)

Tried end to end in this shape against WACS-local (2026-10-15): the image
built from GitHub with no credentials (162 MB); `setup.py` against a real
Forgejo 15; the container through `deploy/wacsdev/compose.yml` on the Forgejo
stack's network with the repositories mounted read-only. It read the repo list
as `dashboard_ro`, sign-in redirected to Forgejo and back, and the first poll
scanned 3,544 repos in 70 s and proofread 3,239 in 20 s, with no failures.
**At wacsdev's ~32,000 repos, expect the first poll to take about 15 minutes**;
pages work meanwhile and fill in.

**0. The code.**
```sh
git clone https://github.com/<org>/wacs-dashboard-2 && cd wacs-dashboard-2 && git checkout hub-core
docker network ls | grep wacs        # the Forgejo stack's network; wacs2_default if compose's default
docker exec wacs2-gitea-1 id         # the user Forgejo runs as (git, uid 1000)
```

**1. The read-only database user**, with a real password:
```sh
PW=$(openssl rand -hex 16)
sed "s/'dev'/'$PW'/" dev/db-init/10-readonly-user.sql \
  | docker exec -i wacs2-db-1 sh -c 'exec mariadb -uroot -p"${MARIADB_ROOT_PASSWORD:-$MYSQL_ROOT_PASSWORD}"'
echo "$PW"   # goes into hub.env in step 2
```

**2. Publishing on Forgejo, once.** Make an access token as a site admin (Forgejo → Settings → Applications; scopes write:admin, write:organization, write:user), then:
```sh
ADMIN_TOKEN=… python3 deploy/wacsdev/setup.py \
  --forgejo https://content.wacsdev.org --hub http://<wacsdev-host>:8787 --container wacs2-gitea-1
```
- It creates TextTranslation and `hub-bot` (an owner of it), mints `hub-bot`'s token in the container, and makes the OAuth app "Translation Hub".
- It writes `hub.env` (mode 600, git-ignored).
- It refuses production WACS and checks the token is an admin's.
- Re-running it is safe; it makes a new OAuth secret and a new bot token.

Put the step-1 password into `hub.env` in place of `CHANGE-ME`.

**3. Build and run.**
```sh
docker build --build-arg HUB_UID=1001 --build-arg HUB_GID=1002 -t translation-hub:$(git rev-parse --short HEAD) .
TAG=$(git rev-parse --short HEAD) docker compose -f deploy/wacsdev/compose.yml up -d
docker logs -f $(docker compose -f deploy/wacsdev/compose.yml ps -q hub) | grep '"event":"poll"'
```
Nothing writes on start or while polling. The hub writes only when a publisher presses a button.

**4. The port.** Open 8787 to the VPN on <wacsdev-host>'s firewall, if it is closed, then browse `http://<wacsdev-host>:8787`.

**5. Verify.**
- Sign in as a site admin; the header shows "publisher".
- Watch `/status` fill in.
- On a throwaway language, choose or create a published repo, pull a book, and publish a release. The release has `SHA256SUMS` and `release.json`.

## Rollback

`docker compose down`, which leaves Forgejo untouched. To undo test writes, delete the `TextTranslation/*` repos made while testing, in Forgejo's UI. The hub never wrote anywhere else.

## Left open

- `verify.py --live` (driving a hub on real data without dev-data's fixed expectations) is not built. Step 5 is a by-hand check instead.
- The first poll at 32,000 repos is estimated from the 3,544-repo run, not measured.
- The hub keeps no one's Forgejo token: starring from the hub was dropped. People star on WACS, and Your work lists those stars.

## Not in this plan

- Production WACS: the hub refuses to write there in code. Reading production is a later decision.
- The Port and BIEL feed of "published": BIEL still reads Primary from the catalog.
- Translating bulletins with NLLB: an idea only.
