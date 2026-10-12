# Proofreading (sous-chef)

**What:** Sous Chef's corpus-relative findings over all of a repo's books
("[;] appears only 2 of 1,053 times"). Run by the poll whenever HEAD moves;
stored per repo. At most 500 findings per code are kept, the rest counted in
`dropped` (fa_opv alone has 235k word-final ZWNJ findings).

**Reach:** `GET /v1/repos/wa-catalog/en_ulb/proof`; the repo page's Proofreading section.

**Drive:** click a finding; the viewer opens the book at `from`..`to` (UTF-16
offsets into the raw file).

**End state:** en_ulb has ~173 findings (Casing, ExactNeighbor, …), each with
book/chapter/verse, offsets, message and evidence counts; the repo's `proof.current`
is true. Check the poll event's `proof_slowest_ms` stays in seconds, not minutes.
