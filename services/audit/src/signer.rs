//! `dj-auditd` holds the ed25519 seed and the last record it signed.
//!
//! A sign request carries the next record. The signer recomputes that
//! record's hash from canonical JSON and accepts it only when it extends
//! the last signed record (`seq` is one past the previous record, and
//! `prev_hash` is that record's hash). It then signs the new head. A bare
//! count and head are not enough. Only that tip is kept in memory and on
//! disk; the hash chain itself lives in the audit log.
//!
//! The seed, the tip, and a durable copy of the verifying key live in
//! `/var/lib/dj-audit` (mode `0700`). The same verifying key is also
//! published under `/run/dj-audit` so `umbrad` can read it without
//! traversing the seed directory. Only `dj-umbra`'s uid may request a
//! signature, and only a few of those requests run at once.
//!
//! This is not a TPM. Root, or anyone who can write the signer's state
//! directory, can still produce signatures. Offline verify also cannot see
//! a rollback that restores an old log together with the checkpoint that
//! was signed for it: that pair has no external anchor. TPM sealing is
//! tracked in the roadmap.

use crate::{checkpoint_message, AuditError};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

pub const DEFAULT_SIGNER_SOCKET: &str = "/run/dj-audit/signer.sock";
/// Group-readable path. The seed directory is mode `0700`, so the verifying
/// key cannot live beside the seed and still be readable by `umbrad`.
pub const DEFAULT_VERIFYING_KEY: &str = "/run/dj-audit/audit.ed25519.pub";
/// Persistent copy of the verifying key, next to the seed. Not a secret.
/// Use it for `dj audit verify` when `/run` is gone.
pub const DURABLE_VERIFYING_KEY: &str = "/var/lib/dj-audit/audit.ed25519.pub";
pub const DEFAULT_SIGNING_KEY: &str = "/var/lib/dj-audit/audit.ed25519";

/// How many authorized clients may be inside a request at once.
pub(crate) const MAX_INFLIGHT_CLIENTS: usize = 8;
const TIP_FILE: &str = "signed-tip.json";
const LEGACY_RECORDS: &str = "signed-records.jsonl";

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
        // A busy or rejected signer can answer and close before this write
        // finishes. The error line is still sitting in the receive buffer.
        let payload = serde_json::to_vec(request)?;
        if let Err(err) = stream
            .write_all(&payload)
            .and_then(|_| stream.write_all(b"\n"))
        {
            if !signer_closed_during_write(&err) {
                return Err(err.into());
            }
        }
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line)?;
        if line.is_empty() {
            return Err(AuditError::Corrupt("audit signer closed the socket".into()));
        }
        Ok(serde_json::from_str(&line)?)
    }
}

/// The signer can answer and close before `write` finishes. `EPIPE` and
/// `ECONNRESET` both mean that; the reply may still be readable.
fn signer_closed_during_write(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::BrokenPipe
        || err.kind() == std::io::ErrorKind::ConnectionReset
        || err.raw_os_error() == Some(libc::ECONNRESET)
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
    let durable_public = key_path.with_file_name("audit.ed25519.pub");
    if durable_public != public_path {
        publish_verifying_key(&durable_public, &key.verifying_key())?;
    }
    let tip_path = key_path.with_file_name(TIP_FILE);
    let state = Arc::new(SignerState {
        key,
        tip: Mutex::new(load_signed_tip(&tip_path)?),
        tip_path,
        inflight: Arc::new(AtomicUsize::new(0)),
        expected_uid,
    });
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
            Ok((stream, _)) => dispatch_client(&state, stream),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(err.into()),
        }
    }
    let _ = fs::remove_file(socket_path);
    Ok(())
}

struct SignerState {
    key: Arc<SigningKey>,
    tip: Mutex<Option<SignedTip>>,
    tip_path: PathBuf,
    inflight: Arc<AtomicUsize>,
    expected_uid: u32,
}

struct InflightPermit {
    slots: Arc<AtomicUsize>,
}

impl Drop for InflightPermit {
    fn drop(&mut self) {
        self.slots.fetch_sub(1, Ordering::SeqCst);
    }
}

fn try_acquire(slots: &Arc<AtomicUsize>) -> Option<InflightPermit> {
    loop {
        let current = slots.load(Ordering::SeqCst);
        if current >= MAX_INFLIGHT_CLIENTS {
            return None;
        }
        if slots
            .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Some(InflightPermit {
                slots: Arc::clone(slots),
            });
        }
    }
}

