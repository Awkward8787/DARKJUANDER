//! Agent sandbox planner.
//!
//! `bubblewrap` provides the namespaces. This crate builds that command, then
//! a confine step applies Landlock and a seccomp denylist before `exec`.
//! The denylist is not an allowlist. Moving agents to an allowlist is tracked
//! as SB-2 in `docs/ROADMAP.md`.
//! If `bwrap` is missing, [`run`] refuses to start the command. It does not
//! fall back to an unsandboxed process.
//!
//! cgroup limits and SELinux domains are recorded as intent. This slice does
//! not claim they are enforced unless the host actually accepted the write
//! or `runcon` exists, and the shipped SELinux module is still a draft.

use landlock::{
    path_beneath_rules, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
    ABI,
};
use serde::Deserialize;
use std::fs;
use std::io::Error as IoError;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use thiserror::Error;

pub const DENIED_SYSCALLS: &[&str] = &[
    "ptrace",
    "init_module",
    "finit_module",
    "delete_module",
    "kexec_file_load",
    "open_by_handle_at",
    "bpf",
    "perf_event_open",
    "mount",
    "umount2",
    "swapon",
    "reboot",
];

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox io: {0}")]
    Io(#[from] IoError),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("{0}")]
    Message(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub trust_domain: String,
    pub workspace: PathBuf,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub api_socket: Option<PathBuf>,
    #[serde(default = "default_memory")]
    pub memory_max: String,
    #[serde(default = "default_pids")]
    pub pids_max: u32,
}

fn default_memory() -> String {
    "1G".into()
}

fn default_pids() -> u32 {
    64
}

impl Manifest {
    pub fn from_path(path: &Path) -> Result<Self, SandboxError> {
        let text = fs::read_to_string(path)?;
        let manifest: Self =
            toml::from_str(&text).map_err(|err| SandboxError::Manifest(err.to_string()))?;
        if manifest.name.is_empty() || manifest.publisher.is_empty() || manifest.version.is_empty()
        {
            return Err(SandboxError::Manifest(
                "name, version, and publisher are required".into(),
            ));
        }
        if manifest.capabilities.is_empty() {
            return Err(SandboxError::Manifest(
                "an agent manifest must declare capabilities".into(),
            ));
        }
        Ok(manifest)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxPlan {
    /// Arguments after the `bwrap` binary, including the confined command.
    pub args: Vec<String>,
    pub memory_max: String,
    pub pids_max: u32,
    pub slice: String,
}

pub const AGENT_ROOT: &str = "/var/lib/darkjuander/agents";

/// The workspace must be exactly `{AGENT_ROOT}/{name}`. `/`, system prefixes,
/// `..`, and symlinks are refused. `plan` and `run` both call this.
pub fn validate_workspace(manifest: &Manifest) -> Result<PathBuf, SandboxError> {
    validate_workspace_under(Path::new(AGENT_ROOT), manifest)
}

pub fn validate_workspace_under(root: &Path, manifest: &Manifest) -> Result<PathBuf, SandboxError> {
    if !agent_name_ok(&manifest.name) {
        return Err(SandboxError::Message(format!(
            "refusing agent name {}",
            manifest.name
        )));
    }
    let expected = lexical_normal(&root.join(&manifest.name))?;
    let given = lexical_normal(&manifest.workspace)?;
    if given != expected {
        return Err(SandboxError::Message(format!(
            "refusing workspace {}: an agent is pinned to {}",
            manifest.workspace.display(),
            expected.display()
        )));
    }
    refuse_symlinks(&expected)?;
    Ok(expected)
}

fn agent_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    name.len() <= 64
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-')
        && name != "."
        && name != ".."
}

fn lexical_normal(path: &Path) -> Result<PathBuf, SandboxError> {
    if !path.is_absolute() {
        return Err(SandboxError::Message(format!(
            "workspace {} is not absolute",
            path.display()
        )));
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir => out.push("/"),
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir | std::path::Component::Prefix(_) => {
                return Err(SandboxError::Message(format!(
                    "refusing workspace {}",
                    path.display()
                )));
            }
        }
    }
    Ok(out)
}

fn refuse_symlinks(path: &Path) -> Result<(), SandboxError> {
    let mut walked = PathBuf::new();
    for component in path.components() {
        walked.push(component);
        if !walked.exists() {
            continue;
        }
        let meta = fs::symlink_metadata(&walked)?;
        if meta.file_type().is_symlink() {
            return Err(SandboxError::Message(format!(
                "refusing symlink at {}",
                walked.display()
            )));
        }
    }
    Ok(())
}

pub fn plan(
    manifest: &Manifest,
    exe: &Path,
    command: &[String],
) -> Result<SandboxPlan, SandboxError> {
    let workspace = validate_workspace(manifest)?;
    let mut args = vec![
        "--unshare-user".into(),
        "--unshare-pid".into(),
        "--unshare-net".into(),
        "--unshare-ipc".into(),
        "--unshare-uts".into(),
        "--unshare-cgroup".into(),
        "--die-with-parent".into(),
        "--new-session".into(),
        "--hostname".into(),
        format!("dj-{}", manifest.name),
        "--cap-drop".into(),
        "ALL".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--tmpfs".into(),
        "/tmp".into(),
    ];
    for path in ["/usr", "/lib", "/lib64", "/bin"] {
        if Path::new(path).is_dir() {
            args.push("--ro-bind".into());
            args.push(path.into());
            args.push(path.into());
        }
    }
    let workspace = workspace.display().to_string();
    args.extend([
        "--bind".into(),
        workspace.clone(),
        workspace.clone(),
        "--chdir".into(),
        workspace.clone(),
        "--clearenv".into(),
        "--setenv".into(),
        "PATH".into(),
        "/usr/bin:/bin".into(),
        "--setenv".into(),
        "HOME".into(),
        workspace.clone(),
        "--setenv".into(),
        "DJ_AGENT".into(),
        manifest.name.clone(),
        "--setenv".into(),
        "DJ_TRUST_DOMAIN".into(),
        manifest.trust_domain.clone(),
    ]);
    if let Some(socket) = &manifest.api_socket {
        let socket = socket.display().to_string();
        args.extend(["--ro-bind".into(), socket.clone(), socket]);
    }
    args.push("--".into());
    args.push(exe.display().to_string());
    args.push("sandbox".into());
    args.push("confine".into());
    args.push("--workspace".into());
    args.push(workspace.clone());
    for path in ["/usr", "/lib", "/lib64", "/bin"] {
        if Path::new(path).is_dir() {
            args.push("--ro".into());
            args.push(path.into());
        }
    }
    args.push("--rw".into());
    args.push(workspace.clone());
    args.push("--rw".into());
    args.push("/tmp".into());
    args.push("--".into());
    args.extend(command.iter().cloned());
    Ok(SandboxPlan {
        args,
        memory_max: manifest.memory_max.clone(),
        pids_max: manifest.pids_max,
        slice: format!("dj-agent.slice/dj-agents-{}.scope", manifest.name),
    })
}

pub fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

/// Try to put the agent in a cgroup under `/sys/fs/cgroup/dj-agents`.
/// Returns false when the kernel will not let this user create it.
pub fn try_apply_cgroup(manifest: &Manifest) -> bool {
    let base = Path::new("/sys/fs/cgroup/dj-agent.slice")
        .join(format!("dj-agents-{}.scope", manifest.name));
    if fs::create_dir_all(&base).is_err() {
        return false;
    }
    let memory = base.join("memory.max");
    let pids = base.join("pids.max");
    if fs::write(&memory, &manifest.memory_max).is_err() {
        return false;
    }
    fs::write(&pids, manifest.pids_max.to_string()).is_ok()
}

pub fn run(manifest: &Manifest, command: &[String]) -> Result<ExitStatus, SandboxError> {
    let bwrap = which("bwrap").ok_or_else(|| {
        SandboxError::Message(
            "bwrap is not installed; refusing to run the agent without a sandbox".into(),
        )
    })?;
    let workspace = validate_workspace(manifest)?;
    fs::create_dir_all(&workspace)?;
    let _cgroup_applied = try_apply_cgroup(manifest);
    let exe = std::env::current_exe()?;
    let plan = plan(manifest, &exe, command)?;
    let status = Command::new(bwrap).args(&plan.args).status()?;
    Ok(status)
}

#[derive(Debug)]
struct ConfineArgs {
    read_only: Vec<PathBuf>,
    read_write: Vec<PathBuf>,
    command: Vec<String>,
}

fn parse_confine(args: &[String]) -> Result<ConfineArgs, SandboxError> {
    let mut read_only = Vec::new();
    let mut read_write = Vec::new();
    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--workspace" | "--ro" => {
                idx += 1;
                let path = args
                    .get(idx)
                    .ok_or_else(|| SandboxError::Message("confine path argument missing".into()))?;
                read_only.push(PathBuf::from(path));
                if args[idx - 1] == "--workspace" {
                    read_write.push(PathBuf::from(path));
                }
            }
            "--rw" => {
                idx += 1;
                let path = args
                    .get(idx)
                    .ok_or_else(|| SandboxError::Message("confine path argument missing".into()))?;
                read_write.push(PathBuf::from(path));
            }
            "--" => {
                return Ok(ConfineArgs {
                    read_only,
                    read_write,
                    command: args[idx + 1..].to_vec(),
                });
            }
            other => {
                return Err(SandboxError::Message(format!(
                    "unknown confine argument {other}"
                )));
            }
        }
        idx += 1;
    }
    Err(SandboxError::Message("confine command is missing".into()))
}

