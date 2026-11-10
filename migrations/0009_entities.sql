-- The entity substrate (SPEC §5b, D52). Until now a "thing" on this site was a member with links; from here a
-- thing is its own row with its own key, its own category, its own reviews, and a page that exists whether or not
-- anybody has registered to talk about it.
--
-- Renumbered: the ledger calls this "migration 0006" from when the roadmap was written, but 0006-0008 were spent on
-- the dial, the concentration snapshots and the impressions. Files are append-only; the ledger note says so.
--
-- Text, not citext, for the same reason as 0001: `citext`'s OID needs sqlx support, and every identifier is
-- lowercased in `canonical_key()` before it reaches a column. Uppercase cannot get in, so case-insensitivity does
-- not need an extension.

-- ---------------------------------------------------------------------------------------------------------------
-- canonical_key: the join key, normalised at write time (SPEC §5b). One implementation, in the database, so a
-- caller cannot forget it and two callers cannot disagree about what "the same page" means.
-- ---------------------------------------------------------------------------------------------------------------
CREATE FUNCTION canonical_key(raw text) RETURNS text
LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE
    work text := btrim(coalesce(raw, ''));
    scheme text;
BEGIN
    IF work = '' THEN
        RETURN '';
    END IF;

    -- An identifier key (`isbn:`, `imdb:`, `tmdb:`, `musicbrainz:`, `atproto:`) is not a URL: only the scheme word
    -- is lowercased, because the value's own canonical form belongs to the identifier's standard, not to this site.
    IF work !~* '^[a-z][a-z0-9+.-]*://' AND work ~ '^[A-Za-z][A-Za-z0-9]*:(?![0-9])' THEN
        scheme := lower(split_part(work, ':', 1));
        RETURN scheme || ':' || substr(work, length(split_part(work, ':', 1)) + 2);
    END IF;

    work := lower(work);
    -- The four rewrites, in a fixed order, each one observable in `tests/entities.rs`:
    --   1. uppercase host            → `lower()` above (path casing is left alone by the second pass below)
    --   2. the fragment              → dropped, because `#section` is not part of the address's identity
    --   3. a trailing slash          → dropped, because `/thing/` and `/thing` serve the same page
    --   4. a `www.` host             → dropped, because it is a redirect, not a second site
    work := split_part(work, '#', 1);
    work := regexp_replace(work, '/+$', '');
    work := regexp_replace(work, '^(https?://)www\.', '\1');

    IF work ~* '^https?://' THEN
        RETURN 'url:' || work;
    END IF;
    -- Anything else that is not an identifier is still keyed, so a malformed import row collides with its own
    -- retries rather than creating a new thing every night.
    RETURN 'raw:' || work;
END;
$$;

COMMENT ON FUNCTION canonical_key(text) IS
    'URL or identifier → the key two rows must share to be the same thing (SPEC §5b). Lowercased, no www., no trailing slash, no fragment.';

-- ---------------------------------------------------------------------------------------------------------------
-- The catalogue.
-- ---------------------------------------------------------------------------------------------------------------
CREATE TABLE categories (
    slug        text PRIMARY KEY CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,39}$'),
    label       text NOT NULL,
    description text NOT NULL DEFAULT '',
    sort        int  NOT NULL DEFAULT 0,
    nsfw        boolean NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE entities (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug          text NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,59}$'),
    kind          text NOT NULL CHECK (kind IN ('person', 'site', 'work', 'place', 'product', 'other')),
    -- The join. Unique *and* canonical, enforced by the trigger below rather than trusted from the caller.
    canonical_key text NOT NULL,
    title         text NOT NULL DEFAULT '',
    description   text NOT NULL DEFAULT '',
    image         text,
    category_slug text REFERENCES categories (slug) ON DELETE SET NULL,
    nsfw          boolean NOT NULL DEFAULT false,
    -- Where this row came from, and whether it was imported. `origin` is not decoration: a claim has to know
    -- whether it is claiming something a person made or something a machine guessed.
    origin        text NOT NULL DEFAULT 'contribution'
                  CHECK (origin IN ('contribution', 'import', 'federation', 'seed')),
    source        text,
    source_id     text,
    imported_at   timestamptz,
    created_by    uuid REFERENCES users (id) ON DELETE SET NULL,
    first_seen    timestamptz NOT NULL DEFAULT now(),
    last_checked  timestamptz,
    last_status   int,
    fail_count    int NOT NULL DEFAULT 0,
    hidden        boolean NOT NULL DEFAULT false,
    -- A merge is non-destructive: the row stays, redirects, and its reviews move in the same transaction.
    merged_into   uuid REFERENCES entities (id) ON DELETE SET NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT entities_not_its_own_survivor CHECK (merged_into IS NULL OR merged_into <> id)
);
CREATE UNIQUE INDEX entities_canonical_key_key ON entities (canonical_key);
CREATE INDEX entities_category ON entities (category_slug, nsfw, hidden);
CREATE INDEX entities_merged ON entities (merged_into) WHERE merged_into IS NOT NULL;
CREATE INDEX entities_title_trgm ON entities USING gin (title gin_trgm_ops);

-- Normalise on the way in, always, including on a re-key.
CREATE FUNCTION entities_canonicalise() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.canonical_key := canonical_key(NEW.canonical_key);
    IF NEW.canonical_key = '' THEN
        RAISE EXCEPTION 'an entity without a canonical key has no identity';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER entities_canonicalise_trg
    BEFORE INSERT OR UPDATE OF canonical_key, title, description, category_slug, hidden, merged_into
    ON entities
    FOR EACH ROW EXECUTE FUNCTION entities_canonicalise();

