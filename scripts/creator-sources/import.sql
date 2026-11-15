-- Idempotent load of the stash-box creator pipeline's staging CSV.
--
--   sed "s|__STAGE__|/tmp/stage-creators.csv|" import.sql > /tmp/import.sql && psql "$DATABASE_URL" -f /tmp/import.sql
--
-- Same shape as scripts/x-following/import.sql: staging table, then members,
-- primary vertical, tags and links, each guarded so a re-run adds nothing.
\set ON_ERROR_STOP on
BEGIN;

CREATE TEMP TABLE stg (
  handle       text,
  display_name text,
  bio          text,
  accent       text,
  avatar_url   text,
  vertical     text,
  tags         jsonb,
  links        jsonb
) ON COMMIT DROP;

\copy stg FROM '__STAGE__' WITH (FORMAT csv)

SELECT count(*) AS staged FROM stg;

INSERT INTO users (handle, display_name, bio, accent, avatar_url, nsfw_enabled, trust_level)
SELECT s.handle, COALESCE(s.display_name, s.handle), COALESCE(s.bio, ''),
       COALESCE(s.accent, '#7c5cff'), NULLIF(s.avatar_url, ''),
       (COALESCE(s.vertical, 'nsfw') = 'nsfw'), 0
  FROM stg s
 WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.handle = s.handle);

INSERT INTO user_verticals (user_id, vertical_slug, role)
SELECT u.id, COALESCE(s.vertical, 'nsfw'), 'primary'
  FROM stg s JOIN users u ON u.handle = s.handle
 WHERE NOT EXISTS (SELECT 1 FROM user_verticals uv WHERE uv.user_id = u.id)
ON CONFLICT DO NOTHING;

INSERT INTO tags (user_id, tag_slug)
SELECT u.id, t.tag
  FROM stg s
  JOIN users u ON u.handle = s.handle
  CROSS JOIN LATERAL jsonb_array_elements_text(s.tags) AS t(tag)
 WHERE t.tag ~ '^[a-z0-9][a-z0-9+.#-]{0,29}$'
ON CONFLICT DO NOTHING;

INSERT INTO links (user_id, url, label, icon, position)
SELECT u.id, l->>'url', l->>'label', l->>'icon', (ord - 1)::int
  FROM stg s
  JOIN users u ON u.handle = s.handle
  CROSS JOIN LATERAL jsonb_array_elements(s.links) WITH ORDINALITY AS x(l, ord)
 WHERE NOT EXISTS (
        SELECT 1 FROM links li
         WHERE li.user_id = u.id AND li.url = l->>'url');

SELECT
  (SELECT count(*) FROM users)                                    AS users_total,
  (SELECT count(*) FROM links)                                    AS links_total,
  (SELECT count(*) FROM users u JOIN stg s USING (handle))        AS staged_members,
  (SELECT count(*) FROM links l JOIN stg s
     ON s.handle = (SELECT handle FROM users WHERE id = l.user_id)) AS staged_links;

COMMIT;
