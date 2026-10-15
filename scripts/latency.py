#!/usr/bin/env python3
"""Response times from the hub's wide events.

    scripts/latency.py LOG.jsonl [LOG.jsonl ...]     (or pipe JSON lines on stdin)

Reads the `event:"request"` lines hub-server writes (stdout or HUB_LOG_DIR
files) and prints, per route: count, p50, p95, max, and the database share;
then the slowest requests, and the poll and catalog rounds. Python stdlib only.
"""

import json
import sys
from collections import defaultdict


def events(paths):
    streams = [open(p, encoding="utf-8") for p in paths] or [sys.stdin]
    for stream in streams:
        for line in stream:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue


def pct(values, p):
    values = sorted(values)
    return values[min(len(values) - 1, int(round(p / 100 * (len(values) - 1))))]


def main():
    by_route = defaultdict(list)
    requests, rounds = [], []
    for e in events(sys.argv[1:]):
        kind = e.get("event")
        if kind == "request":
            span = e.get("span", {})
            row = {
                "route": span.get("route", "?"),
                "path": span.get("path", ""),
                "query": span.get("query", ""),
                "status": e.get("status"),
                "ms": float(e.get("duration_ms", 0)),
                "db_ms": float(span.get("db_ms", 0) or 0),
                "page": span.get("page", ""),
                # Where the time went, when the route says: its own work off
                # the async pool (git history, replay), a git push, Forgejo's API.
                "work_ms": span.get("work_ms"),
                "push_ms": span.get("push_ms"),
                "forgejo_calls": span.get("forgejo_calls"),
                "forgejo_ms": span.get("forgejo_ms"),
            }
            by_route[(span.get("method", "GET"), row["route"])].append(row)
            requests.append(row)
        elif kind in ("poll", "catalog"):
            rounds.append(e)

    if not requests and not rounds:
        print("no wide events found (is this a hub-server log?)", file=sys.stderr)
        return 1

    print(f"{'route':<52} {'n':>5} {'p50 ms':>8} {'p95 ms':>8} {'max ms':>8} {'db %':>5} {'5xx':>4}")
    for (method, route), rows in sorted(by_route.items(), key=lambda kv: -pct([r["ms"] for r in kv[1]], 95)):
        ms = [r["ms"] for r in rows]
        db = sum(r["db_ms"] for r in rows) / max(sum(ms), 1e-9) * 100
        errors = sum(1 for r in rows if (r["status"] or 0) >= 500)
        print(f"{method + ' ' + route:<52} {len(rows):>5} {pct(ms, 50):>8.1f} {pct(ms, 95):>8.1f} {max(ms):>8.1f} {db:>5.0f} {errors:>4}")

    print("\nslowest requests")
    for r in sorted(requests, key=lambda r: -r["ms"])[:8]:
        q = f"?{r['query']}" if r["query"] else ""
        split = [
            f"work {float(r['work_ms']):.0f}" if r["work_ms"] is not None else "",
            f"push {float(r['push_ms']):.0f}" if r["push_ms"] is not None else "",
            f"forgejo {r['forgejo_calls']}× {float(r['forgejo_ms']):.0f}" if r["forgejo_calls"] else "",
        ]
        where = " ".join(s for s in split if s)
        print(f"  {r['ms']:>8.1f} ms  {r['status']}  {r['path']}{q}  {('page=' + r['page']) if r['page'] else ''}{('  [' + where + ']') if where else ''}")

    if rounds:
        print("\npoll and catalog rounds")
        for e in rounds:
            keep = {k: v for k, v in e.items() if k not in ("timestamp", "level", "message", "target", "span", "spans")}
            print(f"  {e.get('timestamp', '')}  {json.dumps(keep)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
