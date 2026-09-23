//! ACME certificate issuance (Let's Encrypt via instant-acme) and rcgen self-signed profiles.

mod account_store;
mod ari;
mod caa;
mod challenge_policy;
mod challenge_store;
mod config;
mod dns;
mod dns_account;
mod dns_aliyun;
mod dns_cloudflare;
mod dns_dnspod;
mod dns_http;
mod dns_http_request;
pub mod dns_zone;
mod error;
mod http_client;
mod issue;
mod lets_encrypt;
mod model;
mod revoke;
mod self_signed;

pub use account_store::{AcmeAccountStore, EncryptedFileAcmeAccountStore, MemoryAcmeAccountStore};
pub use ari::AriRenewalWindow;
pub use caa::{acme_directory_profile, AcmeDirectoryProfile};
pub use challenge_policy::{
    contains_wildcard_identifier, resolve_challenge_method, ChallengeAvailability,
    DeclaredChallengeMethod, ResolvedChallenge, CHALLENGE_METHOD_AUTO, CHALLENGE_METHOD_DNS_01,
    CHALLENGE_METHOD_HTTP_01,
};
pub use challenge_store::ChallengeStore;
pub use config::{
    AcmeConfig, DEFAULT_ACME_OPERATION_TIMEOUT_MS, MAX_ACME_OPERATION_TIMEOUT_MS,
    MIN_ACME_OPERATION_TIMEOUT_MS,
};
pub use dns::{
    dns01_record_name, dns01_txt_value, dns_relative_record_name, normalize_dns_name,
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsAccountVerification, DnsProviderKind,
    InMemoryDns01Presenter, ManualDns01Presenter, ACME_CHALLENGE_LABEL,
};
pub use dns_account::{
    load_dns_account_configs, DispatchingDns01Presenter, DnsAccountVerificationReport,
    DnsCloudAccount, DnsCloudAccountConfig, DnsCloudAccountRegistry, MAX_DNS_ACCOUNTS_FILE_BYTES,
};
pub use dns_aliyun::{AliyunDns01Presenter, ALIYUN_DEFAULT_BASE_URL};
pub use dns_cloudflare::{CloudflareDns01Presenter, CLOUDFLARE_DEFAULT_BASE_URL};
pub use dns_dnspod::{DnspodDns01Presenter, DNSPOD_DEFAULT_BASE_URL};
pub use dns_http::DnsApiClient;
pub use dns_http_request::{HttpRequestDns01Presenter, MAX_HTTP_REQUEST_CONFIG_BYTES};
pub use dns_zone::{DnsZoneResolver, SingleZoneResolver};
pub use error::{AcmeServiceError, AcmeServiceResult};
pub use http_client::{
    AcmeHttpClientFactory, ExtraRootsClientFactory, PlatformVerifierClientFactory,
};
pub use issue::{AcmeDns01Context, CertificateIssuer, ChallengePlan, IssuerDns01Context};
pub use model::IssuedCertificateMaterial;
pub use revoke::CertificateRevocationReason;

/// Certificate identifier ceiling, shared by the engine and its callers.
pub use issue::MAX_CERTIFICATE_IDENTIFIERS;
