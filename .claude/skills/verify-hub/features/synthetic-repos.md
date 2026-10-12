# Writer projects as synthetic repos

**What:** a Writer (translationStudio) repo is one book in chunk files. The hub
serves it as if consolidated: `manifest.yaml`, `NN-BOOK.usfm` (stitched with
Sefer's template), `.upstreams.json` pointing back at the Writer repo and
commit. It is not a real repo, and the page says so.

**Reach:** `GET /v1/repos?synthetic=true&limit=1` for an example; then its
`/tree` and `/file/.upstreams.json`; `/file/<path>?source=true` reads the real
Writer files listed in `source_files`.

**End state:** `tree.synthetic == true`, three files, `source_files` non-empty;
`.upstreams.json`'s `url` ends in `<name>.git` and `commit` equals `head_sha`;
the page shows the "not a real repo" notice and lint/coverage for the stitched
book.
