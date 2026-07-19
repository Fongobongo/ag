-- Stage 9.2: link a durable approval to the workflow step that is waiting on it,
-- so an unanswered (timed-out) approval can block that step instead of leaving
-- the run hanging.
ALTER TABLE approvals ADD COLUMN step_run_id TEXT;
CREATE INDEX IF NOT EXISTS idx_approvals_step ON approvals(step_run_id);
