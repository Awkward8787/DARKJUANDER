//! `dj` covers audit verification, the sandbox, and the demo scenes.
//! The control-center UI is planned.

#![forbid(unsafe_code)]

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
}

#[derive(Debug, Subcommand)]
enum AuditCommand {
    /// Exit 0 when every line matches the chain.
    Verify { path: PathBuf },
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
            AuditCommand::Verify { path } => {
                let report = dj_audit::verify_file(&path).map_err(|err| err.to_string())?;
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
    }
}