/// Apply Landlock and seccomp, then replace this process with the command.
pub fn confine_and_exec(args: &[String]) -> Result<(), SandboxError> {
    let parsed = parse_confine(args)?;
    if parsed.command.is_empty() {
        return Err(SandboxError::Message("confine command is empty".into()));
    }
    apply_landlock(&parsed.read_only, &parsed.read_write)?;
    install_seccomp()?;
    let mut command = Command::new(&parsed.command[0]);
    if parsed.command.len() > 1 {
        command.args(&parsed.command[1..]);
    }
    let err = command.exec();
    Err(SandboxError::Io(err))
}

fn apply_landlock(read_only: &[PathBuf], read_write: &[PathBuf]) -> Result<(), SandboxError> {
    let abi = ABI::V3;
    let ro: Vec<_> = read_only
        .iter()
        .filter(|path| path.exists())
        .map(|path| path.as_path())
        .collect();
    let rw: Vec<_> = read_write
        .iter()
        .filter(|path| path.exists())
        .map(|path| path.as_path())
        .collect();
    let status = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|err| SandboxError::Message(err.to_string()))?
        .create()
        .map_err(|err| SandboxError::Message(err.to_string()))?
        .add_rules(path_beneath_rules(&ro, AccessFs::from_read(abi)))
        .map_err(|err| SandboxError::Message(err.to_string()))?
        .add_rules(path_beneath_rules(&rw, AccessFs::from_all(abi)))
        .map_err(|err| SandboxError::Message(err.to_string()))?
        .restrict_self()
        .map_err(|err| SandboxError::Message(err.to_string()))?;
    if status.ruleset == RulesetStatus::NotEnforced {
        return Err(SandboxError::Message(
            "Landlock is not enforced on this kernel; refusing to start the agent".into(),
        ));
    }
    Ok(())
}

