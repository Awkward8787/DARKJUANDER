//! Operator registry for cloud providers.
//!
//! `dj provider add` stores a key and leaves the provider disabled. Routing
//! stays off until `dj provider grant`. The registry file holds no key material.

#![forbid(unsafe_code)]

use crate::credential::{self, install_secret, shred, CredentialFormat, CredentialSource, Secret};
use crate::engine::remote::{RemoteConfig, RemoteEngine, Wire};
use crate::engine::{ChatMessage, Engine, EngineRequest};
use crate::siwc::{self, Endpoints, SubscriptionCredential};
use dj_label::Confidentiality;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

pub const XAI_BASE: &str = "https://api.x.ai/v1";
pub const OPENAI_BASE: &str = "https://api.openai.com/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderAuth {
    ApiKey,
    ChatgptSubscription,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: String,
    pub granted: bool,
    pub base_url: String,
    pub default_model: String,
    pub auth: ProviderAuth,
    pub wire: WireKind,
    pub credential: CredentialSource,
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireKind {
    ChatCompletions,
    Responses,
}

impl WireKind {
    pub fn engine(self) -> Wire {
        match self {
            Self::ChatCompletions => Wire::ChatCompletions,
            Self::Responses => Wire::Responses,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RegistryFile {
    version: u32,
    providers: BTreeMap<String, ProviderRecord>,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider io: {0}")]
    Io(#[from] std::io::Error),
    #[error("provider yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("provider json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider: {0}")]
    Invalid(String),
    #[error(transparent)]
    Credential(#[from] credential::CredentialError),
    #[error(transparent)]
    SignIn(#[from] siwc::SiwcError),
}

pub struct ProviderStore {
    root: PathBuf,
}

impl ProviderStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn registry_path(&self) -> PathBuf {
        self.root.join("providers").join("registry.yaml")
    }

    pub fn host_id_path(&self) -> PathBuf {
        self.root.join("providers").join("host-id")
    }

    pub fn add_api_key(
        &self,
        id: &str,
        key: &str,
        model: &str,
        systemd: bool,
    ) -> Result<ProviderRecord, ProviderError> {
        let preset = preset(id)?;
        let key = key.trim();
        if key.is_empty() || key.contains('\n') || key.contains('\r') {
            return Err(ProviderError::Invalid(
                "refusing an empty or multi-line api key".into(),
            ));
        }
        if model.is_empty() || model.contains('/') || model.contains(' ') {
            return Err(ProviderError::Invalid(
                "model id must be a single token from the provider's model list".into(),
            ));
        }
        let path = self.root.join("credentials").join(format!("{id}.key"));
        install_secret(&path, format!("{key}\n").as_bytes())?;
        let credential = if systemd {
            CredentialSource::Systemd {
                name: format!("dj-provider-{id}"),
                format: CredentialFormat::ApiKey,
            }
        } else {
            CredentialSource::File {
                path,
                format: CredentialFormat::ApiKey,
            }
        };
        let record = ProviderRecord {
            id: id.to_string(),
            granted: false,
            base_url: preset.base_url.to_string(),
            default_model: model.to_string(),
            auth: ProviderAuth::ApiKey,
            wire: WireKind::ChatCompletions,
            credential,
            alias: None,
        };
        self.upsert(record.clone())?;
        Ok(record)
    }

    pub fn save_subscription(
        &self,
        record: SubscriptionCredential,
    ) -> Result<ProviderRecord, ProviderError> {
        let path = self
            .root
            .join("credentials")
            .join("openai.subscription.json");
        let bytes = serde_json::to_vec_pretty(&record)?;
        install_secret(&path, &bytes)?;
        let provider = ProviderRecord {
            id: "openai".into(),
            granted: false,
            base_url: OPENAI_BASE.into(),
            default_model: String::new(),
            auth: ProviderAuth::ChatgptSubscription,
            wire: WireKind::Responses,
            credential: CredentialSource::File {
                path,
                format: CredentialFormat::ChatgptSubscription,
            },
            alias: None,
        };
        let mut existing = self.load()?;
        if let Some(previous) = existing.get("openai") {
            let mut provider = provider;
            provider.granted = previous.granted;
            provider.default_model = previous.default_model.clone();
            provider.alias = previous.alias.clone();
            existing.insert("openai".into(), provider.clone());
            self.write(&existing)?;
            return Ok(provider);
        }
        existing.insert("openai".into(), provider.clone());
        self.write(&existing)?;
        Ok(provider)
    }

    pub fn list(&self) -> Result<Vec<ProviderRecord>, ProviderError> {
        let map = self.load()?;
        Ok(map.into_values().collect())
    }

    pub fn get(&self, id: &str) -> Result<ProviderRecord, ProviderError> {
        self.load()?
            .remove(id)
            .ok_or_else(|| ProviderError::Invalid(format!("unknown provider {id}")))
    }

    pub fn set_granted(&self, id: &str, granted: bool) -> Result<ProviderRecord, ProviderError> {
        let mut map = self.load()?;
        let record = map
            .get_mut(id)
            .ok_or_else(|| ProviderError::Invalid(format!("unknown provider {id}")))?;
        record.granted = granted;
        let record = record.clone();
        self.write(&map)?;
        Ok(record)
    }

    pub fn set_model(&self, id: &str, model: &str) -> Result<(), ProviderError> {
        if model.is_empty() {
            return Err(ProviderError::Invalid("model id is empty".into()));
        }
        let mut map = self.load()?;
        let record = map
            .get_mut(id)
            .ok_or_else(|| ProviderError::Invalid(format!("unknown provider {id}")))?;
        record.default_model = model.to_string();
        self.write(&map)?;
        Ok(())
    }

    pub fn remove(&self, id: &str) -> Result<(), ProviderError> {
        preset(id)?;
        let mut map = self.load()?;
        let Some(record) = map.remove(id) else {
            return Err(ProviderError::Invalid(format!("unknown provider {id}")));
        };
        if record.auth == ProviderAuth::ChatgptSubscription {
            if let CredentialSource::File { path, .. } = &record.credential {
                if let Ok(bytes) = credential::read_owner_only(path) {
                    if let Ok(subscription) =
                        serde_json::from_slice::<SubscriptionCredential>(&bytes)
                    {
                        if let Err(err) = siwc::revoke(&subscription, Duration::from_secs(5)) {
                            eprintln!(
                                "dj: remote revocation was not confirmed ({err}); local tokens were removed"
                            );
                        }
                    }
                }
                shred(path)?;
            }
        }
        if let CredentialSource::File { path, .. } = &record.credential {
            shred(path)?;
        }
        self.write(&map)?;
        Ok(())
    }

    pub fn probe(
        &self,
        id: &str,
        base_url: Option<&str>,
        allow_untrusted_host: bool,
    ) -> Result<String, ProviderError> {
        let record = self.get(id)?;
        if record.default_model.is_empty() {
            return Err(ProviderError::Invalid(
                "set a model with `dj provider add --model` before testing".into(),
            ));
        }
        let base_url = base_url.unwrap_or(&record.base_url);
        probe_target_allowed(&record.id, base_url, allow_untrusted_host)?;
        let engine = RemoteEngine::new(RemoteConfig {
            name: record.id.clone(),
            provider: record.id.clone(),
            base_url: base_url.to_string(),
            timeout: Duration::from_secs(30),
            retries: 1,
            backoff: Duration::from_millis(200),
            stream: matches!(record.wire, WireKind::Responses),
            wire: record.wire.engine(),
            credential: record.credential.clone(),
        })
        .map_err(|err| ProviderError::Invalid(err.to_string()))?;
        let response = engine
            .complete(&EngineRequest {
                model: record.default_model.clone(),
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "ping".into(),
                }],
                confidentiality: Some(Confidentiality::Public),
            })
            .map_err(|err| ProviderError::Invalid(err.to_string()))?;
        Ok(format!(
            "provider {} model {} ok ({} completion tokens)",
            record.id, record.default_model, response.completion_tokens
        ))
    }

    /// Issued client id, retained id token, email, and subject for a later OpenAI sign-in.
    /// The id token is a credential. Callers must not print it.
    pub fn openai_reauth_hint(&self) -> Option<OpenAiReauth> {
        let record = self.get("openai").ok()?;
        let CredentialSource::File { path, .. } = &record.credential else {
            return None;
        };
        let bytes = credential::read_owner_only(path).ok()?;
        let saved: SubscriptionCredential = serde_json::from_slice(&bytes).ok()?;
        Some(OpenAiReauth {
            client_id: saved.client_id,
            id_token: saved.id_token,
            email: saved.email,
            subject: saved.subject,
        })
    }

    pub fn host_id(&self) -> Result<String, ProviderError> {
        let path = self.host_id_path();
        if path.exists() {
            let bytes = credential::read_owner_only(&path)?;
            let text = String::from_utf8(bytes)
                .map_err(|_| ProviderError::Invalid("host id file is not utf-8".into()))?;
            let text = text.trim().to_string();
            if text.starts_with("urn:uuid:") {
                return Ok(text);
            }
        }
        let id = siwc::new_host_id()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        install_secret(&path, format!("{id}\n").as_bytes())?;
        Ok(id)
    }

    fn upsert(&self, record: ProviderRecord) -> Result<(), ProviderError> {
        let mut map = self.load()?;
        map.insert(record.id.clone(), record);
        self.write(&map)
    }

    pub fn load(&self) -> Result<BTreeMap<String, ProviderRecord>, ProviderError> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&path)?;
        let file: RegistryFile = serde_yaml::from_str(&text)?;
        if file.version != 1 {
            return Err(ProviderError::Invalid(format!(
                "unsupported provider registry version {}",
                file.version
            )));
        }
        for (id, record) in &file.providers {
            if id != &record.id {
                return Err(ProviderError::Invalid(
                    "provider id does not match its registry key".into(),
                ));
            }
            preset(id)?;
        }
        Ok(file.providers)
    }

    fn write(&self, providers: &BTreeMap<String, ProviderRecord>) -> Result<(), ProviderError> {
        let path = self.registry_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
        let file = RegistryFile {
            version: 1,
            providers: providers.clone(),
        };
        let text = serde_yaml::to_string(&file)?;
        install_secret(&path, text.as_bytes())?;
        Ok(())
    }
}

pub struct Preset {
    pub id: &'static str,
    pub base_url: &'static str,
}

pub fn preset(id: &str) -> Result<Preset, ProviderError> {
    match id {
        "xai" => Ok(Preset {
            id: "xai",
            base_url: XAI_BASE,
        }),
        "openai" => Ok(Preset {
            id: "openai",
            base_url: OPENAI_BASE,
        }),
        other => Err(ProviderError::Invalid(format!(
            "unknown provider `{other}`; supported ids are xai and openai"
        ))),
    }
}

pub fn read_key_source(path: Option<&Path>) -> Result<Secret, ProviderError> {
    let mut bytes = Vec::new();
    match path {
        Some(path) => {
            bytes = fs::read(path)?;
        }
        None => {
            std::io::stdin().read_to_end(&mut bytes)?;
        }
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| ProviderError::Invalid("api key is not utf-8".into()))?;
    Ok(Secret::new(text))
}

/// Where `dj provider test` may send the bearer key.
///
/// Loopback may be http. Every other host must be https. The preset host
/// is allowed; any other https host needs `allow_untrusted_host`. This
/// returns before a socket is opened.
pub fn probe_target_allowed(
    provider_id: &str,
    url: &str,
    allow_untrusted_host: bool,
) -> Result<(), ProviderError> {
    if url.contains('@') {
        return Err(ProviderError::Invalid(
            "refusing a provider URL that embeds credentials".into(),
        ));
    }
    let (scheme, host) = url_origin(url)?;
    if scheme == "http" && !is_loopback(&host) {
        return Err(ProviderError::Invalid(format!(
            "refusing cleartext provider URL for {host}"
        )));
    }
    if is_loopback(&host) {
        return Ok(());
    }
    let preset_host = url_host(preset(provider_id)?.base_url)?;
    if host.eq_ignore_ascii_case(&preset_host) {
        return Ok(());
    }
    if allow_untrusted_host {
        return Ok(());
    }
    Err(ProviderError::Invalid(format!(
        "refusing to send the {provider_id} key to {host}; pass --allow-untrusted-host to override"
    )))
}

fn url_origin(url: &str) -> Result<(&str, String), ProviderError> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err(ProviderError::Invalid(
            "provider URL must be http or https".into(),
        ));
    };
    Ok((scheme, url_host_authority(rest)?))
}

