//! `dj-auditd` holds the ed25519 seed and signs checkpoint heads.
//!
//! `umbrad` only has the socket and the published verifying key. The signer
//! remembers the last head it signed and will not sign a shorter chain, so
//! truncating the log and asking for a new signature fails.
//!
//! This is not a TPM. Root, or anyone who can write the signer's state
//! directory, can still produce signatures. TPM sealing is tracked in the
//! roadmap.

use crate::{checkpoint_message, AuditError};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub const DEFAULT_SIGNER_SOCKET: &str = "/run/dj-audit/signer.sock";
pub const DEFAULT_VERIFYING_KEY: &str = "/var/lib/dj-audit/audit.ed25519.pub";
pub const DEFAULT_SIGNING_KEY: &str = "/var/lib/dj-audit/audit.ed25519";

#[derive(Clone, Debug)]
pub struct AuditPaths {
    pub signer_socket: PathBuf,
    pub verifying_key: PathBuf,
}

impl AuditPaths {
    pub fn system_defaults() -> Self {
        Self {
            signer_socket: PathBuf::from(DEFAULT_SIGNER_SOCKET),
            verifying_key: PathBuf::from(DEFAULT_VERIFYING_KEY),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SignerClient {
    socket: PathBuf,
}

impl SignerClient {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
        }
    }

    pub fn sign(&self, count: u64, head: &str) -> Result<Vec<u8>, AuditError> {
        let response = self.round_trip(&json!({
            "op": "sign",
            "count": count,
            "head": head,
        }))?;
        if let Some(err) = response.get("error").and_then(Value::as_str) {
            return Err(AuditError::Corrupt(err.to_string()));
        }
        let hex_sig = response
            .get("signature")
            .and_then(Value::as_str)
            .ok_or_else(|| AuditError::Corrupt("signer response has no signature".into()))?;
        hex::decode(hex_sig)
            .map_err(|err| AuditError::Corrupt(format!("signer signature is not hex: {err}")))
    }
}

impl SignerClient {
    fn round_trip(&self, request: &Value) -> Result<Value, AuditError> {
        let mut stream = UnixStream::connect(&self.socket).map_err(|err| {
            AuditError::Corrupt(format!(
                "cannot reach audit signer {}: {err}",
                self.socket.display()
            ))
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        stream.write_all(&serde_json::to_vec(request)?)?;
        stream.write_all(b"\n")?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        if line.is_empty() {
            return Err(AuditError::Corrupt("audit signer closed the socket".into()));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

struct SignedHead {
    count: u64,
    head: String,
}

pub fn serve(
    key_path: &Path,
    public_path: &Path,
    socket_path: &Path,
    shutdown: &AtomicBool,
) -> Result<(), AuditError> {
    let key = load_or_create_signing_key(key_path)?;
    publish_verifying_key(public_path, &key.verifying_key())?;
    let state_path = key_path.with_file_name("signed-head.json");
    let mut signed = read_signed_head(&state_path)?;
    if socket_path.exists() {
        let _ = fs::remove_file(socket_path);
    }
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let _ = fs::set_permissions(socket_path, fs::Permissions::from_mode(0o660));
    listener.set_nonblocking(true)?;
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = handle_client(&key, &mut signed, &state_path, stream);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(err.into()),
        }
    }
    let _ = fs::remove_file(socket_path);
    Ok(())
}

fn handle_client(
    key: &SigningKey,
    signed: &mut Option<SignedHead>,
    state_path: &Path,
    mut stream: UnixStream,
) -> Result<(), AuditError> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut line = String::new();
    BufReader::new(stream.try_clone()?).read_line(&mut line)?;
    let request: Value = serde_json::from_str(&line).unwrap_or(json!({}));
    let response = match request.get("op").and_then(Value::as_str) {
        Some("sign") => sign_request(key, signed, state_path, &request),
        _ => json!({"error": "unknown signer op"}),
    };
    stream.write_all(&serde_json::to_vec(&response)?)?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn sign_request(
    key: &SigningKey,
    signed: &mut Option<SignedHead>,
    state_path: &Path,
    request: &Value,
) -> Value {
    let Some(count) = request.get("count").and_then(Value::as_u64) else {
        return json!({"error": "signer request needs count"});
    };
    let Some(head) = request.get("head").and_then(Value::as_str) else {
        return json!({"error": "signer request needs head"});
    };
    if let Err(err) = authorize(signed, count, head) {
        return json!({"error": err});
    }
    let message = checkpoint_message(count, head);
    let signature = key.sign(message.as_bytes());
    if let Err(err) = persist_signed_head(
        state_path,
        &SignedHead {
            count,
            head: head.to_string(),
        },
    ) {
        return json!({"error": err.to_string()});
    }
    *signed = Some(SignedHead {
        count,
        head: head.to_string(),
    });
    json!({"signature": hex::encode(signature.to_bytes())})
}

fn authorize(signed: &Option<SignedHead>, count: u64, head: &str) -> Result<(), String> {
    match signed {
        None => Ok(()),
        Some(prev) if prev.count == count && prev.head == head => Ok(()),
        Some(prev) if count == prev.count + 1 => Ok(()),
        Some(prev) => Err(format!(
            "refusing to sign count {count}; signer is at {}",
            prev.count
        )),
    }
}

pub struct TestSigner {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub paths: AuditPaths,
    pub key_path: PathBuf,
}

pub fn spawn_test_signer() -> Result<TestSigner, AuditError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let key_dir = std::env::temp_dir().join(format!("dj-audit-keys-{}-{n}", std::process::id()));
    fs::create_dir_all(&key_dir)?;
    let _ = fs::set_permissions(&key_dir, fs::Permissions::from_mode(0o700));
    let key_path = key_dir.join("audit.ed25519");
    let public_path = key_dir.join("audit.ed25519.pub");
    let socket = PathBuf::from(format!("/tmp/djaudit-{}-{n}.sock", std::process::id()));
    if socket.exists() {
        let _ = fs::remove_file(&socket);
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    let flag = shutdown.clone();
    let key_thread = key_path.clone();
    let public_thread = public_path.clone();
    let socket_thread = socket.clone();
    let thread = std::thread::spawn(move || {
        let _ = serve(&key_thread, &public_thread, &socket_thread, &flag);
    });
    let paths = AuditPaths {
        signer_socket: socket,
        verifying_key: public_path,
    };
    wait_until_reachable(&paths.signer_socket)?;
    Ok(TestSigner {
        shutdown,
        thread: Some(thread),
        paths,
        key_path,
    })
}

fn wait_until_reachable(socket: &Path) -> Result<(), AuditError> {
    for _ in 0..100 {
        if UnixStream::connect(socket).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(AuditError::Corrupt(format!(
        "audit signer did not appear at {}",
        socket.display()
    )))
}

impl Drop for TestSigner {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.paths.signer_socket);
    }
}

fn load_or_create_signing_key(path: &Path) -> Result<SigningKey, AuditError> {
    if path.exists() {
        let bytes = fs::read(path)?;
        if bytes.len() != 32 {
            return Err(AuditError::Corrupt(format!(
                "signing key {} must be 32 bytes",
                path.display()
            )));
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(SigningKey::from_bytes(&seed));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o751));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|err| AuditError::Corrupt(format!("could not generate an audit key: {err}")))?;
    let mut file = File::create(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&seed)?;
    file.sync_all()?;
    Ok(SigningKey::from_bytes(&seed))
}

fn publish_verifying_key(path: &Path, key: &VerifyingKey) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o644))?;
    file.write_all(&key.to_bytes())?;
    file.sync_all()?;
    Ok(())
}

pub fn load_verifying_key(path: &Path) -> Result<VerifyingKey, AuditError> {
    let bytes = fs::read(path).map_err(|err| {
        AuditError::Corrupt(format!(
            "cannot read verifying key {}: {err}",
            path.display()
        ))
    })?;
    if bytes.len() != 32 {
        return Err(AuditError::Corrupt(format!(
            "verifying key {} must be 32 bytes",
            path.display()
        )));
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&raw)
        .map_err(|err| AuditError::Corrupt(format!("verifying key is invalid: {err}")))
}

fn read_signed_head(path: &Path) -> Result<Option<SignedHead>, AuditError> {
    if !path.exists() {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let count = value
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| AuditError::Corrupt("signed-head count is missing".into()))?;
    let head = value
        .get("head")
        .and_then(Value::as_str)
        .ok_or_else(|| AuditError::Corrupt("signed-head head is missing".into()))?
        .to_string();
    Ok(Some(SignedHead { count, head }))
}

fn persist_signed_head(path: &Path, signed: &SignedHead) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(
            serde_json::to_vec(&json!({"count": signed.count, "head": signed.head}))?.as_slice(),
        )?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
