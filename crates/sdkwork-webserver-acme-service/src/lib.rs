//! ACME certificate issuance (Let's Encrypt via instant-acme) and rcgen self-signed profiles.

mod account_store;
mod ari;
mod caa;
mod challenge_store;
mod config;
mod dns;
mod dns_account;
mod dns_aliyun;
mod dns_cloudflare;
mod dns_dnspod;
mod dns_http;
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
pub use challenge_store::ChallengeStore;
pub use config::{
    AcmeConfig, DEFAULT_ACME_OPERATION_TIMEOUT_MS, MAX_ACME_OPERATION_TIMEOUT_MS,
    MIN_ACME_OPERATION_TIMEOUT_MS,
};
pub use dns::{
    dns01_record_name, dns01_txt_value, dns_relative_record_name, normalize_dns_name,
    Dns01Presenter, Dns01RecordHandle, Dns01RecordRequest, DnsProviderKind, InMemoryDns01Presenter,
    ManualDns01Presenter, ACME_CHALLENGE_LABEL,
};
pub use dns_account::{
    DispatchingDns01Presenter, DnsCloudAccount, DnsCloudAccountConfig, DnsCloudAccountRegistry,
};
pub use dns_aliyun::{AliyunDns01Presenter, ALIYUN_DEFAULT_BASE_URL};
pub use dns_cloudflare::{CloudflareDns01Presenter, CLOUDFLARE_DEFAULT_BASE_URL};
pub use dns_dnspod::{DnspodDns01Presenter, DNSPOD_DEFAULT_BASE_URL};
pub use dns_http::DnsApiClient;
pub use dns_zone::{DnsZoneResolver, SingleZoneResolver};
pub use error::{AcmeServiceError, AcmeServiceResult};
pub use http_client::{
    AcmeHttpClientFactory, ExtraRootsClientFactory, PlatformVerifierClientFactory,
};
pub use issue::{AcmeDns01Context, CertificateIssuer, IssuerDns01Context};
pub use model::IssuedCertificateMaterial;
pub use revoke::CertificateRevocationReason;
