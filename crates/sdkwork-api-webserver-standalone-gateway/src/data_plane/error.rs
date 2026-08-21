use std::{
    error::Error as StdError,
    io,
    net::{AddrParseError, SocketAddr},
    path::PathBuf,
};

use sdkwork_webserver_core::WebServerConfigError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataPlaneError {
    #[error(transparent)]
    Config(#[from] WebServerConfigError),

    #[error("invalid listener bind address {bind}: {source}")]
    InvalidBind {
        bind: String,
        source: AddrParseError,
    },

    #[error("cannot build upstream client {upstream_id}: {source}")]
    UpstreamClient {
        upstream_id: String,
        source: Box<dyn StdError + Send + Sync>,
    },

    #[error("cannot load {material} for upstream {upstream_id}: {source}")]
    UpstreamTls {
        upstream_id: String,
        material: &'static str,
        source: Box<dyn StdError + Send + Sync>,
    },

    #[error("upstream {upstream_id} CA bundle {path} contains no certificates")]
    EmptyUpstreamCaBundle { upstream_id: String, path: PathBuf },

    #[error("upstream {upstream_id} CA bundle {path} contains no valid PEM certificates")]
    InvalidUpstreamCaBundle { upstream_id: String, path: PathBuf },

    #[error("upstream {upstream_id} has {actual} custom root certificates; maximum is {maximum}")]
    TooManyUpstreamRootCertificates {
        upstream_id: String,
        actual: usize,
        maximum: usize,
    },

    #[error("upstream {upstream_id} has an invalid target URL {target}")]
    InvalidUpstreamTarget { upstream_id: String, target: String },

    #[error("listener {listener_id} references missing TLS policy {policy_id}")]
    MissingTlsPolicy {
        listener_id: String,
        policy_id: String,
    },

    #[error("TLS policy {policy_id} references missing certificate {certificate_id}")]
    MissingCertificate {
        policy_id: String,
        certificate_id: String,
    },

    #[error("certificate {certificate_id} has no resolved protected files")]
    MissingCertificateFiles { certificate_id: String },

    #[error("TLS policy {policy_id} has an ambiguous certificate mapping for {server_name}")]
    AmbiguousTlsServerName {
        policy_id: String,
        server_name: String,
    },

    #[error("cannot install a process-wide Rustls cryptography provider")]
    TlsCryptoProvider,

    #[error("cannot load TLS certificate {certificate_file} or key {private_key_file}: {source}")]
    TlsFiles {
        certificate_file: PathBuf,
        private_key_file: PathBuf,
        source: io::Error,
    },

    #[error("dynamic TLS runtime configuration is invalid: {detail}")]
    DynamicTlsConfiguration { detail: String },

    #[error("cannot fingerprint TLS material {path}: {source}")]
    TlsMaterialRead { path: PathBuf, source: io::Error },

    #[error("TLS material {path} is {actual_bytes} bytes; maximum is {maximum_bytes}")]
    TlsMaterialTooLarge {
        path: PathBuf,
        actual_bytes: u64,
        maximum_bytes: u64,
    },

    #[error("candidate configuration changes restart-only listener, TLS, or admission topology")]
    ReloadRequiresRestart,

    #[error("{subsystem} lifecycle coordinator is unavailable")]
    LifecycleCoordinatorUnavailable { subsystem: &'static str },

    #[error("configuration reload worker failed: {0}")]
    ReloadWorker(#[source] tokio::task::JoinError),

    #[error("active upstream health supervisor failed: {0}")]
    ActiveHealthTask(#[source] tokio::task::JoinError),

    #[error("resource pressure initial sample failed: {class}")]
    ResourcePressureInitialSample { class: &'static str },

    #[error("resource pressure reserve cannot fit within the effective {resource} ceiling")]
    ResourcePressureCapacity { resource: &'static str },

    #[error("resource pressure supervisor failed: {0}")]
    ResourcePressureTask(#[source] tokio::task::JoinError),

    #[error("WebSocket tunnel drain timed out with {active} active tunnels")]
    TunnelDrainTimeout { active: usize },

    #[error("listener {listener_id} failed: {source}")]
    Listener {
        listener_id: String,
        source: io::Error,
    },

    #[error("listener {listener_id} stopped before shutdown")]
    ListenerStopped { listener_id: String },

    #[error("data-plane operations listener {address} failed: {source}")]
    OperationsListener {
        address: SocketAddr,
        source: io::Error,
    },

    #[error("data-plane operations listener stopped before shutdown")]
    OperationsListenerStopped,

    #[error("provider-backed resource bootstrap failed: {0}")]
    ProviderBootstrap(#[source] Box<crate::website::WebsiteDataPlaneBootstrapError>),

    #[error("listener task failed: {0}")]
    ListenerTask(#[from] tokio::task::JoinError),
}
