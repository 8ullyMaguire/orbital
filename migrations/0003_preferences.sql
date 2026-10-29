-- Preference profiles: what a viewer wants to discover, as opposed to what a member is.
--
-- Two owners share one table. `user` rows belong to an account (survive devices), `visitor` rows
-- belong to an anonymous browser, keyed by a SHA-256 of the visitor cookie so the raw id is not
-- stored in a table that analytics can read.
CREATE TABLE preference_weights (
    owner_kind text NOT NULL CHECK (owner_kind IN ('user', 'visitor')),
    owner_id   text NOT NULL,
    tag_slug   text NOT NULL,
    weight     real NOT NULL CHECK (weight >= 0 AND weight <= 1),
    source     text NOT NULL CHECK (source IN ('explicit', 'passive')),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_kind, owner_id, tag_slug)
);

-- Audience interests read this the other way round ("who cares about chiptune").
CREATE INDEX preference_weights_tag ON preference_weights (tag_slug, source);

-- The counters that decide whether personalisation is allowed yet, and when the visitor was last
-- here. Deliberately not a user table: there is no account, no email, nothing to delete but a hash.
CREATE TABLE visitor_profiles (
    visitor_hash  text PRIMARY KEY,
    pages_visited int NOT NULL DEFAULT 0,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now()
);

-- Who clicked, in hashed form, so a member can be told what their audience cares about without
-- anyone leaving the aggregate. Nullable: every row written before today has no viewer.
ALTER TABLE source_hits ADD COLUMN viewer_hash text;
CREATE INDEX source_hits_viewer ON source_hits (viewer_hash) WHERE viewer_hash IS NOT NULL;
