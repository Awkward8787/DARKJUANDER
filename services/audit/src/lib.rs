//! Append-only hash-chained audit log.
//!
//! The log starts with a genesis record. Each later line's hash is SHA-256 of
//! the canonical JSON of that record, so moving bytes between fields changes
//! the hash. A sibling checkpoint stores the entry count and head hash, signed
//! by an ed25519 key. [`verify_file`] checks the chain and the checkpoint.
//! Truncating the log, emptying it, or appending after a truncate fails.
//!
//! The signing seed is not stored next to the log. `dj-auditd` holds it as a
//! different user and signs heads over a unix socket. [`verify_file`] reads
//! only the published verifying key. A writer who can edit the log but cannot
//! make the signer go backwards cannot hide a truncated chain.
//!
//! That split stops a compromised `umbrad`. It does not stop root, and it is
//! not a TPM. Sealing the seed to a TPM is still roadmap work. Until then the
//! signer protects against offline edits by someone who does not have the
//! signer's uid.
//!
//! If a crash lands after the record is synced and before the checkpoint is
//! replaced, `open` accepts that single trailing record once it chains from
//! the signed head, asks the signer for the new head, and writes the
//! checkpoint. Two extra records, or a shorter log, still fail.
//! Encryption at rest is not implemented. One process holds an exclusive lock.

#![deny(unsafe_code)]

use dj_label::Labels;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use signer::{load_verifying_key, SignerClient};

mod signer;

use fs2::FileExt;
use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub use signer::{
    serve as serve_signer, spawn_test_signer, AuditPaths, TestSigner, DEFAULT_SIGNER_SOCKET,
    DEFAULT_SIGNING_KEY, DEFAULT_VERIFYING_KEY,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit log io: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit log is not a valid chain: {0}")]
    Corrupt(String),
    #[error("audit record is not valid json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug)]
pub struct Event {
    pub kind: String,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub reason: String,
    pub labels: Labels,
    pub details: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub seq: u64,
    pub time: String,
    pub kind: String,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub reason: String,
    pub labels: Labels,
    pub details: Value,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Debug)]
pub struct VerifyReport {
    pub entries: u64,
    pub head: String,
}

pub struct AuditLog {
    inner: Mutex<Inner>,
}

struct Inner {
    path: PathBuf,
    seq: u64,
    last_hash: String,
    file: File,
    signer: SignerClient,
    verifying: VerifyingKey,
    poisoned: bool,
}

/// Sibling of the log that stores the signed head. Not inside the log, so
/// deleting a suffix of the log leaves this file behind.
pub fn checkpoint_path(log: &Path) -> PathBuf {
    let mut name = log
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "audit.log".into());
    name.push(".checkpoint");
    log.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

impl AuditLog {
    pub fn open(path: impl Into<PathBuf>, paths: &AuditPaths) -> Result<Self, AuditError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let verifying = load_verifying_key(&paths.verifying_key)?;
        let signer = SignerClient::new(&paths.signer_socket);
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;
        file.try_lock_exclusive().map_err(|err| {
            AuditError::Corrupt(format!("audit log is locked by another writer: {err}"))
        })?;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        let existing = read_file_text(&file)?;
        let (seq, last_hash) = if existing.is_empty() {
            if checkpoint_path(&path).exists() {
                return Err(AuditError::Corrupt(
                    "audit log is empty but a checkpoint already exists; refusing to continue"
                        .into(),
                ));
            }
            let record = genesis_record();
            write_record(&file, &record)?;
            commit_checkpoint(
                &checkpoint_path(&path),
                &signer,
                &verifying,
                1,
                &record.hash,
            )?;
            (record.seq, record.hash)
        } else {
            let records = parse_chain(&existing)?;
            align_checkpoint(&path, &records, &signer, &verifying)?
        };
        Ok(Self {
            inner: Mutex::new(Inner {
                path,
                seq,
                last_hash,
                file,
                signer,
                verifying,
                poisoned: false,
            }),
        })
    }

    pub fn path(&self) -> PathBuf {
        self.inner.lock().expect("audit lock").path.clone()
    }

    pub fn append(&self, event: Event) -> Result<Record, AuditError> {
        let mut inner = self.inner.lock().expect("audit lock");
        if inner.poisoned {
            return Err(AuditError::Corrupt(
                "audit log needs to be reopened after a partial append".into(),
            ));
        }
        let seq = inner.seq + 1;
        let prev_hash = inner.last_hash.clone();
        let time = rfc3339_now();
        let hash = hash_event(
            seq,
            &time,
            &event.kind,
            &event.actor,
            &event.action,
            &event.outcome,
            &event.reason,
            &event.labels,
            &event.details,
            &prev_hash,
        );
        let record = Record {
            seq,
            time,
            kind: event.kind,
            actor: event.actor,
            action: event.action,
            outcome: event.outcome,
            reason: event.reason,
            labels: event.labels,
            details: event.details,
            prev_hash,
            hash: hash.clone(),
        };
        write_record(&inner.file, &record)?;
        let count = seq + 1;
        if let Err(err) = commit_checkpoint(
            &checkpoint_path(&inner.path),
            &inner.signer,
            &inner.verifying,
            count,
            &record.hash,
        ) {
            inner.poisoned = true;
            inner.seq = seq;
            inner.last_hash = hash;
            return Err(err);
        }
        inner.seq = seq;
        inner.last_hash = hash;
        Ok(record)
    }

    /// Test hook: sync a record and stop before the checkpoint, as a crash would.
    #[cfg(test)]
    fn append_record_only(&self, event: Event) -> Result<Record, AuditError> {
        let mut inner = self.inner.lock().expect("audit lock");
        let seq = inner.seq + 1;
        let prev_hash = inner.last_hash.clone();
        let record = build_record(seq, &prev_hash, event);
        write_record(&inner.file, &record)?;
        inner.seq = seq;
        inner.last_hash = record.hash.clone();
        inner.poisoned = true;
        Ok(record)
    }
}

