//! Permission broker.
//!
//! A grant is necessary and not sufficient. Policy still runs. Approval
//! prompts fail closed in this slice: there is no trusted desktop dialog yet,
//! so an `approve` decision is recorded and then denied.

#![forbid(unsafe_code)]

use dj_audit::{AuditLog, Event};
use dj_label::{Confidentiality, Integrity, Labels};
use dj_policy::{ActionClass, DecisionKind, Policy, PolicyRequest, RouteKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub trust_domain: String,
    pub grants: BTreeSet<String>,
    pub local_only: bool,
}

impl Principal {
    pub fn has_grant(&self, grant: &str) -> bool {
        self.grants.iter().any(|held| held == grant)
    }

    pub fn file_grant<'a>(&'a self, action: &str, target: &Path) -> Option<&'a str> {
        let target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        self.grants.iter().find_map(|grant| {
            let root = grant.strip_prefix(&format!("{action}:"))?;
            if root.is_empty() {
                return None;
            }
            let root_path = Path::new(root.trim_end_matches('/'));
            let root_path = fs::canonicalize(root_path).unwrap_or_else(|_| root_path.to_path_buf());
            if target == root_path || target.starts_with(&root_path) {
                Some(grant.as_str())
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Debug)]
pub struct ActionRequest {
    pub action: String,
    pub target: String,
    pub labels: Labels,
    pub arguments: Value,
    pub route: Option<RouteKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authz {
    pub allowed: bool,
    pub prompted: bool,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error(transparent)]
    Audit(#[from] dj_audit::AuditError),
    #[error("file mediation: {0}")]
    File(String),
}

pub struct Broker {
    policy: Policy,
    audit: AuditLog,
}

impl Broker {
    pub fn new(policy: Policy, audit: AuditLog) -> Self {
        Self { policy, audit }
    }

    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    pub fn authorize(
        &self,
        principal: &Principal,
        request: &ActionRequest,
    ) -> Result<Authz, BrokerError> {
        let class = ActionClass::of(&request.action);
        let granted = match class {
            ActionClass::Read | ActionClass::Destructive => principal
                .file_grant(&request.action, Path::new(&request.target))
                .is_some(),
            ActionClass::Model => principal.has_grant(&format!("model.use:{}", request.target)),
            _ => principal.has_grant(&request.action),
        };
        if !granted {
            let reason = format!(
                "capability not granted for {} {}",
                request.action, request.target
            );
            self.record(principal, request, "denial", &reason, false)?;
            return Ok(Authz {
                allowed: false,
                prompted: false,
                rule_id: "grant".into(),
                reason,
            });
        }

        let decision = self.policy.decide(&PolicyRequest {
            action: request.action.clone(),
            action_class: class,
            labels: request.labels.clone(),
            route: request.route,
        });
        self.record(
            principal,
            request,
            "policy",
            &format!("{}: {}", decision.rule_id, decision.reason),
            false,
        )?;

        match decision.kind {
            DecisionKind::Allow => {
                self.record(principal, request, "allow", &decision.reason, false)?;
                Ok(Authz {
                    allowed: true,
                    prompted: false,
                    rule_id: decision.rule_id,
                    reason: decision.reason,
                })
            }
            DecisionKind::Deny => {
                self.record(principal, request, "denial", &decision.reason, false)?;
                Ok(Authz {
                    allowed: false,
                    prompted: false,
                    rule_id: decision.rule_id,
                    reason: decision.reason,
                })
            }
            DecisionKind::Approve => {
                self.record(
                    principal,
                    request,
                    "approval_prompt",
                    &decision.reason,
                    true,
                )?;
                let reason = format!("{}; no interactive approver; fail closed", decision.reason);
                self.record(principal, request, "denial", &reason, true)?;
                Ok(Authz {
                    allowed: false,
                    prompted: true,
                    rule_id: decision.rule_id,
                    reason,
                })
            }
        }
    }
}

impl Broker {
    fn record(
        &self,
        principal: &Principal,
        request: &ActionRequest,
        kind: &str,
        reason: &str,
        prompted: bool,
    ) -> Result<(), BrokerError> {
        let outcome = match kind {
            "allow" => "allow",
            "approval_prompt" => "prompt",
            "policy" => "recorded",
            _ => "deny",
        };
        self.audit.append(Event {
            kind: kind.to_string(),
            actor: principal.id.clone(),
            action: request.action.clone(),
            outcome: outcome.to_string(),
            reason: reason.to_string(),
            labels: request.labels.clone(),
            details: json!({
                "target": request.target,
                "arguments": request.arguments,
                "prompted": prompted,
                "trust_domain": principal.trust_domain,
            }),
        })?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediatedFile {
    pub text: String,
    pub labels: Labels,
    pub path: PathBuf,
}

/// Read a file the broker has already authorized.
///
/// The path is canonicalized and must stay under the matching grant.
/// Paths under `private_roots` are labeled private. Paths under `secret_roots`
/// are labeled secret. The caller does not choose those labels.
pub fn read_mediated_file(
    principal: &Principal,
    requested: &Path,
    private_roots: &[PathBuf],
    secret_roots: &[PathBuf],
) -> Result<MediatedFile, BrokerError> {
    let canonical = fs::canonicalize(requested).map_err(|err| {
        BrokerError::File(format!("cannot resolve {}: {err}", requested.display()))
    })?;
    let grant = principal
        .file_grant("files.read", &canonical)
        .ok_or_else(|| {
            BrokerError::File(format!(
                "resolved path {} is outside the grant",
                canonical.display()
            ))
        })?;
    let _ = grant;
    let text = fs::read_to_string(&canonical).map_err(|err| BrokerError::File(err.to_string()))?;
    if text.len() > 1024 * 1024 {
        return Err(BrokerError::File("file is larger than 1 MiB".into()));
    }
    let confidentiality = if under_any(&canonical, secret_roots) {
        Confidentiality::Secret
    } else if under_any(&canonical, private_roots) {
        Confidentiality::Private
    } else {
        Confidentiality::Public
    };
    Ok(MediatedFile {
        text,
        labels: Labels {
            integrity: Integrity::Trusted,
            confidentiality,
            origin: format!("file:{}", canonical.display()),
            owner: principal.id.clone(),
        },
        path: canonical,
    })
}

fn under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let Ok(root) = fs::canonicalize(root) else {
            return false;
        };
        path == root || path.starts_with(&root)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dj_label::Integrity;

    fn principal() -> Principal {
        Principal {
            id: "agent-a".into(),
            trust_domain: "domain-a".into(),
            grants: BTreeSet::from([
                "email.send".to_string(),
                "model.use:default-chat".to_string(),
                "files.read:/workspace".to_string(),
            ]),
            local_only: false,
        }
    }

    fn broker(dir: &Path) -> Broker {
        Broker::new(
            Policy::bundled(),
            AuditLog::open(dir.join("audit.log")).unwrap(),
        )
    }

    #[test]
    fn tainted_email_is_prompted_and_denied() {
        let dir = tempfile::tempdir().unwrap();
        let broker = broker(dir.path());
        let labels = Labels {
            integrity: Integrity::Untrusted,
            origin: "web:page".into(),
            ..Labels::user("agent-a")
        };
        let authz = broker
            .authorize(
                &principal(),
                &ActionRequest {
                    action: "email.send".into(),
                    target: "attacker@example.com".into(),
                    labels,
                    arguments: json!({"to": "attacker@example.com"}),
                    route: None,
                },
            )
            .unwrap();
        assert!(!authz.allowed);
        assert!(authz.prompted);
        let records = dj_audit::read_records(&dir.path().join("audit.log")).unwrap();
        let kinds: Vec<_> = records.iter().map(|r| r.kind.as_str()).collect();
        assert!(kinds.contains(&"approval_prompt"));
        assert!(kinds.contains(&"denial"));
    }

    #[test]
    fn private_root_label_is_not_chosen_by_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let private_dir = dir.path().join("private");
        fs::create_dir_all(&private_dir).unwrap();
        let file = private_dir.join("notes.txt");
        fs::write(&file, "CANARY_SECRET_DO_NOT_LEAK").unwrap();
        let outside = dir.path().join("link");
        std::os::unix::fs::symlink("/etc/hostname", &outside).unwrap();

        let mut who = principal();
        who.grants = BTreeSet::from([format!("files.read:{}", private_dir.display())]);
        let mediated =
            read_mediated_file(&who, &file, std::slice::from_ref(&private_dir), &[]).unwrap();
        assert_eq!(mediated.labels.confidentiality, Confidentiality::Private);
        assert!(mediated.text.contains("CANARY_SECRET_DO_NOT_LEAK"));

        let err = read_mediated_file(&who, &outside, &[private_dir], &[]).unwrap_err();
        assert!(err.to_string().contains("outside the grant"));
    }
}
