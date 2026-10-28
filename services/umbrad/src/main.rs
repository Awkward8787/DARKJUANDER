//! `umbrad` serves the local model API and owns the shared model registry.

#![forbid(unsafe_code)]

use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(name = "umbrad", about = "DARKJUANDER UMBRA neural core")]
struct Args {
    /// YAML config. See examples/config/umbrad.yaml.
    #[arg(long, default_value = "/etc/darkjuander/umbrad.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    let cfg = match dj_gateway::FileConfig::from_path(&args.config) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("umbrad: {err}");
            return ExitCode::from(1);
        }
    };
    let socket = cfg.socket.clone();
    let state = match dj_gateway::AppState::from_config(cfg) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("umbrad: {err}");
            return ExitCode::from(1);
        }
    };
    tracing::info!(path = %socket.display(), "listening");
    if let Err(err) = dj_gateway::serve(state, &socket).await {
        eprintln!("umbrad: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
