//! `dj-auditd` holds the ed25519 seed and an append-only copy of every
//! record it has signed.
//!
//! A sign request carries the next record. The signer recomputes that
//! record's hash from canonical JSON and accepts it only when it extends
//! the last signed record (`seq` is one past the previous record, and
//! `prev_hash` is that record's hash). It then signs the new head. A bare
//! count and head are not enough.
//!
//! The seed and the record copy live in `/var/lib/dj-audit` (mode `0700`).
//! The verifying key is published under `/run/dj-audit` so `umbrad` can read
//! it without traversing the seed directory. Only `dj-umbra`'s uid may
//! request a signature.
//!
//! This is not a TPM. Root, or anyone who can write the signer's state
//! directory, can still produce signatures. Offline verify also cannot see
//! a rollback that restores an old log together with the checkpoint that
//! was signed for it: that pair has no external anchor. TPM sealing is
//! tracked in the roadmap.

use crate::{checkpoint_message, AuditError};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub const DEFAULT_SIGNER_SOCKET: &str = "/run/dj-audit/signer.sock";
/// Group-readable path. The seed directory is mode `0700`, so the verifying
/// key cannot live beside the seed and still be readable by `umbrad`.
pub const DEFAULT_VERIFYING_KEY: &str = "/run/dj-audit/audit.ed25519.pub";
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

    /// Ask the signer to extend its chain with this record. The signer
    /// recomputes the hash and only then signs the new head.
    pub fn sign_record(&self, record: &crate::Record) -> Result<Vec<u8>, AuditError> {
        let response = self.transact(&json!({
            "op": "sign",
            "record": record,
        }))?;
        signature_from_response(&response)
    }

    pub fn transact(&self, request: &Value) -> Result<Value, AuditError> {
        self.round_trip(request)
    }
}

fn signature_from_response(response: &Value) -> Result<Vec<u8>, AuditError> {
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

const MAX_REQUEST_BYTES: usize = 64 * 1024;

pub fn serve(
    key_path: &Path,
    public_path: &Path,
    socket_path: &Path,
    shutdown: &AtomicBool,
    expected_uid: u32,
) -> Result<(), AuditError> {
    let key = Arc::new(load_or_create_signing_key(key_path)?);
    publish_verifying_key(public_path, &key.verifying_key())?;
    let records_path = key_path.with_file_name("signed-records.jsonl");
    let signed = Arc::new(std::sync::Mutex::new(load_signed_records(&records_path)?));
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
                let key = Arc::clone(&key);
                let signed = Arc::clone(&signed);
                let records_path = records_path.clone();
                std::thread::spawn(move || {
                    let _ = handle_client(&key, &signed, &records_path, expected_uid, stream);
                });
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
    signed: &std::sync::Mutex<Vec<crate::Record>>,
    records_path: &Path,
    expected_uid: u32,
    mut stream: UnixStream,
) -> Result<(), AuditError> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    let response = match peer_uid(&stream) {
        Ok(uid) if uid == expected_uid => match read_request(&mut stream) {
            Ok(request) => match request.get("op").and_then(Value::as_str) {
                Some("sign") => sign_request(key, signed, records_path, &request),
                _ => json!({"error": "unknown signer op"}),
            },
            Err(err) => json!({"error": err.to_string()}),
        },
        Ok(uid) => json!({"error": format!("peer uid {uid} is not the audit writer")}),
        Err(err) => json!({"error": err.to_string()}),
    };
    stream.write_all(&serde_json::to_vec(&response)?)?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn read_request(stream: &mut UnixStream) -> Result<Value, AuditError> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if buf.len() >= MAX_REQUEST_BYTES {
            return Err(AuditError::Corrupt("signer request is too large".into()));
        }
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => buf.push(byte[0]),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(AuditError::Corrupt("signer request timed out".into()));
            }
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                return Err(AuditError::Corrupt("signer request timed out".into()));
            }
            Err(err) => return Err(err.into()),
        }
    }
    serde_json::from_slice(&buf)
        .map_err(|err| AuditError::Corrupt(format!("signer request: {err}")))
}

