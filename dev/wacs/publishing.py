"""Set WACS-local up for the hub's publishing proposal, and write the hub's
settings for it to target/wacs/hub.env. Safe to re-run.

- `TextTranslation`, the official org, with a `publishers` team (who may adopt,
  pull and publish through the hub).
- `hub-bot`, the service account that writes: an owner of the official org,
  with a token. People act through the hub; hub-bot writes and names them.
- `wacsadmin` (the admin up.sh made) in `publishers`; `translator`, a user who
  is not, to see the refusals.
- An OAuth app for the hub's sign-in, redirecting to HUB_PUBLIC_URL.

    python3 publishing.py URL ADMIN_TOKEN OUT_ENV HUB_PUBLIC_URL COMPOSE_FILE
"""

import json
import secrets
import subprocess
import sys
import urllib.error
import urllib.request

URL, TOKEN, OUT, HUB, COMPOSE = sys.argv[1:6]
ORG, TEAM = "TextTranslation", "publishers"


def api(method, path, body=None, token=TOKEN):
    req = urllib.request.Request(
        URL + "/api/v1" + path,
        method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"Authorization": "token " + token, "Content-Type": "application/json"},
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


def user(name, password):
    if api("GET", f"/users/{name}")[0] == 200:
        # Known password every run, so the one written to hub.env works.
        must(f"reset {name}", api("PATCH", f"/admin/users/{name}", {
            "password": password, "login_name": name, "source_id": 0, "must_change_password": False,
        }))
        return
    must(f"create {name}", api("POST", "/admin/users", {
        "username": name, "email": f"{name}@example.invalid", "password": password,
        "must_change_password": False, "send_notify": False,
    }))


passwords = {}
for name in ("hub-bot", "translator"):
    passwords[name] = f"{name}-{secrets.token_hex(6)}"
    user(name, passwords[name])

if api("GET", f"/orgs/{ORG}")[0] != 200:
    must("create org", api("POST", "/orgs", {"username": ORG, "visibility": "public", "full_name": "Official text translations"}))

teams = must("teams", api("GET", f"/orgs/{ORG}/teams"))
by_name = {t["name"]: t["id"] for t in teams}
if TEAM not in by_name:
    t = must("create team", api("POST", f"/orgs/{ORG}/teams", {
        "name": TEAM, "description": "May adopt, pull and publish through the Translation Hub",
        "permission": "read", "units": ["repo.code", "repo.releases"],
    }))
    by_name[TEAM] = t["id"]
must("hub-bot owns the org", api("PUT", f"/teams/{by_name['Owners']}/members/hub-bot"))
must("wacsadmin publishes", api("PUT", f"/teams/{by_name[TEAM]}/members/wacsadmin"))

# hub-bot's token, from the CLI (the API cannot mint one for another user).
bot_token = subprocess.run(
    ["docker", "compose", "-f", COMPOSE, "exec", "-T", "gitea", "forgejo", "admin", "user",
     "generate-access-token", "--username", "hub-bot", "--token-name", f"hub-{secrets.token_hex(3)}",
     "--scopes", "all", "--raw"],
    check=True, capture_output=True, text=True,
).stdout.strip().splitlines()[-1]

redirect = HUB.rstrip("/") + "/v1/auth/callback"
apps = must("oauth apps", api("GET", "/user/applications/oauth2"))
for a in apps:
    if a["name"] == "Translation Hub (local)":
        api("DELETE", f"/user/applications/oauth2/{a['id']}")
app = must("oauth app", api("POST", "/user/applications/oauth2", {
    "name": "Translation Hub (local)", "redirect_uris": [redirect], "confidential_client": True,
}))

with open(OUT, "w") as f:
    f.write(
        "# Written by dev/wacs/publishing.py: the hub's publishing settings for WACS-local.\n"
        f"HUB_FORGEJO_URL={URL}\n"
        f"HUB_FORGEJO_PUBLIC_URL={URL}\n"
        f"HUB_FORGEJO_TOKEN={bot_token}\n"
        f"HUB_OFFICIAL_ORG={ORG}\n"
        f"HUB_PUBLISHERS_TEAM={TEAM}\n"
        f"HUB_OAUTH_CLIENT_ID={app['client_id']}\n"
        f"HUB_OAUTH_CLIENT_SECRET={app['client_secret']}\n"
        f"HUB_PUBLIC_URL={HUB.rstrip('/')}\n"
        f"# sign in as: wacsadmin (publisher; password in target/wacs/admin) or translator / {passwords['translator']}\n"
    )
print(f"publishing ready: {ORG} with {TEAM}; settings in {OUT}")
print(f"translator / {passwords['translator']} (not a publisher)")