/// Peer check and the concurrency cap run on the accept thread. A rejected
/// or overflow connection never gets a worker.
fn dispatch_client(state: &Arc<SignerState>, stream: UnixStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
    match peer_uid(&stream) {
        Ok(uid) if uid == state.expected_uid => {
            let Some(permit) = try_acquire(&state.inflight) else {
                let _ = write_response(&stream, &json!({"error": "signer is busy"}));
                return;
            };
            let state = Arc::clone(state);
            // A failed spawn drops the closure, which drops the permit.
            let _ = std::thread::Builder::new()
                .name("dj-audit-client".into())
                .spawn(move || {
                    let _permit = permit;
                    let _ = handle_client(&state, stream);
                });
        }
        Ok(uid) => {
            let _ = write_response(
                &stream,
                &json!({"error": format!("peer uid {uid} is not the audit writer")}),
            );
        }
        Err(err) => {
            let _ = write_response(&stream, &json!({"error": err.to_string()}));
        }
    }
}

fn handle_client(state: &SignerState, mut stream: UnixStream) -> Result<(), AuditError> {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    let response = match read_request(&mut stream) {
        Ok(request) => match request.get("op").and_then(Value::as_str) {
            Some("sign") => sign_request(state, &request),
            _ => json!({"error": "unknown signer op"}),
        },
        Err(err) => json!({"error": err.to_string()}),
    };
    write_response(&stream, &response)
}

fn write_response(mut stream: &UnixStream, response: &Value) -> Result<(), AuditError> {
    stream.write_all(&serde_json::to_vec(response)?)?;
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

fn sign_request(state: &SignerState, request: &Value) -> Value {
    let Some(raw) = request.get("record") else {
        return json!({"error": "signer request needs the appended record"});
    };
    let Ok(record) = serde_json::from_value::<crate::Record>(raw.clone()) else {
        return json!({"error": "signer request record is not an audit record"});
    };
    // A panic in another request poisons the mutex. The tip value is still
    // the last one stored, so keep signing instead of failing closed.
    let mut tip = lock_tip(&state.tip);
    let decision = match authorize_record(tip.as_ref(), &record) {
        Ok(decision) => decision,
        Err(err) => return json!({"error": err}),
    };
    let count = record.seq + 1;
    let message = checkpoint_message(count, &record.hash);
    let signature = state.key.sign(message.as_bytes());
    if decision == SignDecision::Extend {
        let next = SignedTip::from_record(&record);
        if let Err(err) = store_signed_tip(&state.tip_path, &next) {
            return json!({"error": err.to_string()});
        }
        *tip = Some(next);
    }
    json!({"signature": hex::encode(signature.to_bytes())})
}

fn lock_tip(tip: &Mutex<Option<SignedTip>>) -> std::sync::MutexGuard<'_, Option<SignedTip>> {
    tip.lock().unwrap_or_else(|err| err.into_inner())
}

#[derive(PartialEq, Eq)]
enum SignDecision {
    Extend,
    Replay,
}

