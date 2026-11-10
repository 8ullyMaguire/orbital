-- How concentrated each surface's attention is, week by week.
--
-- The fame headwind (D34) is judged by this number and by nothing else, so the measurement exists
-- before any damping: a ranking change that raises concentration needs a recorded decision.
CREATE TABLE concentration_snapshots (
    surface     text NOT NULL,
    week        date NOT NULL,
    gini        double precision NOT NULL,
    top10       double precision NOT NULL,
    members     int NOT NULL,
    impressions bigint NOT NULL,
    taken_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (surface, week)
);

COMMENT ON TABLE concentration_snapshots IS
    'One row per surface per ISO week: Gini and top-10 share of impressions. Operator-only (orbital doctor); never served.';
