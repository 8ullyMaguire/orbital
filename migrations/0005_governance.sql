-- Two ways the taxonomy stops being mine alone: members propose verticals and vote, and members
-- second each other's tags. Both are reviewed decisions with a quorum, not votes that bind.

CREATE TABLE vertical_proposals (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug          text NOT NULL,
    label         text NOT NULL,
    description   text NOT NULL DEFAULT '',
    proposed_by   uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Where a merged vertical's members go. NULL means "this is a new vertical".
    merged_into   text REFERENCES verticals (slug) ON DELETE SET NULL,
    status        text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'accepted', 'rejected')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    decided_at    timestamptz
);
CREATE UNIQUE INDEX vertical_proposals_open_slug ON vertical_proposals (slug) WHERE status = 'open';

CREATE TABLE vertical_votes (
    proposal_id uuid NOT NULL REFERENCES vertical_proposals (id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    weight      int  NOT NULL DEFAULT 1,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (proposal_id, user_id)
);

-- A tag is a claim; a second is a signal. TL2+ may endorse, and an endorsed tag carries a little
-- more weight in the relevance term - capped, so a popular tag cannot become a personality cult.
CREATE TABLE tag_endorsements (
    tag_slug   text NOT NULL,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tag_slug, user_id)
);
CREATE INDEX tag_endorsements_tag ON tag_endorsements (tag_slug);
