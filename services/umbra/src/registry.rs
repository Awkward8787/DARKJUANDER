//! One resident record per model. Callers share that record.

#![forbid(unsafe_code)]

use crate::route::{Location, ModelSpec, SizeClass};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct ResidentView {
    pub id: String,
    pub location: String,
    pub size: String,
    pub engine: String,
    pub memory_mb: u64,
    pub resident_copies: u32,
    pub callers: BTreeSet<String>,
    pub requests: u64,
}

#[derive(Default)]
pub struct Registry {
    models: BTreeMap<String, Resident>,
}

struct Resident {
    spec: ModelSpec,
    callers: BTreeSet<String>,
    resident_copies: u32,
    requests: u64,
}

impl Registry {
    pub fn insert(&mut self, spec: ModelSpec) {
        self.models.insert(
            spec.id.clone(),
            Resident {
                spec,
                callers: BTreeSet::new(),
                resident_copies: 0,
                requests: 0,
            },
        );
    }

    pub fn record_use(&mut self, model: &str, caller: &str) {
        let Some(resident) = self.models.get_mut(model) else {
            return;
        };
        resident.resident_copies = 1;
        resident.callers.insert(caller.to_string());
        resident.requests += 1;
    }

    pub fn view(&self) -> Vec<ResidentView> {
        self.models
            .values()
            .map(|resident| ResidentView {
                id: resident.spec.id.clone(),
                location: location_name(resident.spec.location).into(),
                size: size_name(resident.spec.size).into(),
                engine: resident.spec.engine.clone(),
                memory_mb: resident.spec.memory_mb,
                resident_copies: resident.resident_copies,
                callers: resident.callers.clone(),
                requests: resident.requests,
            })
            .collect()
    }
}

pub fn location_name(location: Location) -> &'static str {
    match location {
        Location::Local => "local",
        Location::Cloud => "cloud",
    }
}

pub fn size_name(size: SizeClass) -> &'static str {
    match size {
        SizeClass::Small => "small",
        SizeClass::Mid => "mid",
        SizeClass::Large => "large",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_callers_one_copy() {
        let mut registry = Registry::default();
        registry.insert(ModelSpec {
            id: "mid-local".into(),
            location: Location::Local,
            size: SizeClass::Mid,
            memory_mb: 8000,
            engine: "mock".into(),
            provider: None,
        });
        registry.record_use("mid-local", "agent-a");
        registry.record_use("mid-local", "agent-b");
        let view = registry.view();
        assert_eq!(view[0].resident_copies, 1);
        assert_eq!(view[0].callers.len(), 2);
        assert_eq!(view[0].memory_mb, 8000);
        assert_eq!(view[0].requests, 2);
    }
}