fn sign_request(
    key: &SigningKey,
    signed: &std::sync::Mutex<Vec<crate::Record>>,
    records_path: &Path,
    request: &Value,
) -> Value {
    let Some(raw) = request.get("record") else {
        return json!({"error": "signer request needs the appended record"});
    };
    let Ok(record) = serde_json::from_value::<crate::Record>(raw.clone()) else {
        return json!({"error": "signer request record is not an audit record"});
    };
    let mut signed = match signed.lock() {
        Ok(guard) => guard,
        Err(_) => return json!({"error": "signer state lock is poisoned"}),
    };
    let decision = match authorize_record(&signed, &record) {
        Ok(decision) => decision,
        Err(err) => return json!({"error": err}),
    };
    let count = record.seq + 1;
    let message = checkpoint_message(count, &record.hash);
    let signature = key.sign(message.as_bytes());
    if decision == SignDecision::Extend {
        if let Err(err) = append_signed_record(records_path, &record) {
            return json!({"error": err.to_string()});
        }
        signed.push(record);
    }
    json!({"signature": hex::encode(signature.to_bytes())})
}

#[derive(PartialEq, Eq)]
enum SignDecision {
    Extend,
    Replay,
}

fn authorize_record(
    signed: &[crate::Record],
    record: &crate::Record,
) -> Result<SignDecision, String> {
    let recomputed = crate::recompute_record_hash(record);
    if recomputed != record.hash {
        return Err("record hash does not match canonical json".into());
    }
    match signed.last() {
        None => {
            if record.seq != 0 || record.prev_hash != crate::GENESIS || record.kind != "genesis" {
                return Err("first signed record must be the genesis anchor".into());
            }
            Ok(SignDecision::Extend)
        }
        Some(prev) if record.hash == prev.hash && record.seq == prev.seq => {
            Ok(SignDecision::Replay)
        }
        // Checkpoint count is `seq + 1`, so the next record's seq equals the
        // previous checkpoint count, and its prev_hash must be that head.
        Some(prev) if record.seq == prev.seq + 1 && record.prev_hash == prev.hash => {
            Ok(SignDecision::Extend)
        }
        Some(prev) => Err(format!(
            "refusing record seq {}; signer is at {}",
            record.seq, prev.seq
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
    spawn_test_signer_for(current_uid())
}

/// `expected_uid` is the only peer allowed to request a signature.
pub fn spawn_test_signer_for(expected_uid: u32) -> Result<TestSigner, AuditError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let key_dir = std::env::temp_dir().join(format!("dj-audit-keys-{}-{n}", std::process::id()));
    fs::create_dir_all(&key_dir)?;
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
        let _ = serve(
            &key_thread,
            &public_thread,
            &socket_thread,
            &flag,
            expected_uid,
        );
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
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
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

fn load_signed_records(path: &Path) -> Result<Vec<crate::Record>, AuditError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut records = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let record: crate::Record = serde_json::from_str(line)?;
        let recomputed = crate::recompute_record_hash(&record);
        if recomputed != record.hash {
            return Err(AuditError::Corrupt(format!(
                "signer copy hash mismatch at line {}",
                idx + 1
            )));
        }
        if let Some(prev) = records.last() {
            let prev: &crate::Record = prev;
            if record.seq != prev.seq + 1 || record.prev_hash != prev.hash {
                return Err(AuditError::Corrupt("signer copy does not chain".into()));
            }
        }
        records.push(record);
    }
    Ok(records)
}

fn append_signed_record(path: &Path, record: &crate::Record) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(serde_json::to_string(record)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[allow(unsafe_code)]
fn peer_uid(stream: &UnixStream) -> Result<u32, AuditError> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(AuditError::Corrupt(format!(
            "cannot read peer credentials: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(cred.uid)
}

#[allow(unsafe_code)]
pub fn current_uid() -> u32 {
    // SAFETY: geteuid does not touch memory we own and cannot fail.
    unsafe { libc::geteuid() }
}

/// Uid of `dj-umbra` when that account exists. A host without the account
/// (unit tests, a manual run) falls back to this process, which is the
/// writer in that case.
#[allow(unsafe_code)]
pub fn expected_writer_uid() -> u32 {
    let name = std::ffi::CString::new("dj-umbra").expect("name");
    // SAFETY: the name is a non-null C string. The passwd pointer is used
    // only to copy pw_uid before any later libc call can reuse it.
    let pwd = unsafe { libc::getpwnam(name.as_ptr()) };
    if pwd.is_null() {
        current_uid()
    } else {
        unsafe { (*pwd).pw_uid }
    }
}
