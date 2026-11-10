-- The reader's gravity dial: one number per visitor, NULL meaning "no choice of my own".
--
-- The value is the share of a surface's declared ceiling that this reader wants applied. It is
-- clamped on read (polaris-gravity), so an operator lowering a ceiling is retroactive: a row that
-- said 0.40 when the ceiling was 0.40 is read as 0.10 the moment the ceiling becomes 0.10, and the
-- page never claims the reader chose 0.10.
-- `double precision`, not `real`: a float4 round-trips 0.4 as 0.4000000059604645 and sqlx decodes it
-- as f32, so a stored choice would neither compare equal to the number the reader typed nor read back
-- as the f64 the dial speaks. The estate has met this trap before (silent float4 decode).
ALTER TABLE visitor_profiles ADD COLUMN gravity double precision;

COMMENT ON COLUMN visitor_profiles.gravity IS
    'The reader''s dial, 0.0..=1.0, as a share of each surface''s declared ceiling. NULL = no choice; clamped on read.';
