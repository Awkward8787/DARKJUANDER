use crate::config::{ConfigError, FileConfig};
use dj_audit::{AuditLog, AuditPaths, Event};
use dj_broker::{Broker, Principal};
use dj_label::Labels;
use dj_policy::Policy;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use umbra::credential::{CredentialFormat, CredentialSource};
use umbra::engine::llama::LlamaEngine;
use umbra::engine::mock::MockEngine;
use umbra::engine::remote::{RemoteConfig, RemoteEngine, Wire};
use umbra::engine::Engine;
use umbra::provider::{ProviderAuth, ProviderRecord, WireKind};
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
    enabled_providers: BTreeSet<String>,
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
    pub fn from_config(mut cfg: FileConfig) -> Result<Self, ConfigError> {
        let provider_records = load_registry(&cfg)?;
        apply_registry(&mut cfg, &provider_records)?;
        let enabled_providers = cfg
            .providers
            .iter()
            .filter(|(_, toggle)| toggle.granted)
            .map(|(id, _)| id.clone())
            .collect::<BTreeSet<_>>();
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
                provider: model.provider.clone(),
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
                "openai-compatible" => Arc::new(build_remote(name, engine, &provider_records)?),
                _ => unreachable!("checked in FileConfig::parse"),
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
                enabled_providers,
                spans: Mutex::new(HashMap::new()),
                last_labels: Mutex::new(HashMap::new()),
                psi_override: Mutex::new(None),
                chat_seq: AtomicU64::new(0),
            }),
        })
    }
}

fn load_registry(cfg: &FileConfig) -> Result<BTreeMap<String, ProviderRecord>, ConfigError> {
    let Some(path) = &cfg.provider_registry else {
        return Ok(BTreeMap::new());
    };
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    #[derive(serde::Deserialize)]
    struct File {
        version: u32,
        #[serde(default)]
        providers: BTreeMap<String, ProviderRecord>,
    }
    let text = std::fs::read_to_string(path)?;
    let file: File = serde_yaml::from_str(&text)?;
    if file.version != 1 {
        return Err(ConfigError::Invalid(format!(
            "unsupported provider registry version {}",
            file.version
        )));
    }
    Ok(file.providers)
}

fn apply_registry(
    cfg: &mut FileConfig,
    registry: &BTreeMap<String, ProviderRecord>,
) -> Result<(), ConfigError> {
    for (id, record) in registry {
        let toggle = cfg.providers.entry(id.clone()).or_default();
        if record.granted {
            toggle.granted = true;
        }
        if !record.granted || record.default_model.is_empty() {
            continue;
        }
        if cfg.models.contains_key(&record.default_model) {
            continue;
        }
        let engine_name = format!("provider-{id}");
        cfg.engines.insert(
            engine_name.clone(),
            crate::config::EngineConfig {
                kind: "openai-compatible".into(),
                base_url: Some(record.base_url.clone()),
                timeout_secs: Some(60),
                provider: Some(id.clone()),
                retries: Some(2),
                stream: Some(matches!(record.wire, WireKind::Responses)),
                backoff_ms: Some(200),
                credential_file: match &record.credential {
                    CredentialSource::File { path, .. } => Some(path.clone()),
                    CredentialSource::Systemd { .. } => None,
                },
                credential_systemd: match &record.credential {
                    CredentialSource::Systemd { name, .. } => Some(name.clone()),
                    CredentialSource::File { .. } => None,
                },
                wire: Some(
                    match record.wire {
                        WireKind::ChatCompletions => "chat-completions",
                        WireKind::Responses => "responses",
                    }
                    .into(),
                ),
            },
        );
        cfg.models.insert(
            record.default_model.clone(),
            crate::config::ModelConfig {
                engine: engine_name,
                location: "cloud".into(),
                size: "large".into(),
                memory_mb: 0,
                provider: Some(id.clone()),
            },
        );
        if let Some(alias_name) = &record.alias {
            if let Some(alias) = cfg.aliases.get_mut(alias_name) {
                alias.cloud = Some(record.default_model.clone());
            }
        }
    }
    Ok(())
}

fn build_remote(
    name: &str,
    engine: &crate::config::EngineConfig,
    registry: &BTreeMap<String, ProviderRecord>,
) -> Result<RemoteEngine, ConfigError> {
    let provider = engine.provider.clone().unwrap_or_else(|| name.to_string());
    let record = registry.get(&provider);
    let base_url = engine
        .base_url
        .clone()
        .filter(|url| !url.is_empty())
        .or_else(|| record.map(|record| record.base_url.clone()))
        .ok_or_else(|| ConfigError::Invalid(format!("engine {name} needs base_url")))?;
    let credential = if let Some(path) = &engine.credential_file {
        CredentialSource::File {
            path: path.clone(),
            format: credential_format(engine, record),
        }
    } else if let Some(systemd_name) = &engine.credential_systemd {
        CredentialSource::Systemd {
            name: systemd_name.clone(),
            format: credential_format(engine, record),
        }
    } else if let Some(record) = record {
        record.credential.clone()
    } else {
        return Err(ConfigError::Invalid(format!(
            "engine {name} needs a credential file or systemd credential"
        )));
    };
    let wire = match engine.wire.as_deref() {
        Some("responses") => Wire::Responses,
        Some("chat-completions") | None => match credential.format() {
            CredentialFormat::ChatgptSubscription => Wire::Responses,
            CredentialFormat::ApiKey => Wire::ChatCompletions,
        },
        Some(other) => {
            return Err(ConfigError::Invalid(format!(
                "engine {name} has unknown wire {other}"
            )))
        }
    };
    RemoteEngine::new(RemoteConfig {
        name: name.to_string(),
        provider,
        base_url,
        timeout: Duration::from_secs(engine.timeout_secs.unwrap_or(60).max(1)),
        retries: engine.retries.unwrap_or(2),
        backoff: Duration::from_millis(engine.backoff_ms.unwrap_or(200)),
        stream: engine.stream.unwrap_or(matches!(wire, Wire::Responses)),
        wire,
        credential,
    })
    .map_err(|err| ConfigError::Invalid(err.to_string()))
}

fn credential_format(
    engine: &crate::config::EngineConfig,
    record: Option<&ProviderRecord>,
) -> CredentialFormat {
    if engine.wire.as_deref() == Some("responses") {
        return CredentialFormat::ChatgptSubscription;
    }
    match record.map(|record| record.auth) {
        Some(ProviderAuth::ChatgptSubscription) => CredentialFormat::ChatgptSubscription,
        _ => CredentialFormat::ApiKey,
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

    pub fn enabled_providers(&self) -> &BTreeSet<String> {
        &self.inner.enabled_providers
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
