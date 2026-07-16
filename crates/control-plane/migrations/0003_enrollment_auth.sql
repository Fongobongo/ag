-- Stage 2.3: node enrollment, credential auth, heartbeat telemetry, audit log.
-- nodes.credential_hash and os/arch/agent_version/revoked_at already exist (0001).

CREATE TABLE IF NOT EXISTS enrollment_tokens (
    id          TEXT PRIMARY KEY,
    token_hash  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_enroll_token_hash ON enrollment_tokens (token_hash);

CREATE TABLE IF NOT EXISTS audit_events (
    id          TEXT PRIMARY KEY,
    actor_type  TEXT NOT NULL,
    actor_id    TEXT,
    action      TEXT NOT NULL,
    subject     TEXT,
    payload     TEXT,
    created_at  TEXT NOT NULL
);

ALTER TABLE nodes ADD COLUMN load_avg REAL NOT NULL DEFAULT 0;
ALTER TABLE nodes ADD COLUMN free_disk_mb INTEGER NOT NULL DEFAULT 0;
