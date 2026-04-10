-- Attributes and attribute values tables.

CREATE TABLE IF NOT EXISTS attributes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES actors(id),
    name TEXT NOT NULL,
    data_type TEXT NOT NULL,
    config TEXT NOT NULL  -- JSON as TEXT (upgrade to JSONB later)
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
