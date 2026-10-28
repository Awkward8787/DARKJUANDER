use crate::config::{ConfigError, FileConfig};
use dj_audit::{AuditLog, AuditPaths, Event};
use dj_broker::{Broker, Principal};
use dj_label::Labels;
use dj_policy::Policy;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use umbra::engine::llama::LlamaEngine;
use umbra::engine::mock::MockEngine;
use umbra::engine::Engine;
use umbra::{AliasSpec, Cache, Location, ModelSpec, Registry, SizeClass};

#[derive(Clone)]
pub struct StoredSpan {
    pub id: String,
    pub owner: String,
    pub text: String,
    pub labels: Labels,
}

struct Inner {
    cfg: FileConfig,
    by_token: HashMap<String, Principal>,
    policy: Policy,
    broker: Broker,
    registry: Mutex<Registry>,
    cache: Mutex<Cache>,
    engines: HashMap<String, Arc<dyn Engine>>,
    models: BTreeMap<String, ModelSpec>,
    aliases: BTreeMap<String, AliasSpec>,
    spans: Mutex<HashMap<String, StoredSpan>>,
    last_labels: Mutex<HashMap<String, Labels>>,
    psi_override: Mutex<Option<umbra::PsiSnapshot>>,
    chat_seq: AtomicU64,
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

impl AppState {
    pub fn from_config(cfg: FileConfig) -> Result<Self, ConfigError> {
        let policy = Policy::from_path(&cfg.policy_path).map_err(|err| {
            ConfigError::Invalid(format!("policy {}: {err}", cfg.policy_path.display()))
        })?;
        let audit = AuditLog::open(
            &cfg.audit_path,
            &AuditPaths {
                signer_socket: cfg.audit_signer_socket.clone(),
                verifying_key: cfg.audit_verifying_key.clone(),
            },
        )
        .map_err(|err| ConfigError::Invalid(format!("audit: {err}")))?;
        let mut models = BTreeMap::new();
        let mut registry = Registry::default();
        for (id, model) in &cfg.models {
            if !cfg.engines.contains_key(&model.engine) {
                return Err(ConfigError::Invalid(format!(
                    "model {id} uses unknown engine {}",
                    model.engine
                )));
            }
            let spec = ModelSpec {
                id: id.clone(),
                location: parse_location(&model.location)?,
                size: parse_size(&model.size)?,
                memory_mb: model.memory_mb,
                engine: model.engine.clone(),
            };
            registry.insert(spec.clone());
            models.insert(id.clone(), spec);
        }
        let mut aliases = BTreeMap::new();
        for (name, alias) in &cfg.aliases {
            for id in [&alias.small, &alias.mid]
                .into_iter()
                .chain(alias.cloud.iter())
            {
                if !models.contains_key(id) {
                    return Err(ConfigError::Invalid(format!(
                        "alias {name} points at unknown model {id}"
                    )));
                }
            }
            aliases.insert(
                name.clone(),
                AliasSpec {
                    small: alias.small.clone(),
                    mid: alias.mid.clone(),
                    cloud: alias.cloud.clone(),
                },
            );
        }
        let mut engines: HashMap<String, Arc<dyn Engine>> = HashMap::new();
        for (name, engine) in &cfg.engines {
            let boxed: Arc<dyn Engine> = match engine.kind.as_str() {
                "mock" => Arc::new(MockEngine::new(name.clone())),
                "llama-cpp" => Arc::new(LlamaEngine::new(
                    name.clone(),
                    engine.base_url.clone().unwrap_or_default(),
                    engine.timeout_secs.unwrap_or(120),
                )),
                _ => unreachable!("checked in FileConfig::from_str"),
            };
            engines.insert(name.clone(), boxed);
        }
        let mut by_token = HashMap::new();
        for principal in &cfg.principals {
            if principal.token.is_empty() || principal.id.is_empty() {
                return Err(ConfigError::Invalid(
                    "principals need an id and a token".into(),
                ));
            }
            by_token.insert(
                principal.token.clone(),
                Principal {
                    id: principal.id.clone(),
                    trust_domain: principal.trust_domain.clone(),
                    grants: principal.grants.iter().cloned().collect(),
                    local_only: principal.local_only,
                },
            );
        }
        Ok(Self {
            inner: Arc::new(Inner {
                cfg,
                by_token,
                broker: Broker::new(policy.clone(), audit),
                policy,
                registry: Mutex::new(registry),
                cache: Mutex::new(Cache::default()),
                engines,
                models,
                aliases,
                spans: Mutex::new(HashMap::new()),
                last_labels: Mutex::new(HashMap::new()),
                psi_override: Mutex::new(None),
                chat_seq: AtomicU64::new(0),
            }),
        })
    }
}

pub fn random_span_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("os entropy for a span id");
    format!("span-{}", hex::encode(bytes))
}

