-- SQLite Schema
-- Enable foreign key constraints (SQLite has them disabled by default)
PRAGMA foreign_keys = ON;

-- ACTORS TABLE
CREATE TABLE IF NOT EXISTS actors (
    id BLOB PRIMARY KEY,  -- UUIDs stored as BLOB in SQLite
    actor_kind TEXT NOT NULL CHECK (actor_kind IN ('user', 'system')),
    created_at TEXT DEFAULT (datetime('now'))  -- ISO 8601 format
);

-- USERS TABLE
CREATE TABLE IF NOT EXISTS users (
    actor_id BLOB PRIMARY KEY REFERENCES actors(id),
    email TEXT UNIQUE NOT NULL,
    username TEXT NOT NULL
);

-- ACTIVITIES TABLE
CREATE TABLE IF NOT EXISTS activities (
    id BLOB PRIMARY KEY,
    owner_id BLOB NOT NULL REFERENCES actors(id),
    source_activity_id BLOB REFERENCES activities(id),
    name TEXT NOT NULL,
    description TEXT
);

-- ENTRIES TABLE
CREATE TABLE IF NOT EXISTS entries (
    id BLOB PRIMARY KEY,
    activity_id BLOB REFERENCES activities(id),
    owner_id BLOB NOT NULL REFERENCES actors(id),
    parent_id BLOB REFERENCES entries(id),
    frac_index TEXT,
    is_template INTEGER,  -- SQLite uses INTEGER for BOOLEAN (0 = false, 1 = true)
    display_as_sets INTEGER,
    is_sequence INTEGER,
    start_time TEXT,
    end_time TEXT,
    duration_ms INTEGER,
    CONSTRAINT entry_parent_frac_index_together
        CHECK ((parent_id IS NULL) = (frac_index IS NULL))
);

-- Insert system actor (using OR IGNORE to make this idempotent)
-- UUID eee9e6ae-6531-4580-8356-427604a0dc02 as BLOB
INSERT OR IGNORE INTO actors (id, actor_kind)
VALUES (X'eee9e6ae653145808356427604a0dc02', 'system');
