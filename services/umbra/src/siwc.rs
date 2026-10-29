//! Sign in with ChatGPT for a locally hosted open-source client.
//!
//! This follows the documented public-client flow: authorization code plus
//! PKCE, loopback redirect, ID-token checks against the issuer JWKS, and
//! Responses API scope `chatgpt.tokens.use.direct`. It does not scrape a
//! browser session. xAI has no equivalent in its developer docs; that path
//! stays unimplemented.

#![forbid(unsafe_code)]

use crate::credential::CredentialError;
use fs2::FileExt;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use ureq::AgentBuilder;

pub const DISCOVERY_URL: &str = "https://auth.openai.com/.well-known/openid-configuration";
pub const RESOURCE: &str = "https://api.openai.com/v1";
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
pub const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
pub const AGENT_NAME: &str = "DARKJUANDER";

pub const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub revocation_endpoint: String,
    pub jwks_uri: String,
}

impl Endpoints {
    pub fn documented() -> Self {
        Self {
            issuer: "https://auth.openai.com".into(),
            authorization_endpoint: "https://auth.openai.com/api/accounts/authorize".into(),
            token_endpoint: "https://auth.openai.com/api/accounts/oauth/token".into(),
            revocation_endpoint: "https://auth.openai.com/api/accounts/oauth/revoke".into(),
            jwks_uri: "https://auth.openai.com/.well-known/jwks.json".into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SiwcError {
    #[error("sign-in failed closed: {0}")]
    Closed(String),
    #[error("sign-in io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Credential(#[from] CredentialError),
}

#[derive(Clone, Debug)]
pub struct AuthorizeRequest {
    pub url: String,
    pub redirect_uri: String,
    pub state: String,
    pub nonce: String,
    pub verifier: String,
    pub host_id: String,
    pub client_id: String,
}

#[derive(Clone, Debug)]
pub struct Callback {
    pub code: String,
    pub state: String,
    pub client_id: Option<String>,
    pub error: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscriptionCredential {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
    pub client_id: String,
    pub ext_agent_host_id: String,
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_at: u64,
    pub scopes: Vec<String>,
    pub token_endpoint: String,
    pub revocation_endpoint: String,
    pub resource: String,
}

impl SubscriptionCredential {
    pub fn needs_refresh(&self, within_secs: u64) -> bool {
        let now = unix_now();
        self.expires_at <= now.saturating_add(within_secs)
    }
}

#[derive(Debug)]
pub struct IdCheck<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    pub nonce: Option<&'a str>,
    pub subject: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct IdClaims {
    sub: String,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

pub struct Identity {
    pub subject: String,
    pub email: Option<String>,
}

pub fn fetch_discovery(url: &str, timeout: Duration) -> Result<Endpoints, SiwcError> {
    let body = http_get(url, timeout)?;
    let value: Value = serde_json::from_str(&body)
        .map_err(|_| SiwcError::Closed("discovery document was not json".into()))?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| SiwcError::Closed(format!("discovery document has no {name}")))
    };
    Ok(Endpoints {
        issuer: field("issuer")?,
        authorization_endpoint: field("authorization_endpoint")?,
        token_endpoint: field("token_endpoint")?,
        revocation_endpoint: field("revocation_endpoint")?,
        jwks_uri: field("jwks_uri")?,
    })
}

pub fn begin_authorization(
    endpoints: &Endpoints,
    redirect_uri: &str,
    host_id: &str,
    issued_client_id: Option<&str>,
    id_token_hint: Option<&str>,
    login_hint: Option<&str>,
) -> Result<AuthorizeRequest, SiwcError> {
    if !redirect_uri.starts_with("http://127.0.0.1:") || !redirect_uri.ends_with("/auth/callback") {
        return Err(SiwcError::Closed(
            "redirect_uri must be http://127.0.0.1:<port>/auth/callback".into(),
        ));
    }
    if !host_id.starts_with("urn:uuid:")
        && !host_id.starts_with("urn:ietf:params:oauth:jwk-thumbprint:")
    {
        return Err(SiwcError::Closed(
            "host id must be a urn:uuid or jwk-thumbprint uri".into(),
        ));
    }
    let verifier = random_verifier()?;
    let challenge = pkce_challenge(&verifier);
    let state = random_token()?;
    let nonce = random_token()?;
    let client_id = issued_client_id.unwrap_or(DYNAMIC_CLIENT).to_string();
    let mut pairs = vec![
        ("client_id", client_id.clone()),
        ("response_type", "code".into()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", SCOPES.into()),
        ("resource", RESOURCE.into()),
        ("state", state.clone()),
        ("nonce", nonce.clone()),
        ("code_challenge_method", "S256".into()),
        ("code_challenge", challenge),
        ("ext_agent_host_id", host_id.to_string()),
    ];
    if client_id == DYNAMIC_CLIENT {
        pairs.push(("agent_name_hint", AGENT_NAME.into()));
    }
    if let Some(hint) = id_token_hint {
        pairs.push(("id_token_hint", hint.to_string()));
    }
    if let Some(hint) = login_hint {
        pairs.push(("login_hint", hint.to_string()));
    }
    let query = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(&value)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(AuthorizeRequest {
        url: format!("{}?{query}", endpoints.authorization_endpoint),
        redirect_uri: redirect_uri.to_string(),
        state,
        nonce,
        verifier,
        host_id: host_id.to_string(),
        client_id,
    })
}

/// Drop `id_token_hint` before anything prints the authorize URL.
pub fn redact_authorize_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let kept = query
        .split('&')
        .filter(|pair| !pair.starts_with("id_token_hint="))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{kept}")
}

pub fn parse_callback(query: &str) -> Result<Callback, SiwcError> {
    let mut code = None;
    let mut state = None;
    let mut client_id = None;
    let mut error = None;
    let mut scope = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value).map_err(SiwcError::Closed)?;
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "client_id" => client_id = Some(value),
            "error" => error = Some(value),
            "scope" => scope = Some(value),
            _ => {}
        }
    }
    let state = state.ok_or_else(|| SiwcError::Closed("callback has no state".into()))?;
    Ok(Callback {
        code: code.unwrap_or_default(),
        state,
        client_id,
        error,
        scope,
    })
}

