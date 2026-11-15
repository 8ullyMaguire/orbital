-- Undo the X-follows import of 2026-11-15: removes only members created by it and never claimed/logged in.
\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE undo(handle text) ON COMMIT DROP;
\copy undo FROM '/tmp/orbital-undo.csv' WITH (FORMAT csv)
DELETE FROM users u USING undo x WHERE u.handle = x.handle
  AND u.last_login_at IS NULL AND u.claimed_at IS NULL AND u.is_owner = false;
COMMIT;
