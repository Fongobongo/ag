//! Stage 9.2: operator trust decision for a discovered skill. Absent from the
//! control-plane ledger = `untrusted` (fail-closed): the agent may not
//! load/execute a skill until the operator explicitly trusts it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillTrustView {
    pub name: String,
    /// Where the skill was found: `project` | `user` | `managed`.
    pub source: String,
    pub trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_with_optional_decided_fields() {
        let v = SkillTrustView {
            name: "ponytail".into(),
            source: "user".into(),
            trusted: true,
            decided_by: None,
            decided_at: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        assert!(s.contains("\"trusted\":true"));
        assert!(!s.contains("decided_by"));
        let back: SkillTrustView = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
