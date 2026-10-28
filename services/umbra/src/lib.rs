//! Userspace UMBRA pieces: one shared model registry, a trust-domain cache,
//! a PSI reader, and a router whose hard rules run before the scorer.
//!
//! Nothing here runs in the kernel.

#![forbid(unsafe_code)]

pub mod cache;
pub mod engine;
pub mod psi;
pub mod registry;
pub mod route;

pub use cache::{Cache, CacheKey, CacheStats};
pub use engine::{Engine, EngineError, EngineRequest, EngineResponse, ToolCall};
pub use psi::{parse_pressure, PsiSnapshot};
pub use registry::Registry;
pub use route::{
    decide, AliasSpec, Location, ModelSpec, RemovedRoute, RouteDecision, RouteError, RouteInput,
    SizeClass,
};
