-- Attributes and attribute values tables.
--
-- `config`, `plan`, and `actual` hold JSON as TEXT. Precision note before moving
-- to JSONB: JSONB normalizes JSON numbers (via Postgres `numeric`), so f64 values
-- round-trip to a slightly different bit pattern, whereas TEXT preserves them
-- exactly (with serde_json's `float_roundtrip` on the parse side). Whether that
-- matters depends on the precision needed — but it affects exact round-trips and
-- model/oracle shadowing, so weigh it rather than assuming JSONB is a free upgrade.

CREATE TABLE IF NOT EXISTS attributes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES actors(id),
    name TEXT NOT NULL,
    data_type TEXT NOT NULL,
    config TEXT NOT NULL  -- JSON as TEXT (see header note before moving to JSONB)
);

CREATE TABLE IF NOT EXISTS attribute_values (
    entry_id UUID NOT NULL REFERENCES entries(id),
    attribute_id UUID NOT NULL REFERENCES attributes(id),
    plan TEXT,
    actual TEXT,
    index_float DOUBLE PRECISION,
    index_string TEXT,
    PRIMARY KEY (entry_id, attribute_id)
);