CREATE TABLE entity_tags (
    entity_id uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    tag_slug  text NOT NULL CHECK (tag_slug ~ '^[a-z0-9][a-z0-9 .-]{0,39}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (entity_id, tag_slug)
);

CREATE TABLE entity_edges (
    a_entity    uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    b_entity    uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    relation    text NOT NULL CHECK (relation IN ('same_as', 'related', 'runs', 'mentions')),
    score       real NOT NULL DEFAULT 0,
    computed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (a_entity, b_entity, relation),
    CONSTRAINT entity_edges_not_self CHECK (a_entity <> b_entity)
);
CREATE INDEX entity_edges_back ON entity_edges (b_entity, relation);

CREATE TABLE entity_people (
    entity_id  uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    user_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    role       text NOT NULL DEFAULT 'author' CHECK (role IN ('author', 'maintainer', 'subject')),
    claimed_at timestamptz,
    PRIMARY KEY (entity_id, user_id)
);

-- ---------------------------------------------------------------------------------------------------------------
-- Reviews, votes, edits, scores, ballots. Contribution is open (D28), so `user_id` is nullable and a review by a
-- visitor with no account is a review, not a lesser row.
-- ---------------------------------------------------------------------------------------------------------------
CREATE TABLE reviews (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_id   uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    user_id     uuid REFERENCES users (id) ON DELETE SET NULL,
    -- The visitor who wrote it, hashed (the same instrument `source_hits` uses). A review needs an author; an
    -- account was never the requirement.
    visitor_hash text,
    rating      smallint CHECK (rating BETWEEN 1 AND 5),
    body        text NOT NULL DEFAULT '',
    language    text NOT NULL DEFAULT 'en',
    nsfw        boolean NOT NULL DEFAULT false,
    -- The federated activity id, so redelivery is idempotent and a remote edit finds its row.
    ap_id       text,
    visibility  text NOT NULL DEFAULT 'public' CHECK (visibility IN ('public', 'unlisted', 'private')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    deleted_at  timestamptz,
    CONSTRAINT reviews_have_an_author CHECK (user_id IS NOT NULL OR visitor_hash IS NOT NULL)
);
CREATE UNIQUE INDEX reviews_ap_id ON reviews (ap_id) WHERE ap_id IS NOT NULL;
CREATE INDEX reviews_entity ON reviews (entity_id, created_at DESC) WHERE deleted_at IS NULL;

CREATE TABLE review_votes (
    review_id  uuid NOT NULL REFERENCES reviews (id) ON DELETE CASCADE,
    user_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    visitor_hash text,
    helpful    boolean NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (review_id, user_id)
);

CREATE TABLE entity_edits (
    id         bigserial PRIMARY KEY,
    entity_id  uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    user_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    visitor_hash text,
    diff       jsonb NOT NULL,
    note       text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    reverted_at timestamptz
);
CREATE INDEX entity_edits_entity ON entity_edits (entity_id, created_at DESC);

CREATE TABLE entity_scores (
    entity_id    uuid PRIMARY KEY REFERENCES entities (id) ON DELETE CASCADE,
    rating_avg   real NOT NULL DEFAULT 0,
    rating_count int  NOT NULL DEFAULT 0,
    ballot_count int  NOT NULL DEFAULT 0,
    computed_at  timestamptz NOT NULL DEFAULT now()
);

-- A pair is shown once per person, so a ballot is a vote on a comparison rather than a way to spam one.
CREATE TABLE ballots (
    id         bigserial PRIMARY KEY,
    user_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    visitor_hash text,
    a_entity   uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    b_entity   uuid NOT NULL REFERENCES entities (id) ON DELETE CASCADE,
    winner     char(1) NOT NULL CHECK (winner IN ('a', 'b', 't')),
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT ballots_not_self CHECK (a_entity <> b_entity)
);
CREATE UNIQUE INDEX ballots_once_per_person
    ON ballots (least(a_entity, b_entity), greatest(a_entity, b_entity), coalesce(user_id::text, visitor_hash));

-- ---------------------------------------------------------------------------------------------------------------
-- Imports are runs, not migrations: an import that half-failed says so in a row rather than in a log nobody reads.
-- ---------------------------------------------------------------------------------------------------------------
CREATE TABLE import_runs (
    id            bigserial PRIMARY KEY,
    source        text NOT NULL,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz,
    rows_seen     int NOT NULL DEFAULT 0,
    rows_inserted int NOT NULL DEFAULT 0,
    rows_updated  int NOT NULL DEFAULT 0,
    rows_failed   int NOT NULL DEFAULT 0,
    note          text NOT NULL DEFAULT ''
);

-- ---------------------------------------------------------------------------------------------------------------
-- The three things this step changes about what already exists.
-- ---------------------------------------------------------------------------------------------------------------
-- A link can now point at a thing as well as at a member's page: that is how a member's profile says "I made this".
ALTER TABLE links ADD COLUMN entity_id uuid REFERENCES entities (id) ON DELETE SET NULL;
CREATE INDEX links_entity ON links (entity_id) WHERE entity_id IS NOT NULL;

-- A shell row: an actor that exists because a claim or a federation delivery named it, with no way to log in yet.
-- A NULL password is checked at the login path, not assumed absent here.
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
ALTER TABLE users ADD COLUMN claimed_at timestamptz;
