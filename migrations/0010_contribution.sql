-- Append-only audit trail for contribution, curation and moderation decisions (SPEC §9, D60).
--
-- entity_edits remains the authority for entity field diffs; this table records the DECISION and
-- points at those diffs, so a moderator can filter actions without decoding every diff. Rows are
-- never updated or deleted through the API: a correction is another row that names the one it fixes.
-- No ranking/prior data belongs here (D27): `reason` is a human moderation/curation reason.

CREATE TABLE contribution_log (
    id           bigserial PRIMARY KEY,
    created_at   timestamptz NOT NULL DEFAULT now(),
    action       text NOT NULL,
    decision     text NOT NULL CHECK (decision IN ('applied','refused','pending','reverted')),
    actor_id     uuid REFERENCES users (id) ON DELETE SET NULL,
    actor_handle text,
    visitor_hash text,
    subject_type text,
    subject_id   text,
    before_state jsonb,
    after_state  jsonb,
    reason       text NOT NULL DEFAULT '',
    gate_mode    text NOT NULL DEFAULT 'open',
    gate_value   int,
    client_hash  text,
    request_id   text,
    CONSTRAINT contribution_log_action_shape CHECK (action ~ '^[a-z]+\.[a-z]+$')
);

CREATE INDEX contribution_log_created ON contribution_log (created_at DESC);
CREATE INDEX contribution_log_action ON contribution_log (action, created_at DESC);
CREATE INDEX contribution_log_actor ON contribution_log (actor_id, created_at DESC);
CREATE INDEX contribution_log_subject ON contribution_log (subject_type, subject_id, created_at DESC);
CREATE INDEX contribution_log_decision ON contribution_log (decision, created_at DESC);

COMMENT ON TABLE contribution_log IS
    'Append-only decisions for contribution/curation/moderation. Never write gravity or ranking explanations here (D27).';

-- `grant` lets one account through a closed door without lowering the level for everybody. The
-- owner and TL3 are implicitly granted in code, so a fresh instance cannot deadlock its own queue.
CREATE TABLE contribution_grants (
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    action      text NOT NULL,
    granted_by  uuid REFERENCES users (id) ON DELETE SET NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, action)
);