fn install_seccomp() -> Result<(), SandboxError> {
    let blocked = blocked_syscall_numbers();
    let mut filters: Vec<libc::sock_filter> = Vec::new();
    let count = blocked.len();
    filters.push(bpf_stmt(BPF_LD | BPF_W | BPF_ABS, 0));
    for (index, number) in blocked.iter().copied().enumerate() {
        let skip = u8::try_from(count - index).unwrap_or(255);
        filters.push(bpf_jump(BPF_JMP | BPF_JEQ | BPF_K, number as u32, skip, 0));
    }
    filters.push(bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_ALLOW));
    filters.push(bpf_stmt(BPF_RET | BPF_K, SECCOMP_RET_KILL_PROCESS));

    let mut prog = libc::sock_fprog {
        len: u16::try_from(filters.len()).unwrap_or(u16::MAX),
        filter: filters.as_mut_ptr(),
    };
    let no_new_privs = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if no_new_privs != 0 {
        return Err(SandboxError::Io(IoError::last_os_error()));
    }
    let installed = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            SECCOMP_MODE_FILTER,
            &mut prog as *mut libc::sock_fprog,
        )
    };
    if installed != 0 {
        return Err(SandboxError::Io(IoError::other(IoError::last_os_error())));
    }
    Ok(())
}

