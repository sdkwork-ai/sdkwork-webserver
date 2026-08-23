//! SDKWork Web Server resolution cache component.
//!
//! A pluggable multi-layer cache for hostname/IP resolution, built to
//! nginx-grade standards: the resolution path is local-file →
//! in-process memory → Redis (distributed, cluster-ready) → database,
//! with negative caching and expiry at every layer.
//!
//! Architecture (high cohesion, low coupling):
//!
//! - [`ResolverCacheBackend`] — the plugin interface every cache layer
//!   implements (in-process [`memory::InMemoryResolverCache`], distributed
//!   [`redis::RedisResolverCache`] behind the `redis` feature, database via
//!   [`db::ResolutionDatabase`]).
//! - [`chain::ResolutionChain`] — the orchestrator that walks the layers,
//!   back-fills lower layers on hits, and applies negative caching when the
//!   upstream resolution fails.
//! - [`file::FileResolverSource`] — local file parsing (`/etc/hosts` style),
//!   the deployment seed surface.
//! - [`record::ResolvedRecord`] — the TTL-carrying value type.
//!
//! The component has no database or Redis hard dependency: layers are
//! optional by configuration and enabled features, so a bare deployment
//! runs on the file + memory layers alone.

pub mod backend;
pub mod chain;
pub mod config;
pub mod db;
pub mod file;
pub mod memory;
pub mod record;

#[cfg(feature = "redis")]
pub mod redis;

pub use backend::ResolverCacheBackend;
pub use chain::{ResolutionChain, ResolutionOutcome};
pub use config::{RedisCacheConfig, ResolutionCacheConfig};
pub use db::ResolutionDatabase;
pub use file::FileResolverSource;
pub use memory::InMemoryResolverCache;
pub use record::ResolvedRecord;
