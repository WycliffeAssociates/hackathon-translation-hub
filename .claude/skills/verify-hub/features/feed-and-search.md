# Recent feed and search

**What:** the home page lists repos newest HEAD first, like Forgejo's explore
page, with filters (subject, shape, synthetic, topic) and a text search.

**Reach:** `GET /v1/repos?limit=20`, `?subject=Bible&q=ulb`, `?topic=consolidated`; pages `/`, `/search?q=…`.

**End state:** `total` equals the scanned repo count with no filters; `head_unix`
descends; each filter narrows `total`; search matches owner/name, title and
language code. p95 well under 50 ms on dev-data (`scripts/latency.py`).
