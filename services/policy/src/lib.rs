//! Small declarative policy evaluator.
//!
//! Rules are YAML, first match wins, and an unmatched request is denied.
//! This is not OPA or Cedar. The decision shape (`allow`, `deny`, `approve`)
//! is what a later engine would have to preserve. See `docs/PLAN.md`.

#![forbid(unsafe_code)]

use dj_label::{Confidentiality, Integrity, Labels};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

pub const DEFAULT_POLICY: &str = include_str!("../../../policy/default.yaml");

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("policy io: {0}")]
    Io(#[from] std::io::Error),
    #[error("policy yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("policy: {0}")]
    Invalid(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionClass {
    Outbound,
    Destructive,
    Read,
    Model,
    Unknown,
}

impl ActionClass {
    pub fn of(action: &str) -> Self {
        match action {
            "email.send" | "net.http" | "net.connect" => Self::Outbound,
            "files.write" | "files.delete" | "shell.exec" => Self::Destructive,
            "files.read" => Self::Read,
            "model.use" => Self::Model,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteKind {
    Local,
    Cloud,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionKind {
    Allow,
    #[default]
    Deny,
    Approve,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub kind: DecisionKind,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct PolicyRequest {
    pub action: String,
    pub action_class: ActionClass,
    pub labels: Labels,
    pub route: Option<RouteKind>,
}

#[derive(Clone, Debug)]
pub struct Policy {
    default_decision: DecisionKind,
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyFile {
    version: u32,
    #[serde(default)]
    default_decision: DecisionKind,
    rules: Vec<Rule>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "match")]
    when: Match,
    decision: DecisionKind,
    reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Match {
    #[serde(default)]
    integrity: Option<Integrity>,
    #[serde(default)]
    confidentiality_at_least: Option<Confidentiality>,
    #[serde(default)]
    action_class_any: Vec<ActionClass>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    route: Option<RouteKind>,
}

impl Policy {
    pub fn parse(text: &str) -> Result<Self, PolicyError> {
        let file: PolicyFile = serde_yaml::from_str(text)?;
        if file.version != 1 {
            return Err(PolicyError::Invalid(format!(
                "unsupported policy version {}",
                file.version
            )));
        }
        if file.rules.is_empty() {
            return Err(PolicyError::Invalid("policy has no rules".into()));
        }
        for rule in &file.rules {
            if rule.id.is_empty() || rule.reason.is_empty() {
                return Err(PolicyError::Invalid(
                    "every rule needs an id and a reason".into(),
                ));
            }
            if rule.when.is_empty() {
                return Err(PolicyError::Invalid(format!(
                    "rule {} matches everything; refuse it",
                    rule.id
                )));
            }
            let _ = rule.description.len();
        }
        Ok(Self {
            default_decision: file.default_decision,
            rules: file.rules,
        })
    }

    pub fn from_path(path: &Path) -> Result<Self, PolicyError> {
        let text = fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn bundled() -> Self {
        Self::parse(DEFAULT_POLICY).expect("shipped policy/default.yaml is valid")
    }

    pub fn decide(&self, request: &PolicyRequest) -> Decision {
        for rule in &self.rules {
            if rule.when.matches(request) {
                return Decision {
                    kind: rule.decision,
                    rule_id: rule.id.clone(),
                    reason: rule.reason.clone(),
                };
            }
        }
        Decision {
            kind: self.default_decision,
            rule_id: "default".into(),
            reason: "no rule matched; default decision".into(),
        }
    }
}

impl Match {
    fn is_empty(&self) -> bool {
        self.integrity.is_none()
            && self.confidentiality_at_least.is_none()
            && self.action_class_any.is_empty()
            && self.action.is_none()
            && self.route.is_none()
    }

    fn matches(&self, request: &PolicyRequest) -> bool {
        if let Some(integrity) = self.integrity {
            if request.labels.integrity != integrity {
                return false;
            }
        }
        if let Some(floor) = self.confidentiality_at_least {
            if !request.labels.confidentiality.at_least(floor) {
                return false;
            }
        }
        if !self.action_class_any.is_empty()
            && !self.action_class_any.contains(&request.action_class)
        {
            return false;
        }
        if let Some(action) = &self.action {
            if action != &request.action {
                return false;
            }
        }
        if let Some(route) = self.route {
            if request.route != Some(route) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dj_label::{Confidentiality, Integrity, Labels};

    fn req(action: &str, labels: Labels, route: Option<RouteKind>) -> PolicyRequest {
        PolicyRequest {
            action: action.into(),
            action_class: ActionClass::of(action),
            labels,
            route,
        }
    }

    #[test]
    fn shipped_policy_blocks_private_cloud_and_tainted_email() {
        let policy = Policy::bundled();
        let private = Labels {
            confidentiality: Confidentiality::Private,
            ..Labels::user("agent-a")
        };
        let denied = policy.decide(&req("model.use", private, Some(RouteKind::Cloud)));
        assert_eq!(denied.kind, DecisionKind::Deny);
        assert_eq!(denied.rule_id, "private-data-stays-local");

        let tainted = Labels {
            integrity: Integrity::Untrusted,
            origin: "web:page".into(),
            ..Labels::user("agent-a")
        };
        let prompted = policy.decide(&req("email.send", tainted.clone(), None));
        assert_eq!(prompted.kind, DecisionKind::Approve);
        assert_eq!(prompted.rule_id, "tainted-outbound-or-destructive");

        let read = policy.decide(&req("files.read", tainted, None));
        assert_eq!(read.kind, DecisionKind::Allow);
    }

    #[test]
    fn unknown_action_is_denied() {
        let policy = Policy::bundled();
        let decision = policy.decide(&req("wallet.pay", Labels::user("a"), None));
        assert_eq!(decision.kind, DecisionKind::Deny);
    }
}
