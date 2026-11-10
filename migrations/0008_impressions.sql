-- The impression count behind the ceiling (D45). One row per (surface, member, hour): a page view adds one
-- to each member it showed, so the table stays bounded by surfaces × members × 24 rows and never carries a
-- visitor identity. Nothing here says who looked, only how often a card was put in front of anybody.
CREATE TABLE IF NOT EXISTS impressions (
    surface text NOT NULL,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bucket timestamptz NOT NULL,
    count bigint NOT NULL DEFAULT 0,
    PRIMARY KEY (surface, user_id, bucket)
);

CREATE INDEX IF NOT EXISTS impressions_window ON impressions (surface, bucket);
