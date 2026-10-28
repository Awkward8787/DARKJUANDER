//! Hard rules first, then a pressure scorer.
//!
//! The scorer is a hand-written ranking, not a learned model (UM-10 is later).
//! It cannot put a removed candidate back into the eligible set.

#![forbid(unsafe_code)]

use crate::psi::PsiSnapshot;
use dj_label::Labels;
use dj_policy::{ActionClass, DecisionKind, Policy, PolicyRequest, RouteKind};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Location {
    Local,
    Cloud,
}

impl Location {
    pub fn kind(self) -> RouteKind {
        match self {
            Self::Local => RouteKind::Local,
            Self::Cloud => RouteKind::Cloud,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeClass {
    Small,
    Mid,
    Large,
}

#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub id: String,
    pub location: Location,
    pub size: SizeClass,
    pub memory_mb: u64,
    pub engine: String,
}

#[derive(Clone, Debug)]
pub struct AliasSpec {
    pub small: String,
    pub mid: String,
    pub cloud: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RemovedRoute {
    pub model: String,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RouteDecision {
    pub selected: String,
    pub location: Location,
    pub engine: String,
    pub reason: String,
    pub scorer_preferred: String,
    pub removed: Vec<RemovedRoute>,
    pub memory_some_avg10: f64,
    pub threshold: f64,
    pub elapsed_us: u128,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteError {
    #[error("unknown model or alias `{0}`")]
    Unknown(String),
    #[error("no route left after hard rules: {0}")]
    NoneLeft(String),
    #[error("alias `{0}` points at unknown model `{1}`")]
    BrokenAlias(String, String),
}

pub struct RouteInput<'a> {
    pub requested: &'a str,
    pub models: &'a BTreeMap<String, ModelSpec>,
    pub aliases: &'a BTreeMap<String, AliasSpec>,
    pub labels: &'a Labels,
    pub psi: &'a PsiSnapshot,
    pub threshold: f64,
    pub prefer_cloud: bool,
    pub local_only: bool,
    pub policy: &'a Policy,
    /// Concrete model ids this caller may run. `None` skips the grant filter,
    /// which the daemon does not do. The gateway always passes `Some`.
    pub granted: Option<&'a BTreeSet<String>>,
}

pub fn decide(input: &RouteInput<'_>) -> Result<RouteDecision, RouteError> {
    let started = Instant::now();
    let candidates = candidate_ids(input)?;
    let mut removed = Vec::new();
    let mut eligible = Vec::new();
    for id in candidates {
        let spec = input
            .models
            .get(&id)
            .ok_or_else(|| RouteError::BrokenAlias(input.requested.into(), id.clone()))?;
        if let Some(reason) = hard_block(input, spec) {
            removed.push(reason);
        } else {
            eligible.push(spec);
        }
    }
    let ranked: Vec<_> = input
        .models
        .values()
        .filter(|spec| {
            candidate_ids(input)
                .map(|ids| ids.iter().any(|id| id == &spec.id))
                .unwrap_or(false)
        })
        .collect();
    let scorer_preferred = ranked
        .iter()
        .max_by_key(|spec| score(spec, input))
        .map(|spec| spec.id.clone())
        .unwrap_or_else(|| "none".into());
    let selected = eligible
        .iter()
        .max_by_key(|spec| score(spec, input))
        .copied()
        .ok_or_else(|| {
            RouteError::NoneLeft(
                removed
                    .iter()
                    .map(|item| format!("{} ({})", item.model, item.rule_id))
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })?;
    let pressured = input.psi.memory_some_avg10 >= input.threshold;
    let mut reason = String::new();
    if !removed.is_empty() {
        let summary = removed
            .iter()
            .map(|item| format!("{} by {}", item.model, item.rule_id))
            .collect::<Vec<_>>()
            .join(", ");
        reason.push_str(&format!("hard rules removed {summary}; "));
    }
    if pressured {
        reason.push_str(&format!(
            "PSI memory some avg10 {:.2} above threshold {:.2}; ",
            input.psi.memory_some_avg10, input.threshold
        ));
    }
    reason.push_str(&format!(
        "scorer preferred {scorer_preferred}; selected {}",
        selected.id
    ));
    Ok(RouteDecision {
        selected: selected.id.clone(),
        location: selected.location,
        engine: selected.engine.clone(),
        reason,
        scorer_preferred,
        removed,
        memory_some_avg10: input.psi.memory_some_avg10,
        threshold: input.threshold,
        elapsed_us: started.elapsed().as_micros(),
    })
}

fn candidate_ids(input: &RouteInput<'_>) -> Result<Vec<String>, RouteError> {
    if let Some(alias) = input.aliases.get(input.requested) {
        let mut ids = vec![alias.small.clone(), alias.mid.clone()];
        if let Some(cloud) = &alias.cloud {
            ids.push(cloud.clone());
        }
        return Ok(ids);
    }
    if input.models.contains_key(input.requested) {
        return Ok(vec![input.requested.to_string()]);
    }
    Err(RouteError::Unknown(input.requested.to_string()))
}

fn hard_block(input: &RouteInput<'_>, spec: &ModelSpec) -> Option<RemovedRoute> {
    if let Some(granted) = input.granted {
        if !granted.contains(&spec.id) {
            return Some(RemovedRoute {
                model: spec.id.clone(),
                rule_id: "model-grant".into(),
                reason: format!("caller lacks model.use:{}", spec.id),
            });
        }
    }
    if input.local_only && spec.location == Location::Cloud {
        return Some(RemovedRoute {
            model: spec.id.clone(),
            rule_id: "local-only".into(),
            reason: "local-only mode is on".into(),
        });
    }
    let decision = input.policy.decide(&PolicyRequest {
        action: "model.use".into(),
        action_class: ActionClass::Model,
        labels: input.labels.clone(),
        route: Some(spec.location.kind()),
    });
    if decision.kind == DecisionKind::Deny {
        return Some(RemovedRoute {
            model: spec.id.clone(),
            rule_id: decision.rule_id,
            reason: decision.reason,
        });
    }
    None
}

fn score(spec: &ModelSpec, input: &RouteInput<'_>) -> i32 {
    let pressured = input.psi.memory_some_avg10 >= input.threshold;
    match (spec.location, spec.size) {
        (Location::Cloud, _) => {
            if input.prefer_cloud && !pressured {
                100
            } else {
                10
            }
        }
        (Location::Local, SizeClass::Small) => {
            if pressured {
                200
            } else {
                20
            }
        }
        (Location::Local, SizeClass::Mid) => {
            if pressured {
                30
            } else {
                80
            }
        }
        (Location::Local, SizeClass::Large) => {
            if pressured {
                0
            } else {
                70
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dj_label::{Confidentiality, Labels};

    fn catalog() -> (BTreeMap<String, ModelSpec>, BTreeMap<String, AliasSpec>) {
        let mut models = BTreeMap::new();
        for (id, location, size, memory, engine) in [
            (
                "small-local",
                Location::Local,
                SizeClass::Small,
                2500,
                "mock",
            ),
            ("mid-local", Location::Local, SizeClass::Mid, 8000, "mock"),
            (
                "cloud-remote",
                Location::Cloud,
                SizeClass::Large,
                0,
                "mock-cloud",
            ),
        ] {
            models.insert(
                id.into(),
                ModelSpec {
                    id: id.into(),
                    location,
                    size,
                    memory_mb: memory,
                    engine: engine.into(),
                },
            );
        }
        let mut aliases = BTreeMap::new();
        aliases.insert(
            "default-chat".into(),
            AliasSpec {
                small: "small-local".into(),
                mid: "mid-local".into(),
                cloud: Some("cloud-remote".into()),
            },
        );
        (models, aliases)
    }

    fn input<'a>(
        models: &'a BTreeMap<String, ModelSpec>,
        aliases: &'a BTreeMap<String, AliasSpec>,
        policy: &'a Policy,
        labels: &'a Labels,
        psi: &'a PsiSnapshot,
        local_only: bool,
    ) -> RouteInput<'a> {
        RouteInput {
            requested: "default-chat",
            models,
            aliases,
            labels,
            psi,
            threshold: 25.0,
            prefer_cloud: true,
            local_only,
            policy,
            granted: None,
        }
    }

    #[test]
    fn alias_grant_does_not_keep_an_ungranted_cloud_model() {
        let (models, aliases) = catalog();
        let policy = Policy::bundled();
        let labels = Labels::user("agent");
        let psi = PsiSnapshot::idle();
        let mut granted = BTreeSet::new();
        granted.insert("mid-local".into());
        granted.insert("small-local".into());
        let mut input = input(&models, &aliases, &policy, &labels, &psi, false);
        input.granted = Some(&granted);
        let decision = decide(&input).unwrap();
        assert_eq!(decision.selected, "mid-local");
        assert!(decision
            .removed
            .iter()
            .any(|item| item.model == "cloud-remote" && item.rule_id == "model-grant"));
    }

    #[test]
    fn pressure_selects_small_and_names_psi() {
        let (models, aliases) = catalog();
        let policy = Policy::bundled();
        let labels = Labels::user("agent");
        let psi = PsiSnapshot {
            memory_some_avg10: 80.0,
            memory_full_avg10: 4.0,
            io_some_avg10: 0.0,
            source: "test".into(),
        };
        let decision = decide(&input(&models, &aliases, &policy, &labels, &psi, true)).unwrap();
        assert_eq!(decision.selected, "small-local");
        assert!(decision.reason.contains("PSI memory some avg10"));
        assert!(decision.elapsed_us < 5_000);
    }

    #[test]
    fn private_label_blocks_the_scorer_cloud_preference() {
        let (models, aliases) = catalog();
        let policy = Policy::bundled();
        let labels = Labels {
            confidentiality: Confidentiality::Private,
            origin: "file:/workspace/private/notes.txt".into(),
            ..Labels::user("agent")
        };
        let psi = PsiSnapshot::idle();
        let decision = decide(&input(&models, &aliases, &policy, &labels, &psi, false)).unwrap();
        assert_eq!(decision.scorer_preferred, "cloud-remote");
        assert_eq!(decision.selected, "mid-local");
        assert_eq!(decision.location, Location::Local);
        assert!(
            decision
                .removed
                .iter()
                .any(|item| item.model == "cloud-remote"
                    && item.rule_id == "private-data-stays-local")
        );
    }
}
