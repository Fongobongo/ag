-- Stage 3.2: agent sessions. One row per agent execution inside an attempt,
-- used by the conformance suite and reporting. Linked to attempts(id) so a
-- session is always attributable to the attempt that spawned it.
CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL,
    adapter TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    error_code TEXT,
    FOREIGN KEY (attempt_id) REFERENCES attempts (id)
);
CREATE INDEX idx_agent_sessions_attempt ON agent_sessions (attempt_id);
