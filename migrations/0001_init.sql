-- Orbital initial schema.
--
-- Text, not citext: every identifier is normalised to lowercase in one place
-- (`src/text.rs`) before it reaches SQL, and citext's OID needs sqlx support
-- that is easy to get wrong silently. A CHECK constraint enforces the rules.
--
-- Counters are BIGINT because these columns are decoded into i64 by the
-- application: an `integer` column read into i64 panics the request worker on
-- the response path, and migrations are immutable once applied.

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    handle           text NOT NULL UNIQUE,
    email            text,
    display_name     text NOT NULL,
    bio              text NOT NULL DEFAULT '',
    accent           text NOT NULL DEFAULT '#7c5cff',
    avatar_url       text,
    bsky_did         text,
    bsky_handle      text,
    bsky_verified_at timestamptz,
    verify_token     text NOT NULL DEFAULT encode(gen_random_bytes(12), 'hex'),
    trust_level      smallint NOT NULL DEFAULT 0 CHECK (trust_level BETWEEN 0 AND 3),
    nsfw_enabled     boolean NOT NULL DEFAULT false,
    is_owner         boolean NOT NULL DEFAULT false,
    hidden           boolean NOT NULL DEFAULT false,
    password_hash    text NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    last_active_at   timestamptz NOT NULL DEFAULT now(),
    last_login_at    timestamptz,
    CONSTRAINT users_handle_shape CHECK (handle ~ '^[a-z0-9][a-z0-9.-]{1,29}$')
);

CREATE INDEX users_display_trgm ON users USING gin (display_name gin_trgm_ops);
CREATE INDEX users_handle_trgm ON users USING gin (handle gin_trgm_ops);
CREATE INDEX users_created ON users (created_at DESC);

CREATE TABLE verticals (
    slug        text PRIMARY KEY,
    label       text NOT NULL,
    description text NOT NULL DEFAULT '',
    icon        text NOT NULL DEFAULT 'dot',
    nsfw        boolean NOT NULL DEFAULT false,
    sort        int NOT NULL DEFAULT 100,
    min_members int NOT NULL DEFAULT 5,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT verticals_slug_shape CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,29}$')
);

CREATE TABLE user_verticals (
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    vertical_slug text NOT NULL REFERENCES verticals (slug) ON DELETE CASCADE,
    role          text NOT NULL CHECK (role IN ('primary', 'secondary')),
    joined_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, vertical_slug)
);

-- Exactly one primary per member.
CREATE UNIQUE INDEX user_verticals_one_primary
    ON user_verticals (user_id) WHERE role = 'primary';
CREATE INDEX user_verticals_slug ON user_verticals (vertical_slug);

-- At most two secondaries per member. Enforced in the database because the
-- rule is a ranking-weight rule (0.7x per secondary) and a service-layer-only
-- check is one import away from being bypassed.
CREATE OR REPLACE FUNCTION orbital_limit_secondaries() RETURNS trigger AS $$
BEGIN
    IF NEW.role = 'secondary' AND (
        SELECT count(*) FROM user_verticals
        WHERE user_id = NEW.user_id AND role = 'secondary'
          AND vertical_slug <> NEW.vertical_slug
    ) >= 2 THEN
        RAISE EXCEPTION 'at most two secondary verticals per member';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER user_verticals_secondary_limit
    BEFORE INSERT OR UPDATE ON user_verticals
    FOR EACH ROW EXECUTE FUNCTION orbital_limit_secondaries();

CREATE TABLE tags (
    user_id  uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    tag_slug text NOT NULL,
    PRIMARY KEY (user_id, tag_slug),
    CONSTRAINT tags_shape CHECK (tag_slug ~ '^[a-z0-9][a-z0-9+.#-]{0,29}$')
);
CREATE INDEX tags_slug ON tags (tag_slug);
CREATE INDEX tags_slug_trgm ON tags USING gin (tag_slug gin_trgm_ops);

