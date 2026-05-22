
CREATE TABLE IF NOT EXISTS actors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_kind VARCHAR(50) NOT NULL CHECK (actor_kind IN ('user', 'system')),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS users (
    actor_id UUID PRIMARY KEY REFERENCES actors(id),
    email TEXT UNIQUE NOT NULL,
    username TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id UUID NOT NULL REFERENCES actors(id),
    source_activity_id UUID REFERENCES activities(id), -- If this is a reference to another Activity.
    name TEXT NOT NULL,
    description TEXT
);

CREATE TABLE IF NOT EXISTS entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    activity_id UUID REFERENCES activities(id),
    owner_id UUID NOT NULL REFERENCES actors(id),
    parent_id UUID REFERENCES entries(id) DEFERRABLE INITIALLY IMMEDIATE,
    frac_index TEXT,
    is_template BOOLEAN,
    display_as_sets BOOLEAN,
    is_sequence BOOLEAN,
    start_time TIMESTAMPTZ,
    end_time TIMESTAMPTZ,
    duration_ms BIGINT

    CONSTRAINT entry_parent_frac_index_together
        CHECK ((parent_id IS NULL) = (frac_index IS NULL))
);

-- Insert system actor.
INSERT INTO actors (id, actor_kind) VALUES ('eee9e6ae-6531-4580-8356-427604a0dc02', 'system');