pub fn verify_file(path: &Path, verifying_key: &Path) -> Result<VerifyReport, AuditError> {
    let records = read_records(path, verifying_key)?;
    let head = records
        .last()
        .map(|record| record.hash.clone())
        .ok_or_else(|| AuditError::Corrupt("audit log has no genesis record".into()))?;
    Ok(VerifyReport {
        entries: records.len() as u64,
        head,
    })
}

pub fn read_records(path: &Path, verifying_key: &Path) -> Result<Vec<Record>, AuditError> {
    if !path.exists() {
        return Err(AuditError::Corrupt("audit log is missing".into()));
    }
    let text = fs::read_to_string(path)?;
    if text.is_empty() {
        return Err(AuditError::Corrupt(
            "audit log is empty; the genesis anchor is gone".into(),
        ));
    }
    let records = parse_chain(&text)?;
    let key = load_verifying_key(verifying_key)?;
    match fit_checkpoint(&checkpoint_path(path), &key, &records)? {
        CheckpointFit::Exact => Ok(records),
        CheckpointFit::OneAhead => Err(AuditError::Corrupt(
            "checkpoint count does not match log length; reopen the log to checkpoint the trailing record".into(),
        )),
    }
}

fn read_file_text(file: &File) -> Result<String, AuditError> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn parse_chain(text: &str) -> Result<Vec<Record>, AuditError> {
    if !text.ends_with('\n') {
        return Err(AuditError::Corrupt(
            "audit log does not end with a newline".into(),
        ));
    }
    let mut records = Vec::new();
    let mut expected_prev = GENESIS.to_string();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            return Err(AuditError::Corrupt(format!("empty line at {}", idx + 1)));
        }
        let record: Record = serde_json::from_str(line)?;
        let expected_seq = idx as u64;
        if record.seq != expected_seq {
            return Err(AuditError::Corrupt(format!(
                "sequence {} found where {expected_seq} was expected",
                record.seq
            )));
        }
        if idx == 0 && (record.kind != "genesis" || record.prev_hash != GENESIS) {
            return Err(AuditError::Corrupt(
                "audit log does not start with the genesis anchor".into(),
            ));
        }
        if record.prev_hash != expected_prev {
            return Err(AuditError::Corrupt(format!(
                "prev_hash mismatch at seq {}",
                record.seq
            )));
        }
        let computed = hash_event(
            record.seq,
            &record.time,
            &record.kind,
            &record.actor,
            &record.action,
            &record.outcome,
            &record.reason,
            &record.labels,
            &record.details,
            &record.prev_hash,
        );
        if computed != record.hash {
            return Err(AuditError::Corrupt(format!(
                "hash mismatch at seq {}",
                record.seq
            )));
        }
        expected_prev = record.hash.clone();
        records.push(record);
    }
    if records.is_empty() {
        return Err(AuditError::Corrupt(
            "audit log is empty; the genesis anchor is gone".into(),
        ));
    }
    Ok(records)
}

