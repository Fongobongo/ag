-- Stage 9.2: skill trust ledger. Stores operator trust decisions per skill,
-- keyed by (name, source). A skill absent from this table defaults to
-- `untrusted` (fail-closed): the agent is not allowed to load/execute it until
-- the operator explicitly trusts it. A node that discovers a skill reports it
-- via heartbeat; the control plane answers with the recorded verdict, or
-- untrusted if no decision exists.
--
-- `source` matches SkillSource's display string (project|user|managed) so the
-- same skill name can carry different trust across where it was found.
-- `decided_by` is the operator username (or `system`); `decided_at` is ISO.
CREATE TABLE skills_trust (
    name        TEXT NOT NULL,
    source      TEXT NOT NULL,
    trusted     INTEGER NOT NULL,     -- 0 = untrusted (default), 1 = trusted
    decided_by  TEXT,
    decided_at  TEXT,
    PRIMARY KEY (name, source)
);
