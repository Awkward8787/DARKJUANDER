//! Append-only hash-chained audit log.
//!
//! The log starts with a genesis record. Each later line's hash is SHA-256 of
//! the canonical JSON of that record, so moving bytes between fields changes
//! the hash. A sibling checkpoint stores the entry count and head hash, signed
//! by an ed25519 key. [`verify_file`] checks the chain and the checkpoint.
//! Truncating the log, emptying it, or appending after a truncate fails.
//!
//! The signing key lives at `{log_dir}/keys/audit.ed25519`, or at
//! `DJ_AUDIT_SIGNING_KEY` when that variable is set. It is the anchor: an
//! editor who can change the log but not the key cannot publish a new head.
//! Replacing the key and the log together is indistinguishable from a new
//! install. Agents must not be able to write the key. Encryption at rest is
//! not implemented. One process holds an exclusive lock on the log; a second
//! opener fails.

#![deny(unsafe_code)]

use dj_label::Labels;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use fs2::FileExt;
use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
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
    key: SigningKey,
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

/// Ed25519 seed. Override with `DJ_AUDIT_SIGNING_KEY` when the key should not
/// sit next to the log directory. The default is `{log_dir}/keys/audit.ed25519`.
pub fn signing_key_path(log: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("DJ_AUDIT_SIGNING_KEY") {
        return PathBuf::from(path);
    }
    log.parent()
        .unwrap_or_else(|| Path::new("."))
        .join("keys")
        .join("audit.ed25519")
}

impl AuditLog {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AuditError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let checkpoint = checkpoint_path(&path);
        let key_path = signing_key_path(&path);
        let key_existed = key_path.exists();
        let key = load_or_create_key(&key_path, &path, &checkpoint)?;
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
            if checkpoint.exists() || key_existed {
                return Err(AuditError::Corrupt(
                    "audit log is empty but a checkpoint or signing key already exists; refusing to continue".into(),
                ));
            }
            let record = genesis_record();
            write_record(&file, &record)?;
            write_checkpoint(&checkpoint, &key, 1, &record.hash)?;
            (record.seq, record.hash)
        } else {
            let records = parse_chain(&existing)?;
            check_checkpoint(&checkpoint, key.verifying_key(), &records)?;
            let last = records.last().expect("chain is non-empty");
            (last.seq, last.hash.clone())
        };
        Ok(Self {
            inner: Mutex::new(Inner {
                path,
                seq,
                last_hash,
                file,
                key,
            }),
        })
    }

    pub fn path(&self) -> PathBuf {
        self.inner.lock().expect("audit lock").path.clone()
    }

    pub fn append(&self, event: Event) -> Result<Record, AuditError> {
        let mut inner = self.inner.lock().expect("audit lock");
        inner.seq += 1;
        let prev_hash = inner.last_hash.clone();
        let time = rfc3339_now();
        let hash = hash_event(
            inner.seq,
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
            seq: inner.seq,
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
        let count = inner.seq + 1;
        write_checkpoint(
            &checkpoint_path(&inner.path),
            &inner.key,
            count,
            &record.hash,
        )?;
        inner.last_hash = hash;
        Ok(record)
    }
}

pub fn verify_file(path: &Path) -> Result<VerifyReport, AuditError> {
    let records = read_records(path)?;
    let head = records
        .last()
        .map(|record| record.hash.clone())
        .ok_or_else(|| AuditError::Corrupt("audit log has no genesis record".into()))?;
    Ok(VerifyReport {
        entries: records.len() as u64,
        head,
    })
}

