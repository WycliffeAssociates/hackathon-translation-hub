-- Starring from the hub is dropped (Will, 2026-10-15): the hub keeps no one's
-- Forgejo token. The columns 0010 added go, and every token stored with them.
-- People star on WACS; Your work reads stars publicly.
ALTER TABLE session DROP COLUMN access_token;
ALTER TABLE session DROP COLUMN refresh_token;
ALTER TABLE session DROP COLUMN token_expires;
