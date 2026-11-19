-- D62: URL-shaped searches that found nobody become an aggregate contribution queue.
-- No visitor identity and no raw term: only the site's own canonical key.
CREATE TABLE IF NOT EXISTS wanted_links (
    canonical_key text PRIMARY KEY,
    searches      bigint NOT NULL DEFAULT 1 CHECK (searches > 0),
    first_seen    timestamptz NOT NULL DEFAULT now(),
    last_seen     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS wanted_links_searches ON wanted_links (searches DESC, last_seen DESC);