pub fn read_records(path: &Path) -> Result<Vec<Record>, AuditError> {
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
    let key = read_key(&signing_key_path(path))?;
    check_checkpoint(&checkpoint_path(path), key.verifying_key(), &records)?;
    Ok(records)
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

fn checkpoint_message(count: u64, head: &str) -> String {
    format!("darkjuander-audit-v1\ncount={count}\nhead={head}")
}

fn write_checkpoint(
    path: &Path,
    key: &SigningKey,
    count: u64,
    head: &str,
) -> Result<(), AuditError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let message = checkpoint_message(count, head);
    let signature = key.sign(message.as_bytes());
    let body = json!({
        "version": 1,
        "count": count,
        "head": head,
        "signature": hex_encode(signature.to_bytes()),
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

fn check_checkpoint(path: &Path, key: VerifyingKey, records: &[Record]) -> Result<(), AuditError> {
    if !path.exists() {
        return Err(AuditError::Corrupt(
            "audit checkpoint is missing; refusing a log with no signed head".into(),
        ));
    }
    let text = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&text)?;
    let count = value
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| AuditError::Corrupt("checkpoint count is missing".into()))?;
    let head = value
        .get("head")
        .and_then(Value::as_str)
        .ok_or_else(|| AuditError::Corrupt("checkpoint head is missing".into()))?;
    let signature = value
        .get("signature")
        .and_then(Value::as_str)
        .ok_or_else(|| AuditError::Corrupt("checkpoint signature is missing".into()))?;
    let signature_bytes = hex::decode(signature)
        .map_err(|err| AuditError::Corrupt(format!("checkpoint signature is not hex: {err}")))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|err| AuditError::Corrupt(format!("checkpoint signature is invalid: {err}")))?;
    let message = checkpoint_message(count, head);
    key.verify(message.as_bytes(), &signature).map_err(|_| {
        AuditError::Corrupt("checkpoint signature does not match the signing key".into())
    })?;
    if count != records.len() as u64 {
        return Err(AuditError::Corrupt(format!(
            "checkpoint count {count} does not match log length {}",
            records.len()
        )));
    }
    let actual = records
        .last()
        .map(|record| record.hash.as_str())
        .unwrap_or("");
    if head != actual {
        return Err(AuditError::Corrupt(
            "checkpoint head does not match the log".into(),
        ));
    }
    Ok(())
}

fn load_or_create_key(
    key_path: &Path,
    log: &Path,
    checkpoint: &Path,
) -> Result<SigningKey, AuditError> {
    if key_path.exists() {
        return read_key(key_path);
    }
    let log_nonempty = log.exists() && fs::metadata(log)?.len() > 0;
    if log_nonempty || checkpoint.exists() {
        return Err(AuditError::Corrupt(
            "audit signing key is missing; refusing to mint a replacement for an existing log"
                .into(),
        ));
    }
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)?;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed)
        .map_err(|err| AuditError::Corrupt(format!("could not generate an audit key: {err}")))?;
    let key = SigningKey::from_bytes(&seed);
    let mut file = File::create(key_path)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&seed)?;
    file.sync_all()?;
    Ok(key)
}

fn read_key(path: &Path) -> Result<SigningKey, AuditError> {
    let bytes = fs::read(path)?;
    if bytes.len() != 32 {
        return Err(AuditError::Corrupt(format!(
            "audit signing key {} must be 32 bytes",
            path.display()
        )));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&seed))
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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = AuditLog::open(&path).unwrap();
        log.append(event("email.send")).unwrap();
        log.append(event("files.read")).unwrap();
        let report = verify_file(&path).unwrap();
        assert_eq!(report.entries, 3);

        let mut text = fs::read_to_string(&path).unwrap();
        text = text.replace("email.send", "email.sent");
        fs::write(&path, text).unwrap();
        let err = verify_file(&path).unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
    }

    #[test]
    fn tail_truncation_empty_file_and_reopen_are_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.log");
        let log = AuditLog::open(&path).unwrap();
        for action in ["one", "two", "three", "four", "five"] {
            log.append(event(action)).unwrap();
        }
        drop(log);
        let full = verify_file(&path).unwrap();
        assert_eq!(full.entries, 6);

        let text = fs::read_to_string(&path).unwrap();
        let kept: String = text
            .lines()
            .take(2)
            .map(|line| format!("{line}\n"))
            .collect();
        fs::write(&path, &kept).unwrap();
        let err = verify_file(&path).unwrap_err();
        assert!(
            err.to_string().contains("checkpoint"),
            "truncation error was {err}"
        );

        fs::write(&path, "").unwrap();
        let err = verify_file(&path).unwrap_err();
        assert!(err.to_string().contains("empty"), "empty error was {err}");

        fs::write(&path, &kept).unwrap();
        match AuditLog::open(&path) {
            Err(err) => assert!(
                err.to_string().contains("checkpoint"),
                "reopen error was {err}"
            ),
            Ok(_) => panic!("reopen of a truncated log should fail"),
        }
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
