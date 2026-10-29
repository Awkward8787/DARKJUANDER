//! `dj provider` stores cloud credentials and leaves routing off until grant.

use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use umbra::provider::{self, ProviderAuth, ProviderStore};
use umbra::siwc::{self, IdCheck};

pub fn state_dir(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| {
        std::env::var_os("DJ_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/darkjuander"))
    })
}

pub fn add(
    state: &Path,
    id: &str,
    key_file: Option<&Path>,
    model: &str,
    systemd: bool,
) -> Result<(), String> {
    let secret = provider::read_key_source(key_file).map_err(|err| err.to_string())?;
    let store = ProviderStore::new(state);
    let record = store
        .add_api_key(id, secret.expose(), model, systemd)
        .map_err(|err| err.to_string())?;
    println!(
        "added provider {} (granted: false, model {}, auth api-key)",
        record.id, record.default_model
    );
    println!(
        "cloud routing stays off until: dj provider grant {}",
        record.id
    );
    if systemd {
        println!("add this drop-in, then restart umbrad. The key is not in this snippet.");
        println!(
            "[Service]\nLoadCredential=dj-provider-{}:{}",
            record.id,
            credential_path(state, &record.id)
        );
    }
    print_yaml_hint(&record.id, &record.default_model);
    Ok(())
}

pub fn list(state: &Path) -> Result<(), String> {
    let records = ProviderStore::new(state)
        .list()
        .map_err(|err| err.to_string())?;
    if records.is_empty() {
        println!("no providers");
        return Ok(());
    }
    for record in records {
        let auth = match record.auth {
            ProviderAuth::ApiKey => "api-key",
            ProviderAuth::ChatgptSubscription => "chatgpt-subscription",
        };
        println!(
            "{} granted={} auth={} model={} base={}",
            record.id, record.granted, auth, record.default_model, record.base_url
        );
    }
    Ok(())
}

pub fn remove(state: &Path, id: &str) -> Result<(), String> {
    ProviderStore::new(state)
        .remove(id)
        .map_err(|err| err.to_string())?;
    println!("removed provider {id}");
    Ok(())
}

pub fn grant(state: &Path, id: &str, enabled: bool) -> Result<(), String> {
    let record = ProviderStore::new(state)
        .set_granted(id, enabled)
        .map_err(|err| err.to_string())?;
    if enabled {
        println!("granted provider {}", record.id);
        println!("also set local_only: false, add provider.use:{} to the caller, and install the egress drop-in in docs/providers.md", record.id);
    } else {
        println!("revoked provider {}", record.id);
    }
    Ok(())
}

pub fn test(
    state: &Path,
    id: &str,
    base_url: Option<&str>,
    allow_untrusted_host: bool,
) -> Result<(), String> {
    let line = ProviderStore::new(state)
        .probe(id, base_url, allow_untrusted_host)
        .map_err(|err| err.to_string())?;
    println!("{line}");
    Ok(())
}

pub fn login(state: &Path, id: &str) -> Result<(), String> {
    provider::subscription_available(id).map_err(|err| err.to_string())?;
    let store = ProviderStore::new(state);
    let endpoints = siwc::fetch_discovery(siwc::DISCOVERY_URL, Duration::from_secs(20))
        .map_err(|err| err.to_string())?;
    if endpoints.issuer != "https://auth.openai.com" {
        return Err("discovery issuer was not https://auth.openai.com".into());
    }
    let host_id = store.host_id().map_err(|err| err.to_string())?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|err| err.to_string())?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/auth/callback");
    let saved = store.openai_reauth_hint();
    let issued = saved.as_ref().map(|hint| hint.client_id.clone());
    let hint = saved.as_ref().map(|hint| hint.id_token.clone());
    let email = saved.as_ref().and_then(|hint| hint.email.clone());
    let request = siwc::begin_authorization(
        &endpoints,
        &redirect,
        &host_id,
        issued.as_deref(),
        hint.as_deref(),
        email.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    if hint.is_some() {
        println!("reauthorization started; the authorize URL contains an id token hint and is not printed");
    } else {
        println!("open this URL and approve DARKJUANDER:");
        println!("{}", siwc::redact_authorize_url(&request.url));
    }
    let _ = Command::new("xdg-open").arg(&request.url).spawn();
    let callback =
        siwc::accept_loopback(listener, Duration::from_secs(180)).map_err(|err| err.to_string())?;
    if callback.state != request.state {
        return Err("sign-in state did not match; refused the callback".into());
    }
    if callback.error.as_deref() == Some("access_denied") {
        return Err("sign-in was denied; no token was stored".into());
    }
    if let Some(error) = &callback.error {
        return Err(format!("sign-in failed closed: {error}"));
    }
    let client_id = reauth_client_id(issued.as_deref(), callback.client_id.as_deref())?;
    let expected_subject = reauth_subject(saved.as_ref().map(|hint| hint.subject.as_str()))?;
    let body = siwc::exchange_code(
        &endpoints,
        &request.redirect_uri,
        &client_id,
        &callback.code,
        &request.verifier,
        Duration::from_secs(20),
    )
    .map_err(|err| err.to_string())?;
    let jwks = siwc::fetch_jwks(&endpoints.jwks_uri, Duration::from_secs(20))
        .map_err(|err| err.to_string())?;
    let id_token = body
        .get("id_token")
        .and_then(|value| value.as_str())
        .ok_or("token response has no id token")?;
    let identity = siwc::validate_id_token(
        id_token,
        &jwks,
        &IdCheck {
            issuer: &endpoints.issuer,
            audience: &client_id,
            nonce: Some(&request.nonce),
            subject: expected_subject,
        },
    )
    .map_err(|err| err.to_string())?;
    let subscription = siwc::credential_from_exchange(
        &endpoints,
        &host_id,
        &client_id,
        &identity,
        &body,
        callback.scope.as_deref(),
    )
    .map_err(|err| err.to_string())?;
    let record = store
        .save_subscription(subscription)
        .map_err(|err| err.to_string())?;
    println!(
        "stored ChatGPT subscription credential for openai (granted: {})",
        record.granted
    );
    println!("routing stays off until: dj provider grant openai");
    println!("set the model id from the signed-in account: dj provider model openai MODEL");
    Ok(())
}