fn genesis_record() -> Record {
    let time = rfc3339_now();
    let labels = Labels::default();
    let prev_hash = GENESIS.to_string();
    let hash = hash_event(
        0,
        &time,
        "genesis",
        "dj-audit",
        "anchor",
        "created",
        "genesis anchor",
        &labels,
        &json!({}),
        &prev_hash,
    );
    Record {
        seq: 0,
        time,
        kind: "genesis".into(),
        actor: "dj-audit".into(),
        action: "anchor".into(),
        outcome: "created".into(),
        reason: "genesis anchor".into(),
        labels,
        details: json!({}),
        prev_hash,
        hash,
    }
}

fn write_record(file: &File, record: &Record) -> Result<(), AuditError> {
    let mut file = file.try_clone()?;
    let line = serde_json::to_string(record)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn checkpoint_message(count: u64, head: &str) -> String {
    format!("darkjuander-audit-v1\ncount={count}\nhead={head}")
}

#[cfg(test)]
fn build_record(seq: u64, prev_hash: &str, event: Event) -> Record {
    let time = rfc3339_now();
    let hash = hash_event(
        seq,
        &time,
        &event.kind,
        &event.actor,
        &event.action,
        &event.outcome,
        &event.reason,
        &event.labels,
        &event.details,
        prev_hash,
    );
    Record {
        seq,
        time,
        kind: event.kind,
        actor: event.actor,
        action: event.action,
        outcome: event.outcome,
        reason: event.reason,
        labels: event.labels,
        details: event.details,
        prev_hash: prev_hash.to_string(),
        hash,
    }
}

fn commit_checkpoint(
    path: &Path,
    signer: &SignerClient,
    verifying: &VerifyingKey,
    count: u64,
    head: &str,
) -> Result<(), AuditError> {
    let signature = signer.sign(count, head)?;
    let parsed = Signature::from_slice(&signature)
        .map_err(|err| AuditError::Corrupt(format!("signer returned a bad signature: {err}")))?;
    verifying
        .verify(checkpoint_message(count, head).as_bytes(), &parsed)
        .map_err(|_| {
            AuditError::Corrupt("signer signature does not match the verifying key".into())
        })?;
    write_checkpoint(path, count, head, &signature)
}

fn align_checkpoint(
    log: &Path,
    records: &[Record],
    signer: &SignerClient,
    verifying: &VerifyingKey,
) -> Result<(u64, String), AuditError> {
    let path = checkpoint_path(log);
    match fit_checkpoint(&path, verifying, records)? {
        CheckpointFit::Exact => {
            let last = records.last().expect("chain is non-empty");
            Ok((last.seq, last.hash.clone()))
        }
        CheckpointFit::OneAhead => {
            let last = records.last().expect("chain is non-empty");
            commit_checkpoint(&path, signer, verifying, records.len() as u64, &last.hash)?;
            Ok((last.seq, last.hash.clone()))
        }
    }
}

enum CheckpointFit {
    Exact,
    OneAhead,
}

fn fit_checkpoint(
    path: &Path,
    key: &VerifyingKey,
    records: &[Record],
) -> Result<CheckpointFit, AuditError> {
    let cp = load_checkpoint(path, key)?;
    if records.len() as u64 == cp.count {
        let actual = records
            .last()
            .map(|record| record.hash.as_str())
            .unwrap_or("");
        if cp.head != actual {
            return Err(AuditError::Corrupt(
                "checkpoint head does not match the log".into(),
            ));
        }
        return Ok(CheckpointFit::Exact);
    }
    if records.len() as u64 == cp.count + 1 {
        let prev = records
            .get(cp.count as usize - 1)
            .ok_or_else(|| AuditError::Corrupt("trailing record has no predecessor".into()))?;
        let last = records.last().expect("chain is non-empty");
        if prev.hash != cp.head || last.prev_hash != cp.head {
            return Err(AuditError::Corrupt(
                "trailing record does not chain from the signed head".into(),
            ));
        }
        return Ok(CheckpointFit::OneAhead);
    }
    Err(AuditError::Corrupt(format!(
        "checkpoint count {} does not match log length {}",
        cp.count,
        records.len()
    )))
}

struct CheckpointBody {
    count: u64,
    head: String,
}

fn load_checkpoint(path: &Path, key: &VerifyingKey) -> Result<CheckpointBody, AuditError> {
    if !path.exists() {
        return Err(AuditError::Corrupt(
            "audit checkpoint is missing; refusing a log with no signed head".into(),
        ));
    }
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let count = value
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| AuditError::Corrupt("checkpoint count is missing".into()))?;
    let head = value
        .get("head")
        .and_then(Value::as_str)
        .ok_or_else(|| AuditError::Corrupt("checkpoint head is missing".into()))?
        .to_string();
    let signature = value
        .get("signature")
        .and_then(Value::as_str)
        .ok_or_else(|| AuditError::Corrupt("checkpoint signature is missing".into()))?;
    let signature_bytes = hex::decode(signature)
        .map_err(|err| AuditError::Corrupt(format!("checkpoint signature is not hex: {err}")))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|err| AuditError::Corrupt(format!("checkpoint signature is invalid: {err}")))?;
    key.verify(checkpoint_message(count, &head).as_bytes(), &signature)
        .map_err(|_| {
            AuditError::Corrupt("checkpoint signature does not match the verifying key".into())
        })?;
    Ok(CheckpointBody { count, head })
}

