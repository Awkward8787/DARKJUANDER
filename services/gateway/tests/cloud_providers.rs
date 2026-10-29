//! Private and secret labels must not reach a cloud provider, even with a grant.
//! The HTTP server is in-process. No real provider is contacted.

use dj_policy::DEFAULT_POLICY;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn private_labels_never_reach_xai_or_openai() {
    let hits = Arc::new(AtomicU64::new(0));
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits_bg = hits.clone();
    let bodies_bg = bodies.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = Vec::new();
            let mut tmp = [0u8; 2048];
            loop {
                let read = stream.read(&mut tmp).unwrap_or(0);
                if read == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..read]);
                if buf.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = buf
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap_or(buf.len());
            let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
            let length = header.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            });
            if let Some(length) = length {
                while buf.len() < header_end + 4 + length {
                    let read = stream.read(&mut tmp).unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..read]);
                }
                let start = header_end + 4;
                let end = (start + length).min(buf.len());
                bodies_bg
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[start..end]).to_string());
            }
            hits_bg.fetch_add(1, Ordering::SeqCst);
            let payload = r#"{"choices":[{"message":{"role":"assistant","content":"pong"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let dir = tempfile::tempdir().unwrap();
    let xai_key = b"sk-test-xai-CANARYKEY\n";
    let openai_key = b"sk-test-openai-CANARYKEY\n";
    install_key(&dir.path().join("xai.key"), xai_key);
    install_key(&dir.path().join("openai.key"), openai_key);
    let signer = dj_audit::spawn_test_signer().unwrap();
    let socket = dir.path().join("dj.sock");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(&policy, DEFAULT_POLICY).unwrap();
    write_idle(&dir.path().join("memory"));
    write_idle(&dir.path().join("io"));
    let cfg = config_text(dir.path(), &socket, &policy, &signer.paths, port, true);
    std::fs::write(dir.path().join("umbrad.yaml"), &cfg).unwrap();
    let loaded = dj_gateway::FileConfig::from_path(&dir.path().join("umbrad.yaml")).unwrap();
    let state = dj_gateway::AppState::from_config(loaded).unwrap();
    let audit = state.audit_path();
    let bound = socket.clone();
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let _ = dj_gateway::serve(state, &bound).await;
        });
    });
    wait_socket(&socket);

    let private_labels = json!({
        "integrity": "trusted",
        "confidentiality": "private",
        "origin": "user",
        "owner": "agent"
    });
    let secret_labels = json!({
        "integrity": "trusted",
        "confidentiality": "secret",
        "origin": "user",
        "owner": "agent"
    });
    let public = chat(
        &socket,
        "token-public",
        "grok-test",
        "hello-public",
        json!({
            "integrity": "trusted",
            "confidentiality": "public",
            "origin": "user",
            "owner": "agent"
        }),
    )
    .unwrap();
    assert_eq!(public["model"], "grok-test");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let openai = chat(
        &socket,
        "token-public",
        "gpt-test",
        "hello-public",
        json!({
            "integrity": "trusted",
            "confidentiality": "public",
            "origin": "user",
            "owner": "agent"
        }),
    )
    .unwrap();
    assert_eq!(openai["model"], "gpt-test");
    assert_eq!(hits.load(Ordering::SeqCst), 2);

    let alias = chat(
        &socket,
        "token-private",
        "default-chat",
        "PRIVATE_CANARY_DO_NOT_SEND",
        private_labels,
    )
    .unwrap();
    assert_eq!(alias["model"], "mid-local");
    assert_eq!(alias["dj"]["route"]["location"], "local");
    assert_ne!(alias["dj"]["route"]["scorer_preferred"], "mid-local");
    let direct = chat(
        &socket,
        "token-private",
        "grok-test",
        "PRIVATE_CANARY_DO_NOT_SEND",
        secret_labels.clone(),
    );
    assert!(direct.is_err(), "{direct:?}");
    let other = chat(
        &socket,
        "token-private",
        "gpt-test",
        "PRIVATE_CANARY_DO_NOT_SEND",
        secret_labels,
    );
    assert!(other.is_err(), "{other:?}");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "private calls reached a provider"
    );
    let seen = bodies.lock().unwrap().join("\n");
    assert!(seen.contains("hello-public"));
    assert!(!seen.contains("PRIVATE_CANARY_DO_NOT_SEND"));
    assert!(!seen.contains("CANARYKEY"));

    let ungranted = chat(
        &socket,
        "token-model-only",
        "grok-test",
        "hello-public",
        json!({
            "integrity": "trusted",
            "confidentiality": "public",
            "origin": "user",
            "owner": "agent"
        }),
    );
    assert!(ungranted.is_err(), "{ungranted:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 2);

    let audit_text = std::fs::read_to_string(&audit).unwrap();
    assert!(audit_text.contains("\"provider\":\"xai\"") || audit_text.contains("xai"));
    assert!(!audit_text.contains("CANARYKEY"));
    assert!(!audit_text.contains("PRIVATE_CANARY_DO_NOT_SEND"));
    assert!(!audit_text.contains("hello-public"));
    let _ = signer;
}

