-- Stage 2.5: repository registration and per-node clone tracking.
CREATE TABLE IF NOT EXISTS repositories (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL UNIQUE,
    git_url            TEXT NOT NULL,
    default_branch     TEXT NOT NULL,
    validation_command TEXT,
    created_at         TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS node_repositories (
    node_id         TEXT NOT NULL,
    repository_id   TEXT NOT NULL,
    local_path      TEXT,
    status          TEXT NOT NULL,
    last_synced_at  TEXT,
    PRIMARY KEY (node_id, repository_id)
);
