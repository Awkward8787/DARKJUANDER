//! Local API for UMBRA.
//!
//! The daemon listens on a Unix socket. The HTTP surface is a small
//! OpenAI-compatible subset plus DARKJUANDER tool and admin routes.
//! Anthropic messages and client streaming are planned. Cloud providers use
//! the OpenAI-compatible adapter in `umbra`; see docs/providers.md.

#![forbid(unsafe_code)]

mod config;
mod demo;
mod handlers;
mod state;

pub use config::FileConfig;
pub use demo::run_all;
pub use state::AppState;

use axum::Router;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::Request;
use hyper_util::rt::TokioIo;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use thiserror::Error;
use tokio::net::UnixListener;
use tower::ServiceExt;

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("socket: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn serve(state: AppState, socket: &Path) -> Result<(), ServeError> {
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if socket.exists() {
        std::fs::remove_file(socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    let _ = std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600));
    let app = router(state);
    loop {
        let (stream, _) = listener.accept().await?;
        let tower_service = app.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = hyper::service::service_fn(move |request: Request<Incoming>| {
                tower_service.clone().oneshot(request)
            });
            if let Err(err) = http1::Builder::new()
                .keep_alive(false)
                .serve_connection(io, service)
                .await
            {
                tracing::debug!(%err, "unix connection closed");
            }
        });
    }
}

fn router(state: AppState) -> Router {
    use axum::routing::{get, post};
    use handlers::{chat, health, models, psi_override, tool, top};
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/dj/top", get(top))
        .route("/v1/dj/tool", post(tool))
        .route("/v1/dj/admin/psi", post(psi_override))
        .with_state(state)
}
