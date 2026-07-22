-- Stage 13: Agent profile desired-state ledger. Immutable revisions; the
-- active revision is pointed at by `agent_profiles.active_revision`. A
-- profile carries the system prompt + autonomy + resource limits the node
-- should project for this adapter; secrets are never stored here (the node
-- resolves secret references from its env at apply time).
CREATE TABLE agent_profiles (
    id            TEXT NOT NULL,           -- profile id (e.g. adapter name)
    revision      INTEGER NOT NULL,        -- monotonically increasing per id
    system_prompt TEXT NOT NULL DEFAULT '',
    autonomy      TEXT NOT NULL DEFAULT 'l2',
    memory_max    INTEGER,                 -- bytes; NULL = no ceiling
    cpu_quota     INTEGER,                 -- percent of one core
    tasks_max     INTEGER,                 -- max PIDs
    created_at    TEXT NOT NULL,
    created_by    TEXT,
    PRIMARY KEY (id, revision)
);

-- One row per profile id pointing at the active revision (fail-closed: a
-- profile not present here is not yet active even if revisions exist).
CREATE TABLE agent_profiles_active (
    id             TEXT PRIMARY KEY,
    active_revision INTEGER NOT NULL,
    FOREIGN KEY (id, active_revision) REFERENCES agent_profiles (id, revision)
);