pub fn accept_loopback(listener: TcpListener, timeout: Duration) -> Result<Callback, SiwcError> {
    listener.set_nonblocking(true)?;
    let started = Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                if started.elapsed() > timeout {
                    return Err(SiwcError::Closed("loopback callback timed out".into()));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => return Err(err.into()),
        }
    };
    stream.set_nonblocking(false)?;
    let _ = stream.set_read_timeout(Some(timeout));
    let mut buf = [0u8; 4096];
    let read = stream.read(&mut buf).unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..read]);
    let query = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| path.split_once('?').map(|(_, query)| query))
        .ok_or_else(|| SiwcError::Closed("loopback request was not a callback".into()))?;
    let callback = parse_callback(query)?;
    let page = "DARKJUANDER received the sign-in. You can close this tab.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.write_all(response.as_bytes());
    Ok(callback)
}

pub fn exchange_code(
    endpoints: &Endpoints,
    redirect_uri: &str,
    client_id: &str,
    code: &str,
    verifier: &str,
    timeout: Duration,
) -> Result<Value, SiwcError> {
    if client_id == DYNAMIC_CLIENT || client_id.is_empty() {
        return Err(SiwcError::Closed(
            "token exchange needs the issued client id, not dynamic_agent_client".into(),
        ));
    }
    form_post(
        &endpoints.token_endpoint,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("resource", RESOURCE),
        ],
        timeout,
    )
}

pub fn refresh_tokens(
    record: &SubscriptionCredential,
    timeout: Duration,
) -> Result<Value, SiwcError> {
    if record.client_id == DYNAMIC_CLIENT || record.refresh_token.is_empty() {
        return Err(SiwcError::Closed(
            "refresh needs the issued client id and a refresh token".into(),
        ));
    }
    form_post(
        &record.token_endpoint,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", &record.client_id),
            ("refresh_token", &record.refresh_token),
            ("resource", RESOURCE),
        ],
        timeout,
    )
}

pub fn refresh_file(
    path: &Path,
    record: &mut SubscriptionCredential,
    timeout: Duration,
) -> Result<(), SiwcError> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    file.lock_exclusive()
        .map_err(|err| SiwcError::Closed(format!("credential lock: {err}")))?;
    let value = refresh_tokens(record, timeout)?;
    apply_token_response(record, &value, None)?;
    let bytes = serde_json::to_vec(record)
        .map_err(|err| SiwcError::Closed(format!("could not store refreshed tokens: {err}")))?;
    use std::io::{Seek, SeekFrom, Write};
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub fn revoke(record: &SubscriptionCredential, timeout: Duration) -> Result<(), SiwcError> {
    let _ = form_post(
        &record.revocation_endpoint,
        &[
            ("token", &record.refresh_token),
            ("token_type_hint", "refresh_token"),
            ("client_id", &record.client_id),
        ],
        timeout,
    )?;
    Ok(())
}

