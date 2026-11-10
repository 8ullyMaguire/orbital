-- The reader's gravity dial: one number per visitor, NULL meaning "no choice of my own".
--
-- The value is the share of a surface's declared ceiling that this reader wants applied. It is
-- clamped on read (polaris-gravity), so an operator lowering a ceiling is retroactive: a row that
-- said 0.40 when the ceiling was 0.40 is read as 0.10 the moment the ceiling becomes 0.10, and the
-- page never claims the reader chose 0.10.
ALTER TABLE visitor_profiles ADD COLUMN gravity real;

COMMENT ON COLUMN visitor_profiles.gravity IS
    'The reader''s dial, 0.0..=1.0, as a share of each surface''s declared ceiling. NULL = no choice; clamped on read.';
