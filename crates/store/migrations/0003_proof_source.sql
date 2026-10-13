-- Sous Chef's source-compared lanes (length, presence, source copy) read the
-- repo's declared source text, so a run depends on the source's HEAD too: the
-- poll reruns a repo's proofreading when either moves. NULL: no source.
ALTER TABLE proof ADD COLUMN source_repo TEXT;  -- owner/name, e.g. wa-catalog/en_ulb
ALTER TABLE proof ADD COLUMN source_sha  TEXT;  -- the source's HEAD the run read