pub fn fetch_jwks(url: &str, timeout: Duration) -> Result<JwkSet, SiwcError> {
    let body = http_get(url, timeout)?;
    serde_json::from_str(&body).map_err(|_| SiwcError::Closed("jwks document was not valid".into()))
}

pub fn validate_id_token(
    token: &str,
    jwks: &JwkSet,
    check: &IdCheck<'_>,
) -> Result<Identity, SiwcError> {
    let header = decode_header(token)
        .map_err(|_| SiwcError::Closed("id token header is not a jwt".into()))?;
    if header.alg != Algorithm::RS256 {
        return Err(SiwcError::Closed("id token must use RS256".into()));
    }
    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| SiwcError::Closed("id token has no kid".into()))?;
    let jwk = jwks
        .find(kid)
        .ok_or_else(|| SiwcError::Closed("id token kid is not in the jwks".into()))?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|_| SiwcError::Closed("jwks key could not be used".into()))?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[check.issuer]);
    validation.set_audience(&[check.audience]);
    validation.leeway = 5;
    validation.validate_nbf = false;
    let data = decode::<IdClaims>(token, &key, &validation)
        .map_err(|_| SiwcError::Closed("id token signature or claims were rejected".into()))?;
    if data.claims.sub.is_empty() {
        return Err(SiwcError::Closed("id token has no subject".into()));
    }
    if let Some(nonce) = check.nonce {
        if data.claims.nonce.as_deref() != Some(nonce) {
            return Err(SiwcError::Closed("id token nonce did not match".into()));
        }
    }
    if let Some(subject) = check.subject {
        if data.claims.sub != subject {
            return Err(SiwcError::Closed("id token subject changed".into()));
        }
    }
    Ok(Identity {
        subject: data.claims.sub,
        email: data.claims.email,
    })
}

pub fn credential_from_exchange(
    endpoints: &Endpoints,
    host_id: &str,
    client_id: &str,
    identity: &Identity,
    body: &Value,
    scope_hint: Option<&str>,
) -> Result<SubscriptionCredential, SiwcError> {
    let mut record = SubscriptionCredential {
        issuer: endpoints.issuer.clone(),
        subject: identity.subject.clone(),
        email: identity.email.clone(),
        client_id: client_id.to_string(),
        ext_agent_host_id: host_id.to_string(),
        id_token: String::new(),
        access_token: String::new(),
        refresh_token: String::new(),
        token_type: "Bearer".into(),
        expires_at: unix_now(),
        scopes: split_scopes(scope_hint.unwrap_or("")),
        token_endpoint: endpoints.token_endpoint.clone(),
        revocation_endpoint: endpoints.revocation_endpoint.clone(),
        resource: RESOURCE.into(),
    };
    apply_token_response(&mut record, body, scope_hint)?;
    if !record.scopes.iter().any(|scope| scope == PLAN_SCOPE) {
        return Err(SiwcError::Closed(
            "ChatGPT plan scope chatgpt.tokens.use.direct was not granted".into(),
        ));
    }
    Ok(record)
}

fn apply_token_response(
    record: &mut SubscriptionCredential,
    body: &Value,
    scope_hint: Option<&str>,
) -> Result<(), SiwcError> {
    let access = body
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SiwcError::Closed("token response has no access_token".into()))?;
    let refresh = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| record.refresh_token.clone());
    if refresh.is_empty() {
        return Err(SiwcError::Closed(
            "token response has no refresh_token".into(),
        ));
    }
    let expires_in = body
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    if let Some(id_token) = body.get("id_token").and_then(Value::as_str) {
        record.id_token = id_token.to_string();
    }
    if record.id_token.is_empty() {
        return Err(SiwcError::Closed("token response has no id_token".into()));
    }
    record.access_token = access.to_string();
    record.refresh_token = refresh;
    record.expires_at = unix_now().saturating_add(expires_in);
    if let Some(scope) = body.get("scope").and_then(Value::as_str) {
        record.scopes = split_scopes(scope);
    } else if let Some(scope) = scope_hint {
        if record.scopes.is_empty() {
            record.scopes = split_scopes(scope);
        }
    }
    Ok(())
}