fn write_checkpoint(
    path: &Path,
    count: u64,
    head: &str,
    signature: &[u8],
) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = json!({
        "version": 1,
        "count": count,
        "head": head,
        "signature": hex_encode(signature),
    });
    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp)?;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        file.write_all(&serde_json::to_vec(&body)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn hash_event(
    seq: u64,
    time: &str,
    kind: &str,
    actor: &str,
    action: &str,
    outcome: &str,
    reason: &str,
    labels: &Labels,
    details: &Value,
    prev_hash: &str,
) -> String {
    let payload = json!({
        "action": action,
        "actor": actor,
        "details": details,
        "kind": kind,
        "labels": labels,
        "outcome": outcome,
        "prev_hash": prev_hash,
        "reason": reason,
        "seq": seq,
        "time": time,
    });
    let digest = Sha256::digest(canonical_json(&payload).as_bytes());
    hex_encode(digest)
}

pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => serde_json::to_string(text).expect("string json"),
        Value::Array(items) => {
            let parts: Vec<_> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let parts: Vec<_> = keys
                .iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key json"),
                        canonical_json(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

/// UTC timestamp with millisecond precision. Enough for an audit line.
pub fn rfc3339_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_rfc3339(dur.as_secs(), dur.subsec_millis())
}

pub fn format_rfc3339(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`, days since 1970-01-01.
fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (if month <= 2 { y + 1 } else { y }) as i32;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dj_label::Labels;
    use serde_json::json;

    struct Harness {
        _dir: tempfile::TempDir,
        path: PathBuf,
        signer: TestSigner,
    }

    impl Harness {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let signer = spawn_test_signer().unwrap();
            Self {
                path: dir.path().join("audit.log"),
                _dir: dir,
                signer,
            }
        }

        fn open(&self) -> AuditLog {
            AuditLog::open(&self.path, &self.signer.paths).unwrap()
        }

        fn verify(&self) -> Result<VerifyReport, AuditError> {
            verify_file(&self.path, &self.signer.paths.verifying_key)
        }
    }

    fn event(action: &str) -> Event {
        Event {
            kind: "tool_call".into(),
            actor: "agent-a".into(),
            action: action.into(),
            outcome: "deny".into(),
            reason: "test".into(),
            labels: Labels::user("agent-a"),
            details: json!({"b": 1, "a": "x"}),
        }
    }

    #[test]
    fn chain_verifies_and_detects_edits() {
        let harness = Harness::new();
        let log = harness.open();
        log.append(event("email.send")).unwrap();
        log.append(event("files.read")).unwrap();
        drop(log);
        let report = harness.verify().unwrap();
        assert_eq!(report.entries, 3);

        let mut text = fs::read_to_string(&harness.path).unwrap();
        text = text.replace("email.send", "email.sent");
        fs::write(&harness.path, text).unwrap();
        let err = harness.verify().unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn tail_truncation_empty_file_and_reopen_are_detected() {
        let harness = Harness::new();
        let log = harness.open();
        for action in ["one", "two", "three", "four", "five"] {
            log.append(event(action)).unwrap();
        }
        drop(log);
        let full = harness.verify().unwrap();
        assert_eq!(full.entries, 6);

        let text = fs::read_to_string(&harness.path).unwrap();
        let kept: String = text
            .lines()
            .take(2)
            .map(|line| format!("{line}\n"))
            .collect();
        fs::write(&harness.path, &kept).unwrap();
        let err = harness.verify().unwrap_err();
        assert!(
            err.to_string().contains("checkpoint"),
            "truncation error was {err}"
        );

        fs::write(&harness.path, "").unwrap();
        let err = harness.verify().unwrap_err();
        assert!(err.to_string().contains("empty"), "empty error was {err}");

        fs::write(&harness.path, &kept).unwrap();
        match AuditLog::open(&harness.path, &harness.signer.paths) {
            Err(err) => assert!(
                err.to_string().contains("checkpoint"),
                "reopen error was {err}"
            ),
            Ok(_) => panic!("reopen of a truncated log should fail"),
        }
    }

    #[test]
    fn crash_between_record_and_checkpoint_recovers_on_open() {
        let harness = Harness::new();
        let log = harness.open();
        log.append(event("kept")).unwrap();
        drop(log);
        let before = harness.verify().unwrap().entries;

        // Crash before the next record is written: the log is unchanged.
        let log = harness.open();
        drop(log);
        assert_eq!(harness.verify().unwrap().entries, before);

        // Crash after the record is synced and before the checkpoint.
        let log = harness.open();
        log.append_record_only(event("crashed")).unwrap();
        drop(log);
        let err = harness.verify().unwrap_err();
        assert!(
            err.to_string().contains("checkpoint"),
            "pre-recovery verify was {err}"
        );
        let log = harness.open();
        drop(log);
        let recovered = harness.verify().unwrap();
        assert_eq!(recovered.entries, before + 1);

        // Crash after both writes: a normal append is already consistent.
        let log = harness.open();
        log.append(event("done")).unwrap();
        drop(log);
        assert_eq!(harness.verify().unwrap().entries, before + 2);
    }

    #[test]
    fn two_trailing_records_are_not_replayed() {
        let harness = Harness::new();
        let log = harness.open();
        log.append_record_only(event("one")).unwrap();
        log.append_record_only(event("two")).unwrap();
        drop(log);
        match AuditLog::open(&harness.path, &harness.signer.paths) {
            Err(err) => assert!(err.to_string().contains("checkpoint"), "{err}"),
            Ok(_) => panic!("two extra records should not be checkpointed automatically"),
        }
    }

    #[test]
    fn log_writer_cannot_resign_a_truncated_chain() {
        let harness = Harness::new();
        let log = harness.open();
        for action in ["one", "two", "three", "four", "five"] {
            log.append(event(action)).unwrap();
        }
        drop(log);
        assert!(
            !harness.path.parent().unwrap().join("keys").exists(),
            "signing seed must not live under the log directory"
        );
        let mode = fs::metadata(&harness.signer.key_path)
            .unwrap()
            .permissions()
            .mode();
        fs::set_permissions(&harness.signer.key_path, fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            harness.verify().is_ok(),
            "verify must use the published key, not the seed"
        );
        fs::set_permissions(&harness.signer.key_path, fs::Permissions::from_mode(mode)).unwrap();

        let text = fs::read_to_string(&harness.path).unwrap();
        let kept: String = text
            .lines()
            .take(3)
            .map(|line| format!("{line}\n"))
            .collect();
        fs::write(&harness.path, kept).unwrap();
        let client = signer::SignerClient::new(&harness.signer.paths.signer_socket);
        let err = client.sign(3, &"0".repeat(64)).unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
        assert!(harness.verify().is_err());
    }

    #[test]
    fn newline_across_fields_does_not_preserve_the_hash() {
        let labels = Labels::user("agent-a");
        let details = json!({});
        let left = hash_event(
            1,
            "t",
            "tool_call",
            "a",
            "email.send\nx",
            "deny",
            "r",
            &labels,
            &details,
            GENESIS,
        );
        let right = hash_event(
            1,
            "t",
            "tool_call",
            "a\nemail.send",
            "x",
            "deny",
            "r",
            &labels,
            &details,
            GENESIS,
        );
        assert_ne!(left, right);
    }

    #[test]
    fn unix_epoch_formats() {
        assert_eq!(format_rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn canonical_json_sorts_keys() {
        assert_eq!(
            canonical_json(&json!({"b": 1, "a": "x"})),
            canonical_json(&json!({"a": "x", "b": 1}))
        );
    }
}
