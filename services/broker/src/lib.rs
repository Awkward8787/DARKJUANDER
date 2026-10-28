//! Permission broker.
//!
//! A grant is necessary and not sufficient. Policy still runs. Approval
//! prompts fail closed in this slice: there is no trusted desktop dialog yet,
//! so an `approve` decision is recorded and then denied.

#![deny(unsafe_code)]

use dj_audit::{AuditLog, Event};
use dj_label::{Confidentiality, Integrity, Labels};
use dj_policy::{ActionClass, DecisionKind, Policy, PolicyRequest, RouteKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs::{self, File};
use std::io::Read;
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

#[derive(Clone, Debug)]
pub struct Principal {
    pub id: String,
    pub trust_domain: String,
    pub grants: BTreeSet<String>,
    pub local_only: bool,
}

impl Principal {
    pub fn has_grant(&self, grant: &str) -> bool {
        self.grants.iter().any(|held| held == grant)
    }

    pub fn file_grant<'a>(&'a self, action: &str, target: &Path) -> Option<&'a str> {
        let target = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
        self.grants.iter().find_map(|grant| {
            let root = grant.strip_prefix(&format!("{action}:"))?;
            if root.is_empty() {
                return None;
            }
            let root_path = Path::new(root.trim_end_matches('/'));
            let root_path = fs::canonicalize(root_path).unwrap_or_else(|_| root_path.to_path_buf());
            if target == root_path || target.starts_with(&root_path) {
                Some(grant.as_str())
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Debug)]
pub struct ActionRequest {
    pub action: String,
    pub target: String,
    pub labels: Labels,
    pub arguments: Value,
    pub route: Option<RouteKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authz {
    pub allowed: bool,
    pub prompted: bool,
    pub rule_id: String,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error(transparent)]
    Audit(#[from] dj_audit::AuditError),
    #[error("file mediation: {0}")]
    File(String),
}

pub struct Broker {
    policy: Policy,
    audit: AuditLog,
}

impl Broker {
    pub fn new(policy: Policy, audit: AuditLog) -> Self {
        Self { policy, audit }
    }

    pub fn audit(&self) -> &AuditLog {
        &self.audit
    }

    pub fn authorize(
        &self,
        principal: &Principal,
        request: &ActionRequest,
    ) -> Result<Authz, BrokerError> {
        let class = ActionClass::of(&request.action);
        let granted = match class {
            ActionClass::Read | ActionClass::Destructive => principal
                .file_grant(&request.action, Path::new(&request.target))
                .is_some(),
            ActionClass::Model => principal.has_grant(&format!("model.use:{}", request.target)),
            _ => principal.has_grant(&request.action),
        };
        if !granted {
            let reason = format!(
                "capability not granted for {} {}",
                request.action, request.target
            );
            self.record(principal, request, "denial", &reason, false)?;
            return Ok(Authz {
                allowed: false,
                prompted: false,
                rule_id: "grant".into(),
                reason,
            });
        }

        let decision = self.policy.decide(&PolicyRequest {
            action: request.action.clone(),
            action_class: class,
            labels: request.labels.clone(),
            route: request.route,
        });
        self.record(
            principal,
            request,
            "policy",
            &format!("{}: {}", decision.rule_id, decision.reason),
            false,
        )?;

        match decision.kind {
            DecisionKind::Allow => {
                self.record(principal, request, "allow", &decision.reason, false)?;
                Ok(Authz {
                    allowed: true,
                    prompted: false,
                    rule_id: decision.rule_id,
                    reason: decision.reason,
                })
            }
            DecisionKind::Deny => {
                self.record(principal, request, "denial", &decision.reason, false)?;
                Ok(Authz {
                    allowed: false,
                    prompted: false,
                    rule_id: decision.rule_id,
                    reason: decision.reason,
                })
            }
            DecisionKind::Approve => {
                self.record(
                    principal,
                    request,
                    "approval_prompt",
                    &decision.reason,
                    true,
                )?;
                let reason = format!("{}; no interactive approver; fail closed", decision.reason);
                self.record(principal, request, "denial", &reason, true)?;
                Ok(Authz {
                    allowed: false,
                    prompted: true,
                    rule_id: decision.rule_id,
                    reason,
                })
            }
        }
    }
}

impl Broker {
    fn record(
        &self,
        principal: &Principal,
        request: &ActionRequest,
        kind: &str,
        reason: &str,
        prompted: bool,
    ) -> Result<(), BrokerError> {
        let outcome = match kind {
            "allow" => "allow",
            "approval_prompt" => "prompt",
            "policy" => "recorded",
            _ => "deny",
        };
        self.audit.append(Event {
            kind: kind.to_string(),
            actor: principal.id.clone(),
            action: request.action.clone(),
            outcome: outcome.to_string(),
            reason: reason.to_string(),
            labels: request.labels.clone(),
            details: json!({
                "target": request.target,
                "arguments": request.arguments,
                "prompted": prompted,
                "trust_domain": principal.trust_domain,
            }),
        })?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediatedFile {
    pub text: String,
    pub labels: Labels,
    pub path: PathBuf,
}

/// Read a file through one `openat2` under the grant root.
///
/// `RESOLVE_BENEATH` and `RESOLVE_NO_SYMLINKS` are applied to that open. The
/// label comes from the fd's path, not from a second lookup of the caller's
/// path. A symlink swap between a check and a later open cannot retarget the
/// read, because there is no second open.
pub fn read_mediated_file(
    principal: &Principal,
    requested: &Path,
    private_roots: &[PathBuf],
    secret_roots: &[PathBuf],
) -> Result<MediatedFile, BrokerError> {
    let requested = lexical_absolute(requested)?;
    let root = grant_root(principal, &requested)?;
    let relative = requested.strip_prefix(&root).map_err(|_| {
        BrokerError::File(format!(
            "resolved path {} is outside the grant",
            requested.display()
        ))
    })?;
    if relative.as_os_str().is_empty() {
        return Err(BrokerError::File(
            "refusing to read the grant root itself".into(),
        ));
    }
    let (mut file, located, root_located) = open_beneath(&root, relative)?;
    if !located.starts_with(&root_located) {
        return Err(BrokerError::File(format!(
            "opened fd {} is outside the grant",
            located.display()
        )));
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|err| BrokerError::File(err.to_string()))?;
    if text.len() > 1024 * 1024 {
        return Err(BrokerError::File("file is larger than 1 MiB".into()));
    }
    let confidentiality = if under_any(&located, secret_roots) {
        Confidentiality::Secret
    } else if under_any(&located, private_roots) {
        Confidentiality::Private
    } else {
        Confidentiality::Public
    };
    Ok(MediatedFile {
        text,
        labels: Labels {
            integrity: Integrity::Trusted,
            confidentiality,
            origin: format!("file:{}", located.display()),
            owner: principal.id.clone(),
        },
        path: located,
    })
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, BrokerError> {
    if !path.is_absolute() {
        return Err(BrokerError::File(format!(
            "{} is not absolute",
            path.display()
        )));
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => out.push("/"),
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => {
                return Err(BrokerError::File(format!(
                    "refusing path {}",
                    path.display()
                )));
            }
        }
    }
    Ok(out)
}

fn grant_root(principal: &Principal, requested: &Path) -> Result<PathBuf, BrokerError> {
    let mut best: Option<PathBuf> = None;
    for grant in &principal.grants {
        let Some(root) = grant.strip_prefix("files.read:") else {
            continue;
        };
        if root.is_empty() {
            continue;
        }
        let root = lexical_absolute(Path::new(root.trim_end_matches('/')))?;
        if requested == root || requested.starts_with(&root) {
            let replace = best
                .as_ref()
                .map(|current| root.components().count() > current.components().count())
                .unwrap_or(true);
            if replace {
                best = Some(root);
            }
        }
    }
    best.ok_or_else(|| {
        BrokerError::File(format!(
            "resolved path {} is outside the grant",
            requested.display()
        ))
    })
}

#[allow(unsafe_code)]
fn open_beneath(root: &Path, relative: &Path) -> Result<(File, PathBuf, PathBuf), BrokerError> {
    use std::os::fd::AsRawFd;
    let root_c = c_string(root)?;
    let dirfd = unsafe {
        libc::open(
            root_c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if dirfd < 0 {
        return Err(BrokerError::File(format!(
            "cannot open grant root {}: {}",
            root.display(),
            std::io::Error::last_os_error()
        )));
    }
    let dir = unsafe { File::from_raw_fd(dirfd) };
    let root_located = readlink_fd(dir.as_raw_fd())?;
    let rel_c = c_string(relative)?;
    let mut how = OpenHow {
        flags: (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW) as u64,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS,
    };
    let opened = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dir.as_raw_fd(),
            rel_c.as_ptr(),
            &mut how as *mut OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if opened < 0 {
        return Err(BrokerError::File(format!(
            "openat2 {}: {}",
            relative.display(),
            std::io::Error::last_os_error()
        )));
    }
    let file_fd = opened as i32;
    let located = match readlink_fd(file_fd) {
        Ok(path) => path,
        Err(err) => {
            unsafe { libc::close(file_fd) };
            return Err(err);
        }
    };
    let file = unsafe { File::from_raw_fd(file_fd) };
    Ok((file, located, root_located))
}

fn readlink_fd(fd: i32) -> Result<PathBuf, BrokerError> {
    let link = fs::read_link(format!("/proc/self/fd/{fd}"))
        .map_err(|err| BrokerError::File(format!("cannot locate opened fd: {err}")))?;
    Ok(link)
}

fn c_string(path: &Path) -> Result<CString, BrokerError> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| BrokerError::File(format!("path {} contains a nul", path.display())))
}

fn under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let Ok(root) = fs::canonicalize(root) else {
            return false;
        };
        path == root || path.starts_with(&root)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dj_label::Integrity;
    use std::ffi::CString;

    fn principal() -> Principal {
        Principal {
            id: "agent-a".into(),
            trust_domain: "domain-a".into(),
            grants: BTreeSet::from([
                "email.send".to_string(),
                "model.use:default-chat".to_string(),
                "files.read:/workspace".to_string(),
            ]),
            local_only: false,
        }
    }

    fn broker(dir: &Path) -> (Broker, dj_audit::TestSigner) {
        let signer = dj_audit::spawn_test_signer().unwrap();
        let log = AuditLog::open(dir.join("audit.log"), &signer.paths).unwrap();
        (Broker::new(Policy::bundled(), log), signer)
    }

    #[test]
    fn tainted_email_is_prompted_and_denied() {
        let dir = tempfile::tempdir().unwrap();
        let (broker, _signer) = broker(dir.path());
        let labels = Labels {
            integrity: Integrity::Untrusted,
            origin: "web:page".into(),
            ..Labels::user("agent-a")
        };
        let authz = broker
            .authorize(
                &principal(),
                &ActionRequest {
                    action: "email.send".into(),
                    target: "attacker@example.com".into(),
                    labels,
                    arguments: json!({"to": "attacker@example.com"}),
                    route: None,
                },
            )
            .unwrap();
        assert!(!authz.allowed);
        assert!(authz.prompted);
        let records =
            dj_audit::read_records(&dir.path().join("audit.log"), &_signer.paths.verifying_key)
                .unwrap();
        let kinds: Vec<_> = records.iter().map(|r| r.kind.as_str()).collect();
        assert!(kinds.contains(&"approval_prompt"));
        assert!(kinds.contains(&"denial"));
    }

    #[test]
    fn private_root_label_is_not_chosen_by_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let private_dir = dir.path().join("private");
        fs::create_dir_all(&private_dir).unwrap();
        let file = private_dir.join("notes.txt");
        fs::write(&file, "CANARY_SECRET_DO_NOT_LEAK").unwrap();
        let outside = dir.path().join("link");
        std::os::unix::fs::symlink("/etc/hostname", &outside).unwrap();

        let mut who = principal();
        who.grants = BTreeSet::from([format!("files.read:{}", private_dir.display())]);
        let mediated =
            read_mediated_file(&who, &file, std::slice::from_ref(&private_dir), &[]).unwrap();
        assert_eq!(mediated.labels.confidentiality, Confidentiality::Private);
        assert!(mediated.text.contains("CANARY_SECRET_DO_NOT_LEAK"));

        let err = read_mediated_file(&who, &outside, &[private_dir], &[]).unwrap_err();
        assert!(err.to_string().contains("outside the grant"));
    }

    #[test]
    fn symlink_inside_the_grant_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let grant = dir.path().join("grant");
        fs::create_dir_all(&grant).unwrap();
        let secret = dir.path().join("secret.txt");
        fs::write(&secret, "SECRET_BYTES").unwrap();
        std::os::unix::fs::symlink(&secret, grant.join("alias.txt")).unwrap();
        let mut who = principal();
        who.grants = BTreeSet::from([format!("files.read:{}", grant.display())]);
        let err = read_mediated_file(&who, &grant.join("alias.txt"), &[], &[]).unwrap_err();
        assert!(!err.to_string().contains("SECRET_BYTES"), "{err}");
    }

    #[test]
    #[allow(unsafe_code)]
    fn rename_exchange_cannot_retarget_a_mediated_read() {
        let dir = tempfile::tempdir().unwrap();
        let grant = dir.path().join("grant");
        fs::create_dir_all(&grant).unwrap();
        let victim = grant.join("victim.txt");
        fs::write(&victim, "PUBLIC_OK").unwrap();
        let secret = dir.path().join("secret.txt");
        fs::write(&secret, "SECRET_BYTES").unwrap();
        let swap = dir.path().join("swap");
        std::os::unix::fs::symlink(&secret, &swap).unwrap();

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_thread = stop.clone();
        let victim_thread = victim.clone();
        let swap_thread = swap.clone();
        let exchanger = std::thread::spawn(move || {
            let old = CString::new(victim_thread.as_os_str().as_bytes()).unwrap();
            let new = CString::new(swap_thread.as_os_str().as_bytes()).unwrap();
            while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                unsafe {
                    libc::renameat2(
                        libc::AT_FDCWD,
                        old.as_ptr(),
                        libc::AT_FDCWD,
                        new.as_ptr(),
                        libc::RENAME_EXCHANGE,
                    );
                }
            }
        });

        let mut who = principal();
        who.grants = BTreeSet::from([format!("files.read:{}", grant.display())]);
        for _ in 0..200 {
            match read_mediated_file(&who, &victim, &[], &[]) {
                Ok(mediated) => {
                    assert_ne!(mediated.text, "SECRET_BYTES");
                    assert_eq!(mediated.labels.confidentiality, Confidentiality::Public);
                    assert_eq!(mediated.text, "PUBLIC_OK");
                }
                Err(err) => {
                    assert!(!err.to_string().contains("SECRET_BYTES"), "{err}");
                }
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        exchanger.join().unwrap();
    }
}