fn blocked_syscall_numbers() -> Vec<libc::c_long> {
    let mut numbers = vec![
        libc::SYS_ptrace,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_kexec_file_load,
        libc::SYS_open_by_handle_at,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_swapon,
        libc::SYS_reboot,
    ];
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        numbers.push(libc::SYS_iopl);
        numbers.push(libc::SYS_ioperm);
        numbers.push(libc::SYS_kexec_load);
    }
    numbers
}

const BPF_LD: u16 = 0x00;
const BPF_W: u16 = 0x00;
const BPF_ABS: u16 = 0x20;
const BPF_JMP: u16 = 0x05;
const BPF_JEQ: u16 = 0x10;
const BPF_K: u16 = 0x00;
const BPF_RET: u16 = 0x06;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_MODE_FILTER: libc::c_ulong = 2;

fn bpf_stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}

fn bpf_jump(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(tmp: &Path) -> Manifest {
        Manifest {
            name: "research".into(),
            version: "0.1.0".into(),
            publisher: "darkjuander.local".into(),
            trust_domain: "research".into(),
            workspace: PathBuf::from("/var/lib/darkjuander/agents/research"),
            capabilities: vec!["model.use:default-chat".into()],
            api_socket: Some(tmp.join("umbrad.sock")),
            memory_max: "512M".into(),
            pids_max: 32,
        }
    }

    #[test]
    fn plan_drops_network_and_does_not_bind_home() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = plan(
            &manifest(tmp.path()),
            Path::new("/usr/bin/dj"),
            &["/bin/true".into()],
        )
        .unwrap();
        let text = plan.args.join(" ");
        assert!(text.contains("--unshare-net"));
        assert!(text.contains("--unshare-user"));
        assert!(text.contains("--unshare-pid"));
        assert!(text.contains("--cap-drop ALL"));
        assert!(!text.contains("--bind /home"));
        assert!(text.contains("sandbox confine"));
        assert!(text.contains("/bin/true"));
        assert!(text.contains("umbrad.sock"));
        assert_eq!(plan.pids_max, 32);
    }

    #[test]
    fn manifest_requires_capabilities() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("agent.toml");
        std::fs::write(
            &path,
            "name='a'\nversion='0.1.0'\npublisher='p'\ntrust_domain='t'\nworkspace='/tmp/w'\ncapabilities=[]\n",
        )
        .unwrap();
        assert!(Manifest::from_path(&path).is_err());
    }

    #[test]
    fn root_workspace_is_rejected() {
        let mut manifest = manifest(Path::new("/tmp"));
        manifest.workspace = PathBuf::from("/");
        let err = plan(&manifest, Path::new("/usr/bin/dj"), &["/bin/true".into()]).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("refusing workspace"), "{text}");
        assert!(!text.contains("--bind / /"));
    }

    #[test]
    fn symlink_agent_root_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("agents");
        fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink("/", root.join("evil")).unwrap();
        let manifest = Manifest {
            name: "evil".into(),
            version: "0.1.0".into(),
            publisher: "darkjuander.local".into(),
            trust_domain: "evil".into(),
            workspace: root.join("evil"),
            capabilities: vec!["model.use:default-chat".into()],
            api_socket: None,
            memory_max: "1G".into(),
            pids_max: 8,
        };
        let err = validate_workspace_under(&root, &manifest).unwrap_err();
        assert!(err.to_string().contains("symlink"), "{err}");
    }

    #[test]
    fn denylist_covers_module_and_ptrace() {
        assert!(DENIED_SYSCALLS.contains(&"ptrace"));
        assert!(DENIED_SYSCALLS.contains(&"init_module"));
        assert!(DENIED_SYSCALLS.contains(&"bpf"));
    }
}
