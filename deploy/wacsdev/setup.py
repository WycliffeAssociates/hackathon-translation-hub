"""One-time setup on WACS dev for the Translation Hub. Will runs it, once, on
<wacsdev-host> (DEPLOY-WACSDEV.md). Safe to re-run; it never touches anyone's
repos or account but hub-bot's.

- `TextTranslation`, the official org, if missing (public).
- `hub-bot`, the service account the hub writes as: an owner of TextTranslation,
  with a token made by Forgejo's CLI inside its container.
- An OAuth2 app, "Translation Hub", whose redirect is the hub's address
  (recreated on each run, so its secret is new and written below).
- `hub.env`, mode 600: everything the hub needs except the database password.

Publishers are Forgejo's site admins (HUB_PUBLISHERS_TEAM=admins): no team.

    ADMIN_TOKEN=… python3 deploy/wacsdev/setup.py \\
        --forgejo https://content.wacsdev.org \\
        --hub http://<wacsdev-host>:8787 \\
        --container wacs2-gitea-1 --out hub.env

ADMIN_TOKEN is a site admin's access token (Forgejo → Settings →
Applications), with scopes write:admin, write:organization, write:user.
Prints what it did and never prints a secret.
"""

import argparse
import json
import os
import secrets
import subprocess
import sys
import urllib.error
import urllib.request

PRODUCTION = ("content.bibletranslationtools.org", "git.door43.org")

ap = argparse.ArgumentParser()
ap.add_argument("--forgejo", required=True, help="Forgejo's public URL, e.g. https://content.wacsdev.org")
ap.add_argument("--internal", default="http://wacs2-gitea-1:3000", help="Forgejo as the hub's container reaches it")
ap.add_argument("--hub", required=True, help="the hub as a browser reaches it, e.g. http://<wacsdev-host>:8787")
ap.add_argument("--container", default="wacs2-gitea-1", help="Forgejo's container, for the token CLI")
ap.add_argument("--container-user", default="git", help="the user Forgejo runs as in its container")
ap.add_argument("--org", default="TextTranslation")
ap.add_argument("--out", default="hub.env")
args = ap.parse_args()

URL = args.forgejo.rstrip("/")
if any(h in URL for h in PRODUCTION):
    sys.exit(f"{URL} is production WACS: the hub is never set up to write there.")
TOKEN = os.environ.get("ADMIN_TOKEN") or sys.exit("set ADMIN_TOKEN (a site admin's access token)")


def api(method, path, body=None):
    req = urllib.request.Request(
        URL + "/api/v1" + path,
        method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"Authorization": "token " + TOKEN, "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = r.read()
            return r.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")[:300]


def must(what, result, ok=(200, 201, 204)):
    status, body = result
    if status not in ok:
        sys.exit(f"{what}: {status} {body}")
    return body


me = must("who am I", api("GET", "/user"))
if not me.get("is_admin"):
    sys.exit(f"{me.get('login')} is not a site admin; the setup needs one.")
print(f"as {me['login']} on {URL}")

if api("GET", f"/orgs/{args.org}")[0] != 200:
    must("create org", api("POST", "/orgs", {"username": args.org, "visibility": "public", "full_name": "Official text translations"}))
    print(f"created the org {args.org}")
else:
    print(f"{args.org} exists")

if api("GET", "/users/hub-bot")[0] != 200:
    # Nobody signs in as hub-bot; its password is random and never kept.
    must("create hub-bot", api("POST", "/admin/users", {
        "username": "hub-bot", "email": "hub-bot@translation-hub.invalid", "password": secrets.token_urlsafe(24),
        "must_change_password": False, "send_notify": False, "full_name": "Translation Hub",
    }))
    print("created hub-bot")
else:
    print("hub-bot exists")

teams = must("teams", api("GET", f"/orgs/{args.org}/teams"))
owners = next((t["id"] for t in teams if t["name"] == "Owners"), None)
if owners is None:
    sys.exit(f"{args.org} has no Owners team")
must("hub-bot owns the org", api("PUT", f"/teams/{owners}/members/hub-bot"))
print(f"hub-bot is an owner of {args.org}")

# The API cannot mint a token for another user; Forgejo's CLI in its container can.
bot_token = subprocess.run(
    ["docker", "exec", "-u", args.container_user, args.container, "forgejo", "admin", "user",
     "generate-access-token", "--username", "hub-bot", "--token-name", f"translation-hub-{secrets.token_hex(3)}",
     "--scopes", "all", "--raw"],
    check=True, capture_output=True, text=True,
).stdout.strip().splitlines()[-1]
print("made a token for hub-bot")

redirect = args.hub.rstrip("/") + "/v1/auth/callback"
for a in must("oauth apps", api("GET", "/user/applications/oauth2")) or []:
    if a["name"] == "Translation Hub":
        api("DELETE", f"/user/applications/oauth2/{a['id']}")
app = must("oauth app", api("POST", "/user/applications/oauth2", {
    "name": "Translation Hub", "redirect_uris": [redirect], "confidential_client": True,
}))
print(f"OAuth app 'Translation Hub' → {redirect} (owned by {me['login']})")

env = f"""# Translation Hub on WACS dev: written by deploy/wacsdev/setup.py. Keep it 600.
# Fill in the dashboard_ro password (DEPLOY-WACSDEV.md, step 1).
HUB_FORGEJO_DB=mysql://dashboard_ro:CHANGE-ME@wacs2-db-1/gitea
HUB_BASE_URL={URL}
HUB_FORGEJO_URL={args.internal}
HUB_FORGEJO_PUBLIC_URL={URL}
HUB_FORGEJO_TOKEN={bot_token}
HUB_OAUTH_CLIENT_ID={app['client_id']}
HUB_OAUTH_CLIENT_SECRET={app['client_secret']}
HUB_PUBLIC_URL={args.hub.rstrip('/')}
HUB_OFFICIAL_ORG={args.org}
HUB_PUBLISHERS_TEAM=admins
HUB_WRITE_HOSTS={args.internal.split('//')[1].split(':')[0]},{URL.split('//')[1]}
HUB_POLL_SECS=300
"""
fd = os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as f:
    f.write(env)
print(f"wrote {args.out} (mode 600): fill in the dashboard_ro password, then compose up")