fn authorize_record(
    tip: Option<&SignedTip>,
    record: &crate::Record,
) -> Result<SignDecision, String> {
    let recomputed = crate::recompute_record_hash(record);
    if recomputed != record.hash {
        return Err("record hash does not match canonical json".into());
    }
    match tip {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct SignedTip {
    seq: u64,
    prev_hash: String,
    hash: String,
}

impl SignedTip {
    fn from_record(record: &crate::Record) -> Self {
        Self {
            seq: record.seq,
            prev_hash: record.prev_hash.clone(),
            hash: record.hash.clone(),
        }
    }
}

pub struct TestSigner {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub paths: AuditPaths,
    pub key_path: PathBuf,
    /// One signer at a time in this process, so a flood test can count workers.
    _lock: MutexGuard<'static, ()>,
}

static TEST_SIGNER_LOCK: Mutex<()> = Mutex::new(());
static NEXT_SIGNER: AtomicU64 = AtomicU64::new(0);

pub fn spawn_test_signer() -> Result<TestSigner, AuditError> {
    spawn_test_signer_for(current_uid())
}

/// `expected_uid` is the only peer allowed to request a signature.
pub fn spawn_test_signer_for(expected_uid: u32) -> Result<TestSigner, AuditError> {
    let n = NEXT_SIGNER.fetch_add(1, Ordering::SeqCst);
    let key_dir = std::env::temp_dir().join(format!("dj-audit-keys-{}-{n}", std::process::id()));
    fs::create_dir_all(&key_dir)?;
    start_test_signer(
        key_dir.join("audit.ed25519"),
        key_dir.join("audit.ed25519.pub"),
        expected_uid,
    )
}

/// Start another signer on an existing seed directory. The tip file is reloaded.
#[cfg(test)]
pub(crate) fn spawn_test_signer_in(
    key_dir: &Path,
    expected_uid: u32,
) -> Result<TestSigner, AuditError> {
    start_test_signer(
        key_dir.join("audit.ed25519"),
        key_dir.join("audit.ed25519.pub"),
        expected_uid,
    )
}

/// Publish the live verifying key outside the seed directory.
#[cfg(test)]
pub(crate) fn spawn_test_signer_with_separate_public() -> Result<TestSigner, AuditError> {
    let n = NEXT_SIGNER.fetch_add(1, Ordering::SeqCst);
    let key_dir = std::env::temp_dir().join(format!("dj-audit-keys-{}-{n}", std::process::id()));
    fs::create_dir_all(&key_dir)?;
    let public = std::env::temp_dir().join(format!("dj-audit-pub-{}-{n}.pub", std::process::id()));
    start_test_signer(key_dir.join("audit.ed25519"), public, current_uid())
}

fn start_test_signer(
    key_path: PathBuf,
    public_path: PathBuf,
    expected_uid: u32,
) -> Result<TestSigner, AuditError> {
    let _lock = TEST_SIGNER_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let n = NEXT_SIGNER.fetch_add(1, Ordering::SeqCst);
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
        _lock,
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

fn load_signed_tip(path: &Path) -> Result<Option<SignedTip>, AuditError> {
    if path.exists() {
        let text = fs::read_to_string(path)?;
        let tip: SignedTip = serde_json::from_str(text.trim())?;
        return check_tip(tip).map(Some);
    }
    let legacy = path.with_file_name(LEGACY_RECORDS);
    if !legacy.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&legacy)?;
    let Some(line) = text.lines().rev().find(|line| !line.is_empty()) else {
        let _ = fs::remove_file(&legacy);
        return Ok(None);
    };
    let record: crate::Record = serde_json::from_str(line)?;
    if crate::recompute_record_hash(&record) != record.hash {
        return Err(AuditError::Corrupt(
            "legacy signer copy hash mismatch".into(),
        ));
    }
    let tip = SignedTip::from_record(&record);
    store_signed_tip(path, &tip)?;
    let _ = fs::remove_file(&legacy);
    Ok(Some(tip))
}

fn check_tip(tip: SignedTip) -> Result<SignedTip, AuditError> {
    if tip.hash.len() != 64 || tip.prev_hash.len() != 64 {
        return Err(AuditError::Corrupt(
            "signed tip hash is not 64 hex characters".into(),
        ));
    }
    Ok(tip)
}

fn store_signed_tip(path: &Path, tip: &SignedTip) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(&serde_json::to_vec(tip)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    // The file sync above makes the bytes durable. The rename itself is only
    // durable once the parent directory is synced.
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    File::open(dir)?.sync_all()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tip(seq: u64) -> SignedTip {
        SignedTip {
            seq,
            prev_hash: "ab".repeat(32),
            hash: "cd".repeat(32),
        }
    }

    #[test]
    fn poisoned_tip_lock_returns_the_last_tip() {
        let tip = Mutex::new(Some(sample_tip(4)));
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = tip.lock().expect("lock before poison");
            panic!("worker failed while holding the tip");
        }));
        assert!(panicked.is_err());
        assert!(tip.is_poisoned());
        let guard = lock_tip(&tip);
        assert_eq!(guard.as_ref().expect("recovered tip").seq, 4);
    }

    #[test]
    fn reset_during_write_is_treated_like_a_broken_pipe() {
        let reset = std::io::Error::from_raw_os_error(libc::ECONNRESET);
        let pipe = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe");
        let other = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert!(signer_closed_during_write(&reset));
        assert!(signer_closed_during_write(&pipe));
        assert!(!signer_closed_during_write(&other));
    }

    #[test]
    fn signed_tip_rename_survives_a_directory_sync() {
        let dir = std::env::temp_dir().join(format!(
            "dj-audit-tip-{}-{}",
            std::process::id(),
            NEXT_SIGNER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("signed-tip.json");
        store_signed_tip(&path, &sample_tip(4)).unwrap();
        let loaded = load_signed_tip(&path).unwrap().unwrap();
        assert_eq!(loaded.seq, 4);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