#[test]
fn loosened_policy_still_blocks_a_granted_provider() {
    let hits = Arc::new(AtomicU64::new(0));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits_bg = hits.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            hits_bg.fetch_add(1, Ordering::SeqCst);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        }
    });
    let dir = tempfile::tempdir().unwrap();
    install_key(&dir.path().join("xai.key"), b"sk-test-xai-CANARYKEY\n");
    install_key(
        &dir.path().join("openai.key"),
        b"sk-test-openai-CANARYKEY\n",
    );
    let signer = dj_audit::spawn_test_signer().unwrap();
    let policy = dir.path().join("policy.yaml");
    std::fs::write(
        &policy,
        "version: 1\ndefault_decision: allow\nrules:\n  - id: allow-models\n    match:\n      action: model.use\n    decision: allow\n    reason: loosened\n",
    )
    .unwrap();
    write_idle(&dir.path().join("memory"));
    write_idle(&dir.path().join("io"));
    let socket = dir.path().join("dj.sock");
    let cfg = config_text(dir.path(), &socket, &policy, &signer.paths, port, true);
    std::fs::write(dir.path().join("umbrad.yaml"), cfg).unwrap();
    let state = dj_gateway::AppState::from_config(
        dj_gateway::FileConfig::from_path(&dir.path().join("umbrad.yaml")).unwrap(),
    )
    .unwrap();
    let bound = socket.clone();
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let _ = dj_gateway::serve(state, &bound).await;
        });
    });
    wait_socket(&socket);
    let err = chat(
        &socket,
        "token-granted",
        "gpt-test",
        "PRIVATE_CANARY_DO_NOT_SEND",
        json!({
            "integrity": "trusted",
            "confidentiality": "private",
            "origin": "user",
            "owner": "agent"
        }),
    );
    assert!(err.is_err(), "{err:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    let _ = signer;
}

fn install_key(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn config_text(
    dir: &Path,
    socket: &Path,
    policy: &Path,
    audit: &dj_audit::AuditPaths,
    port: u16,
    granted: bool,
) -> String {
    format!(
        r#"
socket: {socket}
audit_path: {audit_log}
audit_signer_socket: {signer}
audit_verifying_key: {verifying}
policy_path: {policy}
local_only: false
prefer_cloud: true
psi_memory_some_avg10_threshold: 25.0
psi_memory_path: {memory}
psi_io_path: {io}
allow_psi_override: false
providers:
  xai:
    granted: {granted}
  openai:
    granted: {granted}
models:
  small-local:
    engine: mock
    location: local
    size: small
    memory_mb: 800
  mid-local:
    engine: mock
    location: local
    size: mid
    memory_mb: 800
  grok-test:
    engine: xai
    location: cloud
    size: large
    memory_mb: 0
    provider: xai
  gpt-test:
    engine: openai
    location: cloud
    size: large
    memory_mb: 0
    provider: openai
aliases:
  default-chat:
    small: small-local
    mid: mid-local
    cloud: grok-test
engines:
  mock:
    kind: mock
  xai:
    kind: openai-compatible
    provider: xai
    base_url: http://127.0.0.1:{port}/v1
    credential_file: {xai_key}
    timeout_secs: 2
    retries: 0
    backoff_ms: 1
  openai:
    kind: openai-compatible
    provider: openai
    base_url: http://127.0.0.1:{port}/v1
    credential_file: {openai_key}
    timeout_secs: 2
    retries: 0
    backoff_ms: 1
principals:
  - id: agent
    token: token-granted
    trust_domain: agent
    grants:
      - "model.use:default-chat"
      - "model.use:mid-local"
      - "model.use:small-local"
      - "model.use:grok-test"
      - "model.use:gpt-test"
      - "provider.use:xai"
      - "provider.use:openai"
  - id: public-agent
    token: token-public
    trust_domain: public-agent
    grants:
      - "model.use:grok-test"
      - "model.use:gpt-test"
      - "provider.use:xai"
      - "provider.use:openai"
  - id: private-agent
    token: token-private
    trust_domain: private-agent
    grants:
      - "model.use:default-chat"
      - "model.use:mid-local"
      - "model.use:small-local"
      - "model.use:grok-test"
      - "model.use:gpt-test"
      - "provider.use:xai"
      - "provider.use:openai"
  - id: model-only
    token: token-model-only
    trust_domain: model-only
    grants:
      - "model.use:grok-test"
      - "model.use:mid-local"
"#,
        socket = socket.display(),
        audit_log = dir.join("audit.log").display(),
        signer = audit.signer_socket.display(),
        verifying = audit.verifying_key.display(),
        policy = policy.display(),
        memory = dir.join("memory").display(),
        io = dir.join("io").display(),
        xai_key = dir.join("xai.key").display(),
        openai_key = dir.join("openai.key").display(),
    )
}

fn write_idle(path: &Path) {
    std::fs::write(
        path,
        "some avg10=0.00 avg60=0.00 avg300=0.00 total=0\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
    )
    .unwrap();
}

fn wait_socket(path: &Path) {
    for _ in 0..100 {
        if UnixStream::connect(path).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("socket missing");
}

fn chat(
    socket: &Path,
    token: &str,
    model: &str,
    content: &str,
    labels: Value,
) -> Result<Value, String> {
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": content, "dj_labels": labels}]
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    let (status, text) = unix_http(socket, "/v1/chat/completions", token, &bytes)?;
    if status >= 400 {
        return Err(format!("status {status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|err| format!("{err}: {text}"))
}

fn unix_http(socket: &Path, path: &str, token: &str, body: &[u8]) -> Result<(u16, String), String> {
    let mut stream = UnixStream::connect(socket).map_err(|err| err.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf);
    let (head, raw) = text.split_once("\r\n\r\n").ok_or("bad http")?;
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Ok((status, raw.to_string()))
}
