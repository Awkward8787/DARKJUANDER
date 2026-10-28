//! Signing service for the audit checkpoint.
//!
//! Runs as its own user. The seed stays in that user's state directory.
//! `umbrad` sends the appended record over the unix socket and never sees the seed.

use dj_audit::{
    expected_writer_uid, serve_signer as serve, DEFAULT_SIGNER_SOCKET, DEFAULT_SIGNING_KEY,
    DEFAULT_VERIFYING_KEY,
};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("dj-auditd: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut key = PathBuf::from(DEFAULT_SIGNING_KEY);
    let mut public = PathBuf::from(DEFAULT_VERIFYING_KEY);
    let mut socket = PathBuf::from(DEFAULT_SIGNER_SOCKET);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--key" => key = PathBuf::from(args.next().ok_or("missing --key value")?),
            "--public" => public = PathBuf::from(args.next().ok_or("missing --public value")?),
            "--socket" => socket = PathBuf::from(args.next().ok_or("missing --socket value")?),
            "--help" | "-h" => {
                println!(
                    "dj-auditd [--key PATH] [--public PATH] [--socket PATH]\n\
                     defaults: {DEFAULT_SIGNING_KEY} {DEFAULT_VERIFYING_KEY} {DEFAULT_SIGNER_SOCKET}"
                );
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    let shutdown = AtomicBool::new(false);
    serve(&key, &public, &socket, &shutdown, expected_writer_uid())?;
    Ok(())
}
