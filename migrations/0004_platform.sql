-- Platform work: tokens for writing without a browser session, link metadata, rank history,
-- a fingerprint for telling one visitor from a farm, and a reviewed alias table.

-- A link is more than a URL. The title is what the page calls itself, and the favicon is what makes
-- a list of links scannable; both are fetched once and cached here.
ALTER TABLE links ADD COLUMN title text;
ALTER TABLE links ADD COLUMN favicon text;
ALTER TABLE links ADD COLUMN metadata_at timestamptz;

-- Long-lived credentials for the API. Only the hash is stored: a token is shown once, at creation,
-- and a database dump cannot be replayed afterwards.
CREATE TABLE api_tokens (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash   text NOT NULL UNIQUE,
    label        text NOT NULL DEFAULT '',
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);
CREATE INDEX api_tokens_user ON api_tokens (user_id) WHERE revoked_at IS NULL;

-- Rank over time, so "you moved 12 → 9" becomes a sentence the dashboard can say.
CREATE TABLE rank_snapshots (
    id            bigserial PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    vertical_slug text NOT NULL,
    rank          int NOT NULL,
    taken_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX rank_snapshots_user ON rank_snapshots (user_id, taken_at DESC);

-- A hash of the things a script cannot cheaply vary per request (user agent, accept-language).
-- Not an identity: it groups sessions that look like one client, which is what a farm is.
ALTER TABLE source_hits ADD COLUMN fingerprint text;
CREATE INDEX source_hits_fingerprint ON source_hits (fingerprint)
    WHERE fingerprint IS NOT NULL AND surface <> 'voided';

-- Aliases are a reviewed decision, not a fuzzy match: an admin adds one, the nightly retag applies
-- it to existing rows, and normalise_tag keeps honouring it from then on.
CREATE TABLE tag_aliases (
    from_tag   text PRIMARY KEY,
    to_tag     text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
