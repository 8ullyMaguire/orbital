-- Link health and referrer attribution.
--
-- A directory of outbound links rots silently: nothing recorded whether a URL still resolves, so a
-- dead link looked exactly like a live one until a visitor clicked it. `last_checked` is also what
-- makes the checker auditable - a link that has never been checked is distinguishable from one that
-- was checked and failed.
ALTER TABLE links
    ADD COLUMN IF NOT EXISTS last_status  int,
    ADD COLUMN IF NOT EXISTS last_checked timestamptz,
    -- fail_count, not `dead`: a site can be down for an hour. The badge appears at 3 consecutive
    -- failures and clears itself on the next success, so nothing needs a moderator to undo it.
    ADD COLUMN IF NOT EXISTS fail_count   int NOT NULL DEFAULT 0;

-- Where a click came from. The surface (profile/vertical/embed/search) says which page of OURS
-- earned the click; the referrer says which network sent the visitor, which is the question the
-- dashboard could not answer at all.
ALTER TABLE source_hits
    ADD COLUMN IF NOT EXISTS referrer_host text;

CREATE INDEX IF NOT EXISTS source_hits_referrer
    ON source_hits (user_id, referrer_host, created_at DESC);

-- The checker walks live links oldest-checked-first; without this every run re-HEADs the whole
-- directory in insert order.
CREATE INDEX IF NOT EXISTS links_check_order
    ON links (last_checked NULLS FIRST, position);