pub fn model(state: &Path, id: &str, model_id: &str) -> Result<(), String> {
    ProviderStore::new(state)
        .set_model(id, model_id)
        .map_err(|err| err.to_string())?;
    println!("provider {id} model {model_id}");
    Ok(())
}

pub(crate) fn reauth_subject(stored: Option<&str>) -> Result<Option<&str>, String> {
    match stored {
        Some("") => Err("stored subscription has no subject; refusing to replace it".into()),
        Some(subject) => Ok(Some(subject)),
        None => Ok(None),
    }
}

/// Returning sign-in must keep the issued client id. A different id is refused.
pub(crate) fn reauth_client_id(
    stored: Option<&str>,
    returned: Option<&str>,
) -> Result<String, String> {
    match (stored, returned) {
        (Some(stored), Some(returned)) if returned != stored => {
            Err("sign-in client id did not match the stored registration".into())
        }
        (_, Some(returned)) if returned == siwc::DYNAMIC_CLIENT || returned.is_empty() => {
            Err("sign-in did not return an issued client id".into())
        }
        (Some(stored), None) => Ok(stored.to_string()),
        (_, Some(returned)) => Ok(returned.to_string()),
        (None, None) => Err("sign-in did not return an issued client id".into()),
    }
}

fn credential_path(state: &Path, id: &str) -> String {
    state
        .join("credentials")
        .join(format!("{id}.key"))
        .display()
        .to_string()
}

fn print_yaml_hint(id: &str, model: &str) {
    println!("point umbrad at the registry and the model. Example:");
    println!("provider_registry: /var/lib/darkjuander/providers/registry.yaml");
    println!("local_only: false");
    println!("providers:");
    println!("  {id}:");
    println!("    granted: true");
    println!("models:");
    println!("  {model}:");
    println!("    engine: provider-{id}");
    println!("    location: cloud");
    println!("    size: large");
    println!("    memory_mb: 0");
    println!("    provider: {id}");
    let _ = std::io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reauth_rejects_a_different_client_id_and_keeps_the_subject() {
        let err = reauth_client_id(Some("oaiapp_stored"), Some("oaiapp_other")).unwrap_err();
        assert!(err.contains("did not match"));
        assert_eq!(
            reauth_client_id(Some("oaiapp_stored"), None).unwrap(),
            "oaiapp_stored"
        );
        assert_eq!(
            reauth_client_id(Some("oaiapp_stored"), Some("oaiapp_stored")).unwrap(),
            "oaiapp_stored"
        );
        assert!(reauth_client_id(Some("oaiapp_stored"), Some("dynamic_agent_client")).is_err());
        assert_eq!(
            reauth_client_id(None, Some("oaiapp_new")).unwrap(),
            "oaiapp_new"
        );
        assert_eq!(reauth_subject(Some("user-1")).unwrap(), Some("user-1"));
        assert_eq!(reauth_subject(None).unwrap(), None);
        assert!(reauth_subject(Some("")).is_err());
    }
}
