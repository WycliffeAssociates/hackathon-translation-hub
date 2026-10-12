# Languages, Primary and BIEL

**What:** the public data API's facts, copied into SQLite hourly: language names,
region, gateway flag, alternate names; per content row its status (Primary /
Active / Inactive), `show_on_biel`, and books. From those: which repo is
Official per book (Sefer's `defaultCandidate`) and which books BIEL shows twice
(two repos both Primary AND shown — workTracker's rule).

**Reach:** `GET /v1/languages?q=swahili`, `/v1/languages/ceb`, `/v1/biel/conflicts`; pages `/languages/ceb`, `/biel`.

**End state:** searching "swahili" finds `sw` by its English name; `ceb` lists
alternate names (Binisaya, …) and a `books` table with `state` and `official`;
`/v1/biel/conflicts` matches workTracker's
`shared/biel-duplicate-chapters-primary.ts` (320 books in 33 languages on
2026-10-12). Needs the catalog (`/v1/status` `languages > 0`).