fn split_scopes(scope: &str) -> Vec<String> {
    scope
        .split([' ', '+'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn form_post(url: &str, fields: &[(&str, &str)], timeout: Duration) -> Result<Value, SiwcError> {
    crate::engine::remote::check_base_url(url).map_err(SiwcError::Closed)?;
    let body = fields
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let agent = AgentBuilder::new()
        .redirects(0)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let response = agent
        .post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&body)
        .map_err(|_| SiwcError::Closed("token endpoint request failed".into()))?;
    let text = response
        .into_string()
        .map_err(|_| SiwcError::Closed("token endpoint response was unreadable".into()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| SiwcError::Closed("token endpoint response was not json".into()))?;
    if value.get("error").is_some() {
        let code = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("error");
        return Err(SiwcError::Closed(format!("token endpoint returned {code}")));
    }
    Ok(value)
}

fn http_get(url: &str, timeout: Duration) -> Result<String, SiwcError> {
    crate::engine::remote::check_base_url(url).map_err(SiwcError::Closed)?;
    let agent = AgentBuilder::new()
        .redirects(0)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|_| SiwcError::Closed(format!("GET {url} failed")))?;
    response
        .into_string()
        .map_err(|_| SiwcError::Closed("response body was unreadable".into()))
}

pub fn pkce_challenge(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

fn random_verifier() -> Result<String, SiwcError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| SiwcError::Closed("entropy failed".into()))?;
    Ok(base64url(&bytes))
}

fn random_token() -> Result<String, SiwcError> {
    random_verifier()
}

pub fn new_host_id() -> Result<String, SiwcError> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|_| SiwcError::Closed("entropy failed".into()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| "bad percent encoding".to_string())?;
            let value =
                u8::from_str_radix(hex, 16).map_err(|_| "bad percent encoding".to_string())?;
            out.push(value);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "callback was not utf-8".to_string())
}

