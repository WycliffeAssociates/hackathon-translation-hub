"""Adopt the repos copied into WACS-local's folder, as they are in production.

Forgejo finds bare repos it has no rows for ("unadopted") and the admin API
adopts them. Owners must exist first; the dumps hold no user table, so every
owner becomes an organisation. Adoption names a repo after its folder (lower
case), so the dump's spelling, visibility, branch, description, archive flag
and topics are applied afterwards. Used by dev/wacs/up.sh; stdlib only.

    python3 adopt.py URL TOKEN dump.tsv REPO_ROOT
"""

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

URL, TOKEN, DUMP, ROOT = sys.argv[1:5]


def api(method, path, body=None):
    # Forgejo deadlocks on its own access tables when repos are added side by
    # side (500, "Deadlock found"); trying again a moment later succeeds.
    for attempt in range(6):
        status, out = _api(method, path, body)
        if status != 500 or "eadlock" not in str(out):
            return status, out
        time.sleep(0.5 * (attempt + 1))
    return status, out


def _api(method, path, body=None):
    req = urllib.request.Request(
        URL + "/api/v1" + path,
        method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={"Authorization": "token " + TOKEN, "Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            raw = r.read()
            return r.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode(errors="replace")[:200]


q = urllib.parse.quote

# The dump's rows by folder (owner/name, lower case), for the repos on disk.
rows = {}
# newline="\n": a description may hold a bare \r, which Python would split on.
for line in open(DUMP, encoding="utf-8", newline="\n"):
    owner, name, private, branch, archived, desc, topics = line.rstrip("\n").split("\t")
    rows[(owner.lower(), name.lower())] = dict(
        owner=owner, name=name, private=private == "1", branch=branch,
        archived=archived == "1", desc=desc.replace("\\n", "\n").replace("\\t", "\t"),
        topics=json.loads(topics.replace("\\\\", "\\") or "[]"),
    )
on_disk = sorted(
    (o, n[:-4]) for o in os.listdir(ROOT) if os.path.isdir(os.path.join(ROOT, o))
    for n in os.listdir(os.path.join(ROOT, o)) if n.endswith(".git")
)
missing = [k for k in on_disk if k not in rows]
print(f"{len(on_disk)} repos on disk, {len(missing)} without a dump row")

failed = []

# Owners, with the dump's spelling.
owners = sorted({rows.get((o, n), {"owner": o})["owner"] for o, n in on_disk}, key=str.lower)
made = 0
status_of_org = {}
for owner in owners:
    status, _ = api("GET", f"/orgs/{q(owner)}")
    if status == 200:
        continue
    status, body = api("POST", "/orgs", {"username": owner, "visibility": "public"})
    if status == 201:
        made += 1
    elif status == 422 and "invalid username" in str(body):
        # Older than Forgejo's naming rules: production keeps them, a new
        # instance refuses them. Their repos are left out.
        status_of_org[owner.lower()] = "invalid"
    else:
        failed.append(f"org {owner}: {status} {body}")
invalid = sorted(o for o, v in status_of_org.items() if v == "invalid")
skipped = sum(1 for o, _ in on_disk if o in invalid)
print(f"{len(owners)} owners, {made} organisations created; {len(invalid)} names Forgejo refuses "
      f"({', '.join(invalid)}), so {skipped} repos are left out")


def adopt(key):
    o, n = key
    row = rows.get(key)
    owner = row["owner"] if row else o
    status, repo = api("GET", f"/repos/{q(owner)}/{q(n)}")
    if status != 200 and status_of_org.get(owner.lower()) == "invalid":
        return None  # counted once, with its organisation
    if status != 200:
        status, body = api("POST", f"/admin/unadopted/{q(owner)}/{q(n)}")
        if status not in (200, 204):
            return f"adopt {owner}/{n}: {status} {body}"
    if row is None:
        return None
    edit = {"private": row["private"], "description": row["desc"], "archived": row["archived"]}
    if row["branch"]:
        edit["default_branch"] = row["branch"]
    if row["name"] != n:
        edit["name"] = row["name"]
    status, body = api("PATCH", f"/repos/{q(owner)}/{q(n)}", edit)
    if status != 200:
        return f"edit {owner}/{n}: {status} {body}"
    if row["topics"]:
        status, body = api("PUT", f"/repos/{q(owner)}/{q(row['name'])}/topics", {"topics": row["topics"]})
        if status not in (200, 204):
            return f"topics {owner}/{row['name']}: {status} {body}"
    return None


with ThreadPoolExecutor(4) as pool:
    for i, err in enumerate(pool.map(adopt, on_disk), 1):
        if err:
            failed.append(err)
        if i % 200 == 0:
            print(f"  {i}/{len(on_disk)}")

print(f"{len(on_disk) - skipped} repos adopted or already there, {len(failed)} problems")
for line in failed[:30]:
    print("  " + line)
sys.exit(1 if failed else 0)
