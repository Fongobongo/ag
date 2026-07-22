//! Stage 13: an `AgentProfile` is the desired-state spec the control plane
//! projects to nodes for a given adapter — system prompt, autonomy level, and
//! resource limits. Revisions are immutable; the active revision is flipped by
//! updating an `agent_profiles_active` pointer, so rollback is "point back".
//! Secrets are never stored here (the node resolves secret references from its
//! own env at apply time — Stage 13 sync only carries *requirements*, never
//! values).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub revision: i64,
    pub system_prompt: String,
    /// Autonomy level string (`l0`..`l4`); the node parses it.
    pub autonomy: String,
    /// Optional resource ceilings (Stage 12). `None` = no ceiling.
    pub memory_max: Option<i64>,
    pub cpu_quota: Option<i64>,
    pub tasks_max: Option<i64>,
    pub created_at: String,
    pub created_by: Option<String>,
    /// Whether this revision is the active one for the profile id.
    pub active: bool,
}

/// Body for `POST /v1/profiles/{id}` — create a new revision. Fields the caller
/// omits default to empty/None; the server fills `revision`/`created_at`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentProfileCreate {
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default = "default_autonomy")]
    pub autonomy: String,
    #[serde(default)]
    pub memory_max: Option<i64>,
    #[serde(default)]
    pub cpu_quota: Option<i64>,
    #[serde(default)]
    pub tasks_max: Option<i64>,
}

fn default_autonomy() -> String {
    "l2".into()
}

/// Body for `POST /v1/profiles/{id}/activate` — flip the active pointer to an
/// existing revision (rollback = point at an older revision).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateProfile {
    pub revision: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_defaults_to_l2_when_deserialized() {
        // serde defaults (the L2 autonomy, empty prompt) apply when fields are
        // absent from JSON — the path the CP handler takes.
        let c: AgentProfileCreate = serde_json::from_str("{}").unwrap();
        assert_eq!(c.autonomy, "l2");
        assert!(c.system_prompt.is_empty());
        assert!(c.memory_max.is_none());
    }
}