fn base64url(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = u32::from(bytes[index]) << 16
            | u32::from(bytes[index + 1]) << 8
            | u32::from(bytes[index + 2]);
        out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
        out.push(TABLE[((chunk >> 6) & 63) as usize] as char);
        out.push(TABLE[(chunk & 63) as usize] as char);
        index += 3;
    }
    if index < bytes.len() {
        let mut chunk = u32::from(bytes[index]) << 16;
        out.push(TABLE[((chunk >> 18) & 63) as usize] as char);
        if index + 1 < bytes.len() {
            chunk |= u32::from(bytes[index + 1]) << 8;
            out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
            out.push(TABLE[((chunk >> 6) & 63) as usize] as char);
        } else {
            out.push(TABLE[((chunk >> 12) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::json;
    use std::io::Write;
    use std::net::TcpStream;
    use std::process::Command;
    use std::thread;

    #[test]
    fn authorize_url_uses_pkce_and_hides_the_verifier() {
        let endpoints = Endpoints::documented();
        let request = begin_authorization(
            &endpoints,
            "http://127.0.0.1:1455/auth/callback",
            "urn:uuid:11111111-1111-4111-8111-111111111111",
            None,
            Some("id-token-CANARY"),
            None,
        )
        .unwrap();
        assert!(request.url.contains("code_challenge_method=S256"));
        assert!(request.url.contains("client_id=dynamic_agent_client"));
        assert!(request.url.contains("agent_name_hint=DARKJUANDER"));
        assert!(request.url.contains(&pkce_challenge(&request.verifier)));
        assert!(!request.url.contains(&request.verifier));
        assert!(request.url.contains("id_token_hint="));
        let printed = redact_authorize_url(&request.url);
        assert!(!printed.contains("id-token-CANARY"));
        assert!(!printed.contains("id_token_hint="));
    }

    #[test]
    fn access_denied_does_not_yield_a_code() {
        let callback = parse_callback("error=access_denied&state=abc").unwrap();
        assert_eq!(callback.error.as_deref(), Some("access_denied"));
        assert!(callback.code.is_empty());
    }

    #[test]
    fn loopback_exchange_and_id_token_round_trip() {
        let (pem, jwks) = rsa_fixture();
        let header = Header {
            kid: Some("test-key".into()),
            alg: Algorithm::RS256,
            ..Header::default()
        };
        let exp = unix_now() + 3600;
        let token = encode(
            &header,
            &json!({
                "iss": "https://auth.openai.com",
                "sub": "user-1",
                "aud": "oaiapp_test",
                "exp": exp,
                "iat": unix_now(),
                "nonce": "nonce-1",
                "email": "user@example.com"
            }),
            &EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let token_server = TcpListener::bind("127.0.0.1:0").unwrap();
        let token_port = token_server.local_addr().unwrap().port();
        thread::spawn(move || {
            let (mut stream, _) = token_server.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let body = format!(
                r#"{{"access_token":"access-CANARY","refresh_token":"refresh-CANARY","id_token":"{token}","token_type":"Bearer","expires_in":3600,"scope":"openid chatgpt.tokens.use.direct"}}"#
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        });
        let endpoints = Endpoints {
            issuer: "https://auth.openai.com".into(),
            authorization_endpoint: "https://auth.openai.com/api/accounts/authorize".into(),
            token_endpoint: format!("http://127.0.0.1:{token_port}/token"),
            revocation_endpoint: format!("http://127.0.0.1:{token_port}/revoke"),
            jwks_uri: format!("http://127.0.0.1:{token_port}/jwks"),
        };
        let started = begin_authorization(
            &endpoints,
            &format!("http://127.0.0.1:{port}/auth/callback"),
            "urn:uuid:11111111-1111-4111-8111-111111111111",
            None,
            None,
            None,
        )
        .unwrap();
        let state = started.state.clone();
        thread::spawn(move || {
            let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let req = format!(
                "GET /auth/callback?code=auth-code&state={state}&client_id=oaiapp_test HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
            );
            client.write_all(req.as_bytes()).unwrap();
        });
        let callback = accept_loopback(listener, Duration::from_secs(2)).unwrap();
        assert_eq!(callback.state, started.state);
        assert_eq!(callback.client_id.as_deref(), Some("oaiapp_test"));
        let body = exchange_code(
            &endpoints,
            &started.redirect_uri,
            "oaiapp_test",
            &callback.code,
            &started.verifier,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(!body.to_string().contains("code_verifier"));
        let set: JwkSet = serde_json::from_str(&jwks).unwrap();
        let identity = validate_id_token(
            body["id_token"].as_str().unwrap(),
            &set,
            &IdCheck {
                issuer: "https://auth.openai.com",
                audience: "oaiapp_test",
                nonce: Some("nonce-1"),
                subject: None,
            },
        )
        .unwrap();
        assert_eq!(identity.subject, "user-1");
        let record = credential_from_exchange(
            &endpoints,
            &started.host_id,
            "oaiapp_test",
            &identity,
            &body,
            None,
        )
        .unwrap();
        assert!(record.scopes.iter().any(|scope| scope == PLAN_SCOPE));
        assert_eq!(record.access_token, "access-CANARY");
        let wrong = validate_id_token(
            body["id_token"].as_str().unwrap(),
            &set,
            &IdCheck {
                issuer: "https://auth.openai.com",
                audience: "oaiapp_test",
                nonce: Some("other"),
                subject: None,
            },
        );
        assert!(wrong.is_err());
    }

    fn rsa_fixture() -> (String, String) {
        let dir = tempfile::tempdir().unwrap();
        let pem_path = dir.path().join("key.pem");
        let status = Command::new("openssl")
            .args(["genrsa", "-out"])
            .arg(&pem_path)
            .arg("2048")
            .output()
            .expect("openssl genrsa");
        assert!(status.status.success(), "openssl genrsa failed");
        let pem = std::fs::read_to_string(&pem_path).unwrap();
        let text = Command::new("openssl")
            .args(["rsa", "-in"])
            .arg(&pem_path)
            .args(["-text", "-noout"])
            .output()
            .unwrap();
        assert!(text.status.success());
        let text = String::from_utf8(text.stdout).unwrap();
        let modulus = text
            .lines()
            .skip_while(|line| !line.contains("modulus"))
            .skip(1)
            .take_while(|line| {
                line.trim()
                    .starts_with(|ch: char| ch.is_ascii_hexdigit() || ch == ':')
            })
            .collect::<String>();
        let hexmod: String = modulus
            .chars()
            .filter(|ch| ch.is_ascii_hexdigit())
            .collect();
        let mut raw = hex::decode(hexmod).unwrap();
        if raw.first() == Some(&0) {
            raw.remove(0);
        }
        let n = base64url(&raw);
        let jwks = format!(
            r#"{{"keys":[{{"kty":"RSA","kid":"test-key","alg":"RS256","use":"sig","n":"{n}","e":"AQAB"}}]}}"#
        );
        (pem, jwks)
    }
}
