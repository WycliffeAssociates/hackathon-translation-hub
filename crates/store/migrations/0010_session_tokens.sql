-- A session keeps the person's Forgejo tokens (from the sign-in), so the hub
-- can act as them where Forgejo wants the person, not the service account:
-- starring a repo. The access token lasts an hour; the refresh token renews it.
-- The hub's own SQLite only; ended sessions are deleted with their tokens.
ALTER TABLE session ADD COLUMN access_token TEXT;
ALTER TABLE session ADD COLUMN refresh_token TEXT;
ALTER TABLE session ADD COLUMN token_expires INTEGER;