fn url_host(url: &str) -> Result<String, ProviderError> {
    Ok(url_origin(url)?.1)
}

fn url_host_authority(rest: &str) -> Result<String, ProviderError> {
    let authority = rest.split('/').next().unwrap_or("");
    if let Some(host) = authority.strip_prefix('[') {
        let end = host
            .find(']')
            .ok_or_else(|| ProviderError::Invalid("provider URL has a bad IPv6 host".into()))?;
        let host = &host[..end];
        if host.is_empty() {
            return Err(ProviderError::Invalid("provider URL has no host".into()));
        }
        return Ok(host.to_string());
    }
    let host = authority
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(authority);
    if host.is_empty() {
        return Err(ProviderError::Invalid("provider URL has no host".into()));
    }
    Ok(host.to_string())
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

#[derive(Clone, Debug)]
pub struct OpenAiReauth {
    pub client_id: String,
    pub id_token: String,
    pub email: Option<String>,
    pub subject: String,
}

pub fn subscription_available(id: &str) -> Result<(), ProviderError> {
    match id {
        "openai" => Ok(()),
        "xai" => Err(ProviderError::Invalid(
            "xAI documents API keys at https://docs.x.ai/developers/quickstart. SuperGrok is not a documented third-party OAuth or device flow. Refusing undocumented session login.".into(),
        )),
        other => Err(ProviderError::Invalid(format!(
            "no documented subscription sign-in for {other}"
        ))),
    }
}

pub fn production_endpoints() -> Endpoints {
    Endpoints::documented()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_grant_remove_never_stores_the_key_in_the_registry() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::new(dir.path());
        let record = store
            .add_api_key("xai", "sk-test-CANARYKEY", "grok-4.7", false)
            .unwrap();
        assert!(!record.granted);
        let registry = fs::read_to_string(store.registry_path()).unwrap();
        assert!(!registry.contains("CANARYKEY"));
        assert!(registry.contains("https://api.x.ai/v1"));
        let mode = fs::metadata(dir.path().join("credentials/xai.key"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let granted = store.set_granted("xai", true).unwrap();
        assert!(granted.granted);
        store.remove("xai").unwrap();
        assert!(!dir.path().join("credentials/xai.key").exists());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn unknown_provider_and_xai_subscription_are_refused() {
        assert!(preset("anthropic").is_err());
        let err = subscription_available("xai").unwrap_err();
        assert!(err.to_string().contains("docs.x.ai"));
        assert!(subscription_available("openai").is_ok());
    }

    #[test]
    fn probe_base_url_stays_on_the_preset_host_or_loopback() {
        assert!(probe_target_allowed("xai", "https://api.x.ai/v1", false).is_ok());
        assert!(probe_target_allowed("openai", "https://api.openai.com/v1", false).is_ok());
        let clear = probe_target_allowed("xai", "http://api.x.ai/v1", false).unwrap_err();
        assert!(clear.to_string().contains("cleartext"), "{clear}");
        assert!(probe_target_allowed("xai", "http://api.x.ai/v1", true).is_err());
        assert!(probe_target_allowed("openai", "http://api.openai.com/v1", true).is_err());
        assert!(probe_target_allowed("xai", "http://collector.invalid/v1", true).is_err());
        assert!(probe_target_allowed("xai", "http://127.0.0.1:9/v1", false).is_ok());
        assert!(probe_target_allowed("xai", "http://[::1]:9/v1", false).is_ok());
        let evil = probe_target_allowed("xai", "https://collector.invalid/v1", false).unwrap_err();
        assert!(evil.to_string().contains("--allow-untrusted-host"));
        assert!(probe_target_allowed("xai", "https://api.openai.com/v1", false).is_err());
        assert!(probe_target_allowed("xai", "https://api.x.ai.evil.com/v1", false).is_err());
        assert!(probe_target_allowed("xai", "https://collector.invalid/v1", true).is_ok());
        assert!(probe_target_allowed("xai", "https://user:secret@api.x.ai/v1", true).is_err());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let payload = r#"{"choices":[{"message":{"content":"pong"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        });
        let dir = tempfile::tempdir().unwrap();
        let store = ProviderStore::new(dir.path());
        store
            .add_api_key("xai", "sk-test-CANARYKEY", "grok-test", false)
            .unwrap();
        let refused = store
            .probe("xai", Some("https://collector.invalid/v1"), false)
            .unwrap_err();
        assert!(refused.to_string().contains("--allow-untrusted-host"));
        assert!(!refused.to_string().contains("CANARYKEY"));
        let ok = store
            .probe("xai", Some(&format!("http://127.0.0.1:{port}/v1")), false)
            .unwrap();
        assert!(ok.contains("xai"));
        assert!(!ok.contains("CANARYKEY"));
    }
}
