//! Append-only hash-chained audit log.
//!
//! Each line is one JSON object. The hash covers the previous hash and a
//! canonical encoding of the event, so a changed, deleted, or reordered line
//! fails [`verify_file`]. The writer opens the file append-only and mode
//! `0600`. Encryption at rest is not implemented.

#![forbid(unsafe_code)]

use dj_label::Labels;
use hex::encode as hex_encode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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
}

impl AuditLog {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AuditError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let existing = read_records(&path)?;
        let (seq, last_hash) = if let Some(last) = existing.last() {
            (last.seq, last.hash.clone())
        } else {
            (0, GENESIS.to_string())
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;
        let _ = file.set_permissions(fs::Permissions::from_mode(0o600));
        Ok(Self {
            inner: Mutex::new(Inner {
                path,
                seq,
                last_hash,
                file,
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
        let line = serde_json::to_string(&record)?;
        inner.file.write_all(line.as_bytes())?;
        inner.file.write_all(b"\n")?;
        inner.file.flush()?;
        inner.last_hash = hash;
        Ok(record)
    }
}

pub fn verify_file(path: &Path) -> Result<VerifyReport, AuditError> {
    let records = read_records(path)?;
    let head = records
        .last()
        .map(|record| record.hash.clone())
        .unwrap_or_else(|| GENESIS.to_string());
    Ok(VerifyReport {
        entries: records.len() as u64,
        head,
    })
}

pub fn read_records(path: &Path) -> Result<Vec<Record>, AuditError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut file = File::open(path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
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
        let expected_seq = (idx as u64) + 1;
        if record.seq != expected_seq {
            return Err(AuditError::Corrupt(format!(
                "sequence {} found where {expected_seq} was expected",
                record.seq
            )));
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
    Ok(records)
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
    let payload = format!(
        "{seq}\n{time}\n{kind}\n{actor}\n{action}\n{outcome}\n{reason}\n{}\n{}\n{prev_hash}",
        labels.fingerprint(),
        canonical_json(details)
    );
    let digest = Sha256::digest(payload.as_bytes());
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
        assert_eq!(report.entries, 2);

        let mut text = fs::read_to_string(&path).unwrap();
        text = text.replace("email.send", "email.sent");
        fs::write(&path, text).unwrap();
        let err = verify_file(&path).unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
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