fn parse_location(value: &str) -> Result<Location, ConfigError> {
    match value {
        "local" => Ok(Location::Local),
        "cloud" => Ok(Location::Cloud),
        other => Err(ConfigError::Invalid(format!("unknown location {other}"))),
    }
}

fn parse_size(value: &str) -> Result<SizeClass, ConfigError> {
    match value {
        "small" => Ok(SizeClass::Small),
        "mid" => Ok(SizeClass::Mid),
        "large" => Ok(SizeClass::Large),
        other => Err(ConfigError::Invalid(format!("unknown size {other}"))),
    }
}

impl AppState {
    pub fn config(&self) -> &FileConfig {
        &self.inner.cfg
    }

    pub fn policy(&self) -> &Policy {
        &self.inner.policy
    }

    pub fn broker(&self) -> &Broker {
        &self.inner.broker
    }

    pub fn principal_for_token(&self, token: &str) -> Option<Principal> {
        self.inner.by_token.get(token).cloned()
    }

    pub fn models(&self) -> &BTreeMap<String, ModelSpec> {
        &self.inner.models
    }

    pub fn aliases(&self) -> &BTreeMap<String, AliasSpec> {
        &self.inner.aliases
    }

    pub fn engine(&self, name: &str) -> Option<Arc<dyn Engine>> {
        self.inner.engines.get(name).cloned()
    }

    pub fn engines(&self) -> &HashMap<String, Arc<dyn Engine>> {
        &self.inner.engines
    }

    pub fn registry(&self) -> &Mutex<Registry> {
        &self.inner.registry
    }

    pub fn cache(&self) -> &Mutex<Cache> {
        &self.inner.cache
    }

    pub fn audit(&self, event: Event) -> Result<(), dj_audit::AuditError> {
        self.inner.broker.audit().append(event).map(|_| ())
    }

    pub fn audit_path(&self) -> std::path::PathBuf {
        self.inner.broker.audit().path()
    }

    pub fn next_chat_id(&self) -> u64 {
        self.inner
            .chat_seq
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }

    pub fn store_span(&self, owner: String, text: String, labels: Labels) -> StoredSpan {
        let span = StoredSpan {
            id: random_span_id(),
            owner,
            text,
            labels,
        };
        self.inner
            .spans
            .lock()
            .expect("spans")
            .insert(span.id.clone(), span.clone());
        span
    }

    pub fn span(&self, id: &str) -> Option<StoredSpan> {
        self.inner.spans.lock().expect("spans").get(id).cloned()
    }

    pub fn remember_labels(&self, owner: &str, labels: Labels) {
        self.inner
            .last_labels
            .lock()
            .expect("labels")
            .insert(owner.to_string(), labels);
    }

    pub fn last_labels(&self, owner: &str) -> Option<Labels> {
        self.inner
            .last_labels
            .lock()
            .expect("labels")
            .get(owner)
            .cloned()
    }

    pub fn set_psi_override(&self, snapshot: Option<umbra::PsiSnapshot>) {
        *self.inner.psi_override.lock().expect("psi") = snapshot;
    }

    pub fn psi(&self) -> umbra::PsiSnapshot {
        if let Some(snapshot) = self.inner.psi_override.lock().expect("psi").clone() {
            return snapshot;
        }
        umbra::psi::read_snapshot(&self.inner.cfg.psi_memory_path, &self.inner.cfg.psi_io_path)
    }
}

#[cfg(test)]
mod tests {
    use super::random_span_id;

    #[test]
    fn span_ids_are_128_bits_and_not_sequential() {
        let first = random_span_id();
        let second = random_span_id();
        assert_ne!(first, second);
        assert!(first.starts_with("span-"));
        let hex_id = first.trim_start_matches("span-");
        assert_eq!(hex_id.len(), 32);
        assert!(hex_id.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(first, "span-1");
        assert_ne!(second, "span-2");
    }
}
