-- agentgrid control-plane schema (Stage 8): shared base_commit + per-step
-- retry policy for distributed workflows.
ALTER TABLE workflow_runs ADD COLUMN base_commit TEXT;
ALTER TABLE workflow_steps ADD COLUMN base_commit TEXT;
ALTER TABLE workflow_steps ADD COLUMN retryable INTEGER;
ALTER TABLE workflow_steps ADD COLUMN max_attempts INTEGER;
ALTER TABLE workflow_steps ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tasks ADD COLUMN base_commit TEXT;
