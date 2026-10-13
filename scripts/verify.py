#!/usr/bin/env python3
"""Run the real hub on dev-data and check it the way a user would.

    scripts/verify.py                 build, launch, wait, drive every route, report
    scripts/verify.py --base URL      drive an already running hub instead (no launch)
    scripts/verify.py --keep          leave the launched server running afterwards

What it proves, end to end (not tests in isolation):
  1. hub-server builds and starts against dev-data with a fresh SQLite.
  2. The first poll finishes: every public repo scanned, Writer repos turned
     into synthetic repos, sous-chef run (from the `event:"poll"` wide event).
  3. Every /v1 route answers with the shapes and numbers the dev-data should
     give (en_ulb's books and lint, a synthetic repo's three files, a USFM
     file's bytes, a 404 for a private/unknown repo, BIEL conflicts once the
     catalog lands).
  4. Every request left a wide event with its duration, and p95 per route is
     under the budget (--budget-ms, default 250).
  5. dev-data was not written.

Evidence goes to target/verify/<timestamp>/: the server's logs, each route's
response body, and report.txt. The launched server is stopped by its own PID;
nothing else is touched. Python 3 stdlib only.
"""

import argparse
import json
import os
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Check:
    def __init__(self, out: Path):
        self.out, self.failures, self.lines = out, 0, []

    def ok(self, cond: bool, what: str, detail: str = ""):
        mark = "ok  " if cond else "FAIL"
        self.failures += 0 if cond else 1
        line = f"{mark} {what}" + (f"  ({detail})" if detail else "")
        self.lines.append(line)
        print(line, flush=True)
        return cond

    def note(self, line: str):
        self.lines.append(line)
        print(line, flush=True)


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def get(base: str, path: str, save: Path | None = None, page: str = "verify"):
    req = urllib.request.Request(base + path, headers={
        "traceparent": "00-" + os.urandom(16).hex() + "-" + os.urandom(8).hex() + "-01",
        "x-hub-page": page,
    })
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            status, body = res.status, res.read()
    except urllib.error.HTTPError as e:
        status, body = e.code, e.read()
    except (urllib.error.URLError, ConnectionError, TimeoutError):
        return 0, b"{}"  # not listening yet
    if save:
        save.parent.mkdir(parents=True, exist_ok=True)
        save.write_bytes(body)
    return status, body


def get_json(base, path, out, name):
    status, body = get(base, path, out / "responses" / f"{name}.json")
    try:
        return status, json.loads(body)
    except json.JSONDecodeError:
        return status, None


def wait_for(cond, timeout: float, every: float = 0.5):
    end = time.time() + timeout
    while time.time() < end:
        value = cond()
        if value:
            return value
        time.sleep(every)
    return None


def log_events(log_dir: Path):
    for f in sorted(log_dir.glob("*.jsonl")):
        for line in f.read_text(encoding="utf-8").splitlines():
            if line.startswith("{"):
                try:
                    yield json.loads(line)
                except json.JSONDecodeError:
                    pass


