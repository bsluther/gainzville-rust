-- Attributes and attribute values tables.

CREATE TABLE IF NOT EXISTS attributes (
    id BLOB PRIMARY KEY,
    owner_id BLOB NOT NULL REFERENCES actors(id),
    name TEXT NOT NULL,
    data_type TEXT NOT NULL,
    config TEXT NOT NULL  -- JSON as TEXT
);

CREATE TABLE IF NOT EXISTS attribute_values (
    entry_id BLOB NOT NULL REFERENCES entries(id),
    attribute_id BLOB NOT NULL REFERENCES attributes(id),
    plan TEXT,            -- JSON as TEXT
    actual TEXT,          -- JSON as TEXT
    index_float REAL,
    index_string TEXT,
    PRIMARY KEY (entry_id, attribute_id)
);