CREATE TABLE links (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    vertical_slug text REFERENCES verticals (slug) ON DELETE SET NULL,
    url           text NOT NULL,
    label         text NOT NULL,
    icon          text NOT NULL DEFAULT 'link',
    position      int NOT NULL DEFAULT 0,
    click_count   bigint NOT NULL DEFAULT 0,
    hidden        boolean NOT NULL DEFAULT false,
    created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX links_user_pos ON links (user_id, position, created_at);

CREATE TABLE sessions (
    id         text PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);
CREATE INDEX sessions_expiry ON sessions (expires_at);

CREATE TABLE source_hits (
    id               bigserial PRIMARY KEY,
    user_id          uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    link_id          uuid REFERENCES links (id) ON DELETE SET NULL,
    surface          text NOT NULL DEFAULT 'profile',
    vertical_context text REFERENCES verticals (slug) ON DELETE SET NULL,
    position         int,
    session_hash     text NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX source_hits_user_time ON source_hits (user_id, created_at DESC);
CREATE INDEX source_hits_user_session ON source_hits (user_id, session_hash, created_at DESC);
CREATE INDEX source_hits_link ON source_hits (link_id, created_at DESC);

CREATE TABLE profile_views (
    id               bigserial PRIMARY KEY,
    user_id          uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    session_hash     text NOT NULL,
    vertical_context text REFERENCES verticals (slug) ON DELETE SET NULL,
    view_date        date NOT NULL DEFAULT CURRENT_DATE,
    created_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, session_hash, view_date)
);
CREATE INDEX profile_views_user_date ON profile_views (user_id, view_date);

CREATE TABLE flags (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    flagger_id     uuid REFERENCES users (id) ON DELETE SET NULL,
    target_user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    vertical_slug  text REFERENCES verticals (slug) ON DELETE SET NULL,
    reason         text NOT NULL,
    detail         text NOT NULL DEFAULT '',
    status         text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved', 'dismissed')),
    created_at     timestamptz NOT NULL DEFAULT now(),
    resolved_at    timestamptz,
    resolver_id    uuid REFERENCES users (id) ON DELETE SET NULL,
    note           text NOT NULL DEFAULT ''
);
CREATE INDEX flags_status ON flags (status, created_at DESC);

CREATE TABLE curation (
    vertical_slug text NOT NULL REFERENCES verticals (slug) ON DELETE CASCADE,
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    position      int NOT NULL DEFAULT 0,
    PRIMARY KEY (vertical_slug, user_id)
);

CREATE TABLE bot_posts (
    id         bigserial PRIMARY KEY,
    kind       text NOT NULL,
    subject    text NOT NULL,
    uri        text,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX bot_posts_subject ON bot_posts (kind, subject, created_at DESC);

INSERT INTO verticals (slug, label, description, icon, nsfw, sort) VALUES
  ('gaming',     'Video Games',   'Streamers, devs, reviewers and esports.',             'game',  false, 10),
  ('podcasters', 'Podcasters',    'Audio-first creators and the shows they make.',       'mic',   false, 20),
  ('youtube',    'YouTubers',     'Video-first creators, essayists and documentarians.', 'video', false, 30),
  ('music',      'Musicians',     'Artists, producers and DJs.',                         'music', false, 40),
  ('art',        'Visual Artists','Illustrators, photographers and 3D artists.',         'brush', false, 50),
  ('dev',        'Developers',    'Open source, indie dev and tools worth using.',       'code',  false, 60),
  ('writers',    'Writers',       'Bloggers, journalists and fiction authors.',          'pen',   false, 70),
  ('vtubers',    'VTubers',       'Virtual creators, often gaming and video too.',       'mask',  false, 80),
  ('indie-web',  'Indie Web',     'Personal sites, small web and digital gardens.',      'globe', false, 90),
  ('nsfw',       'Adult Creators','Age-gated vertical with its own directory and ring.',  'flame', true,  900);