def newer_than(root: Path, marker: Path):
    since = marker.stat().st_mtime
    for dirpath, _, files in os.walk(root):
        for name in files:
            p = Path(dirpath) / name
            try:
                if p.stat().st_mtime > since:
                    return p
            except OSError:
                pass
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", help="drive a running hub at this URL instead of launching one")
    ap.add_argument("--keep", action="store_true", help="leave the launched server running")
    ap.add_argument("--budget-ms", type=float, default=250.0, help="p95 budget per route")
    ap.add_argument("--no-catalog", action="store_true", help="skip the public data API (offline)")
    args = ap.parse_args()

    stamp = time.strftime("%Y%m%d-%H%M%S")
    out = ROOT / "target" / "verify" / stamp
    log_dir = out / "logs"
    log_dir.mkdir(parents=True, exist_ok=True)
    c = Check(out)
    proc = None
    marker = out / "dev-data.marker"
    marker.touch()
    time.sleep(1.1)  # mtime resolution: anything written after this is newer

    try:
        if args.base:
            base = args.base.rstrip("/")
            log_dir = None
            c.note(f"driving {base} (not launched here; wide-event checks skipped)")
        else:
            c.note("building hub-server (release)…")
            build = subprocess.run(["cargo", "build", "--release", "-q", "-p", "hub-server"], cwd=ROOT)
            if not c.ok(build.returncode == 0, "cargo build --release -p hub-server"):
                return 1
            port = free_port()
            base = f"http://127.0.0.1:{port}"
            env = dict(os.environ,
                       HUB_DB=str(out / "hub.sqlite"),
                       HUB_LOG_DIR=str(log_dir),
                       HUB_LISTEN=f"127.0.0.1:{port}",
                       HUB_REPO_ROOT=str(ROOT / "dev-data" / "repositories"),
                       HUB_FORGEJO_DUMP=str(ROOT / "dev-data" / "sql" / "repository.sql.gz"))
            if args.no_catalog:
                env["HUB_CATALOG_URL"] = "off"
            web = ROOT / "web" / "dist"
            if web.is_dir():
                env["HUB_WEB_DIR"] = str(web)
            stdout = open(out / "stdout.jsonl", "w")
            proc = subprocess.Popen([str(ROOT / "target" / "release" / "hub-server")], cwd=ROOT, env=env,
                                    stdout=stdout, stderr=subprocess.STDOUT)
            c.note(f"launched pid {proc.pid} on {base}; evidence in {out.relative_to(ROOT)}")

        # --- ready: the server answers, then the first poll lands -------------
        up = wait_for(lambda: get(base, "/v1/status")[0] == 200, 30)
        if not c.ok(bool(up), "server answers /v1/status"):
            return 1
        polled = wait_for(lambda: json.loads(get(base, "/v1/status")[1]).get("state", {}).get("polled_unix"), 180)
        c.ok(bool(polled), "first poll finished")
        _, status = get_json(base, "/v1/status", out, "status")
        c.ok(status["scanned"] == status["repos"] and status["repos"] > 0, "every public repo scanned",
             f"{status['scanned']}/{status['repos']}")
        c.ok(status["synthetic"] > 0, "Writer repos served as synthetic repos", str(status["synthetic"]))
        c.ok(status["proofread"] > 0, "sous-chef ran", f"{status['proofread']} repos")

        # --- drive: what the pages call --------------------------------------
        code, feed = get_json(base, "/v1/repos?limit=5", out, "repos-feed")
        c.ok(code == 200 and len(feed["repos"]) == 5 and feed["total"] >= 5, "home feed", f"total {feed['total']}")
        heads = [r["head_unix"] or 0 for r in feed["repos"]]
        c.ok(heads == sorted(heads, reverse=True), "feed is newest first")

        code, en = get_json(base, "/v1/repos/wa-catalog/en_ulb", out, "repo-en_ulb")
        if c.ok(code == 200, "repo page: wa-catalog/en_ulb"):
            books = {b["book"]: b for b in en["books"]}
            c.ok(len(books) == 66, "en_ulb has 66 books", str(len(books)))
            c.ok(all(b["lint"] is not None for b in en["books"]), "every book has a lint summary")
            c.ok(en["proof"] is not None and en["proof"]["current"], "proofread at HEAD")

        code, tree = get_json(base, "/v1/repos/wa-catalog/en_ulb/tree", out, "tree-en_ulb")
        c.ok(code == 200 and any(f["path"] == "41-MAT.usfm" for f in tree["files"]), "file tree lists 41-MAT.usfm")
        code, body = get(base, "/v1/repos/wa-catalog/en_ulb/file/41-MAT.usfm", out / "responses" / "41-MAT.usfm")
        c.ok(code == 200 and body.startswith(b"\\id MAT"), "file bytes are the USFM", f"{len(body)} bytes")

        code, counts = get_json(base, "/v1/repos/wa-catalog/en_ulb/findings", out, "findings-en_ulb")
        totals = [x["total"] for x in counts or []]
        c.ok(code == 200 and totals and totals == sorted(totals, reverse=True)
             and {x["engine"] for x in counts} == {"lint", "sous"},
             "repo findings: lint and sous checks, sorted by count", f"{len(counts)} checks")
        sous = next(x for x in counts if x["engine"] == "sous")
        book = sous["books"][0]
        code, proof = get_json(base, f"/v1/repos/wa-catalog/en_ulb/proof?book={book['book']}&code={sous['code']}",
                               out, "proof-en_ulb-book-code")
        rows = proof["findings"] if proof else []
        c.ok(code == 200 and len(rows) == min(book["n"], 200)
             and all(f["book"] == book["book"] and f["code"] == sous["code"] and "from" in f for f in rows),
             "proof rows for one book and check match its count", f"{sous['code']} in {book['book']}: {len(rows)}")
        code, hub = get_json(base, "/v1/findings", out, "findings-hub")
        c.ok(code == 200 and hub and hub[0]["total"] >= hub[-1]["total"], "hub-wide checks sorted by count",
             f"top: {hub[0]['engine']} {hub[0]['code']} {hub[0]['total']}" if hub else "")
        code, top = get_json(base, f"/v1/findings/{hub[0]['engine']}/{hub[0]['code']}?limit=3", out, "findings-hub-top")
        c.ok(code == 200 and top and top[0]["n"] >= top[-1]["n"], "repos for the top check", 
             ", ".join(f"{r['owner']}/{r['name']} {r['n']}" for r in top or []))

        code, syn = get_json(base, "/v1/repos?synthetic=true&limit=1", out, "repos-synthetic")
        s = syn["repos"][0]
        code, stree = get_json(base, f"/v1/repos/{s['owner']}/{s['name']}/tree", out, "tree-synthetic")
        names = [f["path"] for f in stree["files"]]
        c.ok(stree["synthetic"] and "manifest.yaml" in names and ".upstreams.json" in names
             and any(n.endswith(".usfm") for n in names) and stree["source_files"],
             "synthetic repo serves manifest.yaml, NN-BOOK.usfm, .upstreams.json", f"{s['owner']}/{s['name']}")
        code, up = get_json(base, f"/v1/repos/{s['owner']}/{s['name']}/file/.upstreams.json", out, "upstreams")
        c.ok(code == 200 and up["upstreams"][0]["url"].endswith(f"{s['name']}.git"), ".upstreams.json points back at the Writer repo")

        code, _ = get_json(base, "/v1/repos/nobody/nothing", out, "repo-404")
        c.ok(code == 404, "unknown repo is a 404")

        code, langs = get_json(base, "/v1/languages?q=en", out, "languages")
        c.ok(code == 200 and langs and langs[0]["ietf"] == "en", "language search puts the exact code first")
        code, history = get_json(base, "/v1/repos/wa-catalog/en_ulb/books/MAT/history", out, "history-mat")
        c.ok(code == 200 and len(history) >= 1, "book history has a point")

        code, spec = get_json(base, "/v1/openapi.json", out, "openapi")
        c.ok(code == 200 and "/v1/repos/{owner}/{name}/tree" in spec["paths"], "OpenAPI document served")

        if not args.no_catalog:
            caught = wait_for(lambda: json.loads(get(base, "/v1/status")[1]).get("languages", 0) > 0, 120, 2)
            if c.ok(bool(caught), "catalog copy landed (public data API)"):
                code, ceb = get_json(base, "/v1/languages/ceb", out, "language-ceb")
                c.ok(code == 200 and "Binisaya" in ceb["facts"]["alternate_names"], "alternate names from the catalog")
                code, conflicts = get_json(base, "/v1/biel/conflicts", out, "biel-conflicts")
                langs = {x["language"] for x in conflicts}
                c.ok(code == 200 and len(conflicts) > 0, "BIEL shown-twice books",
                     f"{len(conflicts)} books in {len(langs)} languages (workTracker said 320/33 on 2026-10-12)")

        if web_dir_served(base):
            code, html = get(base, "/repos/wa-catalog/en_ulb")
            c.ok(code == 200 and b"<html" in html.lower(), "web app served with SPA fallback")

        # --- evidence: wide events --------------------------------------------
        if log_dir:
            time.sleep(1.5)  # the file writer is non-blocking
            evs = list(log_events(log_dir))
            reqs = [e for e in evs if e.get("event") == "request"]
            c.ok(len(reqs) > 10 and all("duration_ms" in e for e in reqs), "every request left a wide event with duration",
                 f"{len(reqs)} events")
            c.ok(any(e.get("span", {}).get("page") == "verify" for e in reqs), "x-hub-page recorded on the event")
            polls = [e for e in evs if e.get("event") == "poll"]
            if c.ok(bool(polls), "poll wide event"):
                p = polls[0]
                c.note(f"     poll: {p['rescanned']} rescanned in {p['scan_ms']} ms, {p['proofread']} proofread in "
                       f"{p['proof_ms']} ms (slowest {p['proof_slowest']} {p['proof_slowest_ms']} ms)")
            routes = {}
            for e in reqs:
                routes.setdefault(e["span"].get("route", "?"), []).append(float(e["duration_ms"]))
            for route, ms in sorted(routes.items()):
                ms.sort()
                p95 = ms[min(len(ms) - 1, int(round(0.95 * (len(ms) - 1))))]
                c.ok(p95 <= args.budget_ms, f"p95 {route}", f"{p95:.1f} ms over {len(ms)}")

        changed = newer_than(ROOT / "dev-data", marker)
        c.ok(changed is None, "dev-data not written", str(changed or ""))
    finally:
        if proc and not args.keep:
            proc.terminate()
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()
            c.note(f"stopped pid {proc.pid}")
        elif proc:
            c.note(f"left running: pid {proc.pid} at {base}")
        (out / "report.txt").write_text("\n".join(c.lines) + "\n", encoding="utf-8")

    print(f"\n{'PASS' if c.failures == 0 else f'{c.failures} FAILED'} · evidence in {out.relative_to(ROOT)}")
    return 0 if c.failures == 0 else 1


def web_dir_served(base: str) -> bool:
    code, body = get(base, "/")
    return code == 200 and b"<html" in body.lower()


if __name__ == "__main__":
    sys.exit(main())
