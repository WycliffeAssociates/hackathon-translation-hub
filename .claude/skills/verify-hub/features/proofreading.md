# Proofreading (sous-chef)

**What:** Sous Chef's corpus-relative findings over all of a repo's books
("[;] appears only 2 of 1,053 times"). Run by the poll whenever HEAD moves;
stored per repo. Every finding is counted per book and check
(`/findings`); rows are kept up to 200 per book and check (`/proof?book=&code=`),
since some scripts produce findings by the hundred thousand.

**Reach:** `GET /v1/repos/wa-catalog/en_ulb/findings` (counts, both engines),
`/proof?book=GEN&code=Casing` (rows); the repo page's Findings table; hub-wide
`/v1/findings` and the `/checks` page.

**Drive:** click a finding; the viewer opens the book at `from`..`to` (UTF-16
offsets into the raw file).

**End state:** en_ulb has ~173 findings (Casing, ExactNeighbor, …), each with
book/chapter/verse, offsets, message and evidence counts; the repo's `proof.current`
is true. Check the poll event's `proof_slowest_ms` stays in seconds, not minutes.
