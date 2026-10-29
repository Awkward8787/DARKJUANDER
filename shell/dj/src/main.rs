//! `dj` covers audit verification, the sandbox, and the demo scenes.
//! The control-center UI is planned.

#![forbid(unsafe_code)]

mod provider_cmd;

use clap::{Parser, Subcommand};
use dj_sandbox::Manifest;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "dj", about = "DARKJUANDER command line")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check the hash-chained audit log.
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    /// Print the bubblewrap plan for an agent manifest.
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommand,
    },
    /// Run the five UMBRA scenes against the mock engine.
    Demo,
    /// Store a cloud provider key. Routing stays off until `grant`.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ProviderCommand {
    /// Store an API key. Reads `--key-file` or stdin. The key is not a flag.
    Add {
        provider: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        key_file: Option<PathBuf>,
        /// Record a systemd LoadCredential name instead of a direct file read.
        #[arg(long)]
        systemd: bool,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Show providers. Keys are not printed.
    List {
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Delete a provider and shred its local credential.
    Remove {
        provider: String,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Explicitly allow UMBRA to consider this provider.
    Grant {
        provider: String,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Turn a provider grant off.
    Revoke {
        provider: String,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Send a fixed public "ping" to the provider. Do not put private text here.
    Test {
        provider: String,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Official subscription sign-in, when the provider documents one.
    Login {
        provider: String,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
    /// Set the provider model id used by `test` and by registry injection.
    Model {
        provider: String,
        model: String,
        #[arg(long)]
        state_dir: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    /// Exit 0 when every line matches the chain and the signed checkpoint.
    Verify {
        path: PathBuf,
        /// Published ed25519 verifying key. Defaults to /run/dj-audit/audit.ed25519.pub.
        /// A durable copy for offline verify is /var/lib/dj-audit/audit.ed25519.pub.
        #[arg(long)]
        verifying_key: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum SandboxCommand {
    /// Print the confinement plan. Does not start the agent.
    Plan {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Run a command inside bubblewrap, Landlock, and the seccomp denylist.
    Run {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Internal step used by `dj sandbox run`. Applies Landlock and seccomp.
    Confine {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("dj: {err}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<ExitCode, String> {
    let args = Args::parse();
    match args.command {
        Command::Audit { command } => match command {
            AuditCommand::Verify {
                path,
                verifying_key,
            } => {
                let verifying_key =
                    verifying_key.unwrap_or_else(|| PathBuf::from(dj_audit::DEFAULT_VERIFYING_KEY));
                let report =
                    dj_audit::verify_file(&path, &verifying_key).map_err(|err| err.to_string())?;
                println!("audit ok: {} entries, head {}", report.entries, report.head);
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Sandbox { command } => match command {
            SandboxCommand::Plan { manifest, command } => {
                let manifest = Manifest::from_path(&manifest).map_err(|err| err.to_string())?;
                let plan = dj_sandbox::plan(
                    &manifest,
                    &std::env::current_exe().map_err(|err| err.to_string())?,
                    &command,
                )
                .map_err(|err| err.to_string())?;
                println!("bwrap {}", plan.args.join(" "));
                println!(
                    "cgroup intent: {} memory.max={} pids.max={} (applied only if the host cgroup fs accepts the write)",
                    plan.slice, plan.memory_max, plan.pids_max
                );
                println!(
                    "seccomp denylist: {}",
                    dj_sandbox::DENIED_SYSCALLS.join(", ")
                );
                let applied = dj_sandbox::try_apply_cgroup(&manifest);
                println!("cgroup applied now: {applied}");
                Ok(ExitCode::SUCCESS)
            }
            SandboxCommand::Run { manifest, command } => {
                if command.is_empty() {
                    return Err("dj sandbox run needs a command after the manifest".into());
                }
                let manifest = Manifest::from_path(&manifest).map_err(|err| err.to_string())?;
                let cgroup = dj_sandbox::try_apply_cgroup(&manifest);
                eprintln!(
                    "dj sandbox: namespaces via bwrap; cgroup applied={cgroup}; landlock and seccomp run in the child"
                );
                let status = dj_sandbox::run(&manifest, &command).map_err(|err| err.to_string())?;
                Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
            }
            SandboxCommand::Confine { args } => {
                dj_sandbox::confine_and_exec(&args).map_err(|err| err.to_string())?;
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Demo => {
            let report = dj_gateway::run_all()?;
            print!("{report}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Provider { command } => match command {
            ProviderCommand::Add {
                provider,
                model,
                key_file,
                systemd,
                state_dir,
            } => provider_cmd::add(
                &provider_cmd::state_dir(state_dir),
                &provider,
                key_file.as_deref(),
                &model,
                systemd,
            ),
            ProviderCommand::List { state_dir } => {
                provider_cmd::list(&provider_cmd::state_dir(state_dir))
            }
            ProviderCommand::Remove {
                provider,
                state_dir,
            } => provider_cmd::remove(&provider_cmd::state_dir(state_dir), &provider),
            ProviderCommand::Grant {
                provider,
                state_dir,
            } => provider_cmd::grant(&provider_cmd::state_dir(state_dir), &provider, true),
            ProviderCommand::Revoke {
                provider,
                state_dir,
            } => provider_cmd::grant(&provider_cmd::state_dir(state_dir), &provider, false),
            ProviderCommand::Test {
                provider,
                base_url,
                state_dir,
            } => provider_cmd::test(
                &provider_cmd::state_dir(state_dir),
                &provider,
                base_url.as_deref(),
            ),
            ProviderCommand::Login {
                provider,
                state_dir,
            } => provider_cmd::login(&provider_cmd::state_dir(state_dir), &provider),
            ProviderCommand::Model {
                provider,
                model,
                state_dir,
            } => provider_cmd::model(&provider_cmd::state_dir(state_dir), &provider, &model),
        }
        .map(|()| ExitCode::SUCCESS),
    }
}
