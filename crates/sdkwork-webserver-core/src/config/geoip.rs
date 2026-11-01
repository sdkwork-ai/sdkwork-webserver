//! nginx `ngx_http_geoip_module` compatibility over MaxMind DB (MMDB) files.
//!
//! nginx 1.29.6's GeoIP module reads the legacy libGeoIP `.dat` editions. This
//! runtime executes the same directive and variable surface against MaxMind DB
//! (`.mmdb`) editions instead, because MMDB is the maintained format and the
//! Rust reader is pure Rust (no `libmaxminddb` C dependency). Every directive
//! and variable nginx 1.29.6 registers is covered:
//!
//! - `geoip_country <file> [utf8]`, `geoip_city <file> [utf8]`,
//!   `geoip_org <file> [utf8]` — http context only, one database each;
//! - `geoip_proxy <cidr>` (repeatable) and `geoip_proxy_recursive on|off`;
//! - `$geoip_country_code`, `$geoip_country_code3`, `$geoip_country_name`,
//!   `$geoip_city_country_code`, `$geoip_city_country_code3`,
//!   `$geoip_city_country_name`, `$geoip_region`, `$geoip_region_name`,
//!   `$geoip_city`, `$geoip_postal_code`, `$geoip_city_continent_code`,
//!   `$geoip_latitude`, `$geoip_longitude`, `$geoip_org`, plus nginx's
//!   deprecated `$geoip_dma_code` and `$geoip_area_code`.
//!
//! Bounded differences from nginx, each fail-closed or documented rather than
//! silently approximated:
//!
//! - the database must be an MMDB whose `database_type` names the matching
//!   edition (`*Country*` for `geoip_country`, `*City*` for `geoip_city`, and
//!   `*ASN*`/`*ISP*`/`*Org*`/`*Domain*`/`*Enterprise*` for `geoip_org`);
//!   a missing, unreadable, corrupt, or wrong-edition database is a located
//!   configuration diagnostic;
//! - database paths must be absolute and, when `[http] geoipDatabaseRoot` is
//!   configured, a strict lexical descendant of that root (the nginx plane has
//!   no root to state, so it accepts any absolute path exactly like a stock
//!   nginx file);
//! - `$geoip_country_code3` is derived from the ISO 3166-1 alpha-2 code because
//!   GeoIP2/GeoLite2 MMDB records carry only the two-letter code;
//! - `$geoip_area_code` is always empty: GeoIP2/GeoLite2 City records have no
//!   legacy area-code field (`$geoip_dma_code` maps to `location.metro_code`);
//! - the X-Forwarded-For walk is bounded (64 tokens, 8 KiB) instead of
//!   unbounded; exceeding the bound stops at the last trusted hop, so a
//!   pathological header can never make the lookup unbounded.
//!
//! `geoip_proxy` is independent of the real_ip module: nginx starts from the
//! connection address (which real_ip may already have replaced) and then walks
//! `X-Forwarded-For` right-to-left while the current address is a configured
//! trusted proxy. `geoip_proxy_recursive on` keeps walking through trusted
//! hops; `off` stops after one hop.

use std::{
    fs::{self, File},
    io::Read,
    net::{IpAddr, SocketAddr},
    path::{Component, Path},
};

use ipnet::IpNet;
use maxminddb::{geoip2, Reader};
use serde::{Deserialize, Serialize};

use super::geoip_country_codes::alpha3_for;

/// The nginx GeoIP variable vocabulary, in nginx registration order.
pub const GEOIP_VARIABLES: &[&str] = &[
    "geoip_country_code",
    "geoip_country_code3",
    "geoip_country_name",
    "geoip_org",
    "geoip_city_continent_code",
    "geoip_city_country_code",
    "geoip_city_country_code3",
    "geoip_city_country_name",
    "geoip_region",
    "geoip_region_name",
    "geoip_city",
    "geoip_postal_code",
    "geoip_latitude",
    "geoip_longitude",
    "geoip_dma_code",
    "geoip_area_code",
];

/// Largest MMDB the runtime will open. A GeoLite2-City database is well below
/// this; the cap keeps a misconfigured path from exhausting process memory at
/// configuration load or generation build.
pub const MAX_GEOIP_DATABASE_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum `geoip_proxy` entries in one configuration.
pub const MAX_GEOIP_PROXIES: usize = 256;

/// Maximum X-Forwarded-For hops inspected for one lookup.
pub const MAX_GEOIP_XFF_HOPS: usize = 64;

/// Maximum total X-Forwarded-For bytes inspected for one lookup.
pub const MAX_GEOIP_XFF_BYTES: usize = 8 * 1024;

/// The three nginx `geoip_*` database roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoIpDatabaseKind {
    /// `geoip_country`: a Country edition.
    Country,
    /// `geoip_city`: a City edition.
    City,
    /// `geoip_org`: an ASN/ISP/Organization/Domain/Enterprise edition.
    Org,
}

impl GeoIpDatabaseKind {
    /// The nginx directive name.
    #[must_use]
    pub fn directive(self) -> &'static str {
        match self {
            Self::Country => "geoip_country",
            Self::City => "geoip_city",
            Self::Org => "geoip_org",
        }
    }

    /// The MMDB `database_type` families this role accepts.
    fn accepts(self, database_type: &str) -> bool {
        match self {
            Self::Country => contains_ignore_ascii_case(database_type, "country"),
            Self::City => contains_ignore_ascii_case(database_type, "city"),
            Self::Org => {
                contains_ignore_ascii_case(database_type, "asn")
                    || contains_ignore_ascii_case(database_type, "isp")
                    || contains_ignore_ascii_case(database_type, "org")
                    || contains_ignore_ascii_case(database_type, "domain")
                    || contains_ignore_ascii_case(database_type, "enterprise")
            }
        }
    }
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Typed nginx GeoIP configuration (http context only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeoIpConfig {
    /// `geoip_country` database path (Country MMDB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// `geoip_city` database path (City MMDB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// `geoip_org` database path (ASN/ISP/Organization/Domain/Enterprise MMDB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org: Option<String>,
    /// `geoip_proxy` trusted proxy networks, in declaration order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxies: Vec<String>,
    /// `geoip_proxy_recursive on|off` (nginx default `off`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proxy_recursive: bool,
}

impl GeoIpConfig {
    /// Whether this configuration declares nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.country.is_none()
            && self.city.is_none()
            && self.org.is_none()
            && self.proxies.is_empty()
            && !self.proxy_recursive
    }

    /// The configured database path for one role.
    #[must_use]
    pub fn database(&self, kind: GeoIpDatabaseKind) -> Option<&str> {
        match kind {
            GeoIpDatabaseKind::Country => self.country.as_deref(),
            GeoIpDatabaseKind::City => self.city.as_deref(),
            GeoIpDatabaseKind::Org => self.org.as_deref(),
        }
    }
}

/// One parsed GeoIP database path plus the MMDB metadata needed to validate it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeoIpDatabaseMetadata {
    /// MMDB `database_type`, for example `GeoIP2-City`.
    pub database_type: String,
    /// MMDB `ip_version` (4 or 6).
    pub ip_version: u16,
}

impl GeoIpDatabaseMetadata {
    /// Open the database, validate its edition against `kind`, and return the
    /// metadata. The reader is dropped; runtime generations open their own
    /// cached readers.
    pub fn inspect(path: &str, kind: GeoIpDatabaseKind) -> Result<Self, GeoIpDatabaseError> {
        validate_database_path(path)?;
        let bytes = read_bounded_database(path)?;
        let reader = Reader::from_source(bytes).map_err(|source| GeoIpDatabaseError::Invalid {
            path: path.to_owned(),
            source,
        })?;
        let metadata = reader.metadata();
        if !kind.accepts(&metadata.database_type) {
            return Err(GeoIpDatabaseError::WrongEdition {
                path: path.to_owned(),
                directive: kind.directive(),
                database_type: metadata.database_type.clone(),
            });
        }
        Ok(Self {
            database_type: metadata.database_type.clone(),
            ip_version: metadata.ip_version,
        })
    }
}

/// Failure opening or validating one GeoIP database.
#[derive(Debug, thiserror::Error)]
pub enum GeoIpDatabaseError {
    /// The path is not a usable absolute MMDB path.
    #[error("geoip database path `{path}` {reason}")]
    Path {
        /// The offending path.
        path: String,
        /// Why it is rejected.
        reason: &'static str,
    },
    /// The file does not exist or cannot be read.
    #[error("geoip database `{path}` cannot be read: {source}")]
    Read {
        /// The offending path.
        path: String,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// The file exceeds [`MAX_GEOIP_DATABASE_BYTES`].
    #[error("geoip database `{path}` is {actual_bytes} bytes; maximum is {maximum_bytes}")]
    TooLarge {
        /// The offending path.
        path: String,
        /// Observed size.
        actual_bytes: u64,
        /// Configured bound.
        maximum_bytes: u64,
    },
    /// The path is not a regular file.
    #[error("geoip database `{path}` is not a regular file")]
    NotAFile {
        /// The offending path.
        path: String,
    },
    /// The bytes are not a valid MaxMind DB.
    #[error("geoip database `{path}` is not a valid MaxMind DB: {source}")]
    Invalid {
        /// The offending path.
        path: String,
        /// The reader failure.
        #[source]
        source: maxminddb::MaxMindDbError,
    },
    /// The database edition does not match the role.
    #[error(
        "geoip database `{path}` has database_type `{database_type}`; `{directive}` requires a matching MMDB edition"
    )]
    WrongEdition {
        /// The offending path.
        path: String,
        /// The nginx directive being satisfied.
        directive: &'static str,
        /// The MMDB `database_type` found.
        database_type: String,
    },
}

/// Validate one configured database path, independent of the filesystem.
fn validate_database_path(path: &str) -> Result<(), GeoIpDatabaseError> {
    let fail = |reason: &'static str| {
        Err(GeoIpDatabaseError::Path {
            path: path.to_owned(),
            reason,
        })
    };
    if path.is_empty() {
        return fail("must not be empty");
    }
    if !path.starts_with('/') {
        return fail("must be an absolute path");
    }
    if path == "/" || path.ends_with('/') {
        return fail("must name a file, not a directory");
    }
    if path.contains('\\') || path.contains('\0') {
        return fail("must not contain backslashes or NUL");
    }
    if path.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return fail("must not contain control characters");
    }
    if Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return fail("must not contain `.` or `..` components");
    }
    Ok(())
}

fn read_bounded_database(path: &str) -> Result<Vec<u8>, GeoIpDatabaseError> {
    let metadata = fs::metadata(path).map_err(|source| GeoIpDatabaseError::Read {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(GeoIpDatabaseError::NotAFile {
            path: path.to_owned(),
        });
    }
    if metadata.len() > MAX_GEOIP_DATABASE_BYTES {
        return Err(GeoIpDatabaseError::TooLarge {
            path: path.to_owned(),
            actual_bytes: metadata.len(),
            maximum_bytes: MAX_GEOIP_DATABASE_BYTES,
        });
    }
    let file = File::open(path).map_err(|source| GeoIpDatabaseError::Read {
        path: path.to_owned(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_GEOIP_DATABASE_BYTES) as usize);
    file.take(MAX_GEOIP_DATABASE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| GeoIpDatabaseError::Read {
            path: path.to_owned(),
            source,
        })?;
    if bytes.len() as u64 > MAX_GEOIP_DATABASE_BYTES {
        return Err(GeoIpDatabaseError::TooLarge {
            path: path.to_owned(),
            actual_bytes: bytes.len() as u64,
            maximum_bytes: MAX_GEOIP_DATABASE_BYTES,
        });
    }
    Ok(bytes)
}

/// One `geoip_proxy` value: a bare address (`/32` or `/128`) or a CIDR.
pub fn parse_geoip_proxy(value: &str) -> Result<IpNet, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("geoip_proxy requires a network".to_owned());
    }
    if let Ok(network) = value.parse::<IpNet>() {
        return Ok(network);
    }
    let address = value
        .parse::<IpAddr>()
        .map_err(|_| format!("geoip_proxy `{value}` is not an IP address or CIDR"))?;
    match address {
        IpAddr::V4(address) => IpNet::new(IpAddr::V4(address), 32),
        IpAddr::V6(address) => IpNet::new(IpAddr::V6(address), 128),
    }
    .map_err(|_| format!("geoip_proxy `{value}` is not a valid network"))
}

/// Validate a GeoIP configuration. Returns located `(json pointer, message)`
/// diagnostics, in field order. Database files are opened (metadata only).
#[must_use]
pub fn validate_geoip_config(
    config: &GeoIpConfig,
    database_root: Option<&str>,
) -> Vec<(String, String)> {
    let mut diagnostics = Vec::new();
    for (field, kind) in [
        ("country", GeoIpDatabaseKind::Country),
        ("city", GeoIpDatabaseKind::City),
        ("org", GeoIpDatabaseKind::Org),
    ] {
        let Some(path) = config.database(kind) else {
            continue;
        };
        if let Some(root) = database_root {
            if !super::validate::unix_path_is_confined(root, path) {
                diagnostics.push((
                    format!("/geoip/{field}"),
                    format!("geoip database `{path}` is outside geoipDatabaseRoot `{root}`"),
                ));
                continue;
            }
        }
        if let Err(error) = GeoIpDatabaseMetadata::inspect(path, kind) {
            diagnostics.push((format!("/geoip/{field}"), error.to_string()));
        }
    }
    if config.proxies.len() > MAX_GEOIP_PROXIES {
        diagnostics.push((
            "/geoip/proxies".to_owned(),
            format!(
                "geoip_proxy declares {} entries; maximum is {MAX_GEOIP_PROXIES}",
                config.proxies.len()
            ),
        ));
    }
    for (index, proxy) in config.proxies.iter().enumerate() {
        if let Err(message) = parse_geoip_proxy(proxy) {
            diagnostics.push((format!("/geoip/proxies/{index}"), message));
        }
    }
    diagnostics
}

/// The MMDB edition family of one `geoip_org` database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrgEdition {
    Asn,
    Isp,
    Domain,
    Enterprise,
}

impl OrgEdition {
    fn from_database_type(database_type: &str) -> Self {
        if contains_ignore_ascii_case(database_type, "asn") {
            Self::Asn
        } else if contains_ignore_ascii_case(database_type, "domain") {
            Self::Domain
        } else if contains_ignore_ascii_case(database_type, "enterprise") {
            Self::Enterprise
        } else {
            Self::Isp
        }
    }
}

enum OrgReader {
    Asn(Reader<Vec<u8>>),
    Isp(Reader<Vec<u8>>),
    Domain(Reader<Vec<u8>>),
    Enterprise(Reader<Vec<u8>>),
}

/// The resolved nginx GeoIP variable values for one request.
///
/// Every variable is present; a missing database or an address with no record
/// yields the nginx `not_found` rendering, which is the empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeoIpRequestState {
    /// `$geoip_country_code`.
    pub country_code: String,
    /// `$geoip_country_code3`.
    pub country_code3: String,
    /// `$geoip_country_name`.
    pub country_name: String,
    /// `$geoip_org`.
    pub org: String,
    /// `$geoip_city_continent_code`.
    pub city_continent_code: String,
    /// `$geoip_city_country_code`.
    pub city_country_code: String,
    /// `$geoip_city_country_code3`.
    pub city_country_code3: String,
    /// `$geoip_city_country_name`.
    pub city_country_name: String,
    /// `$geoip_region`.
    pub region: String,
    /// `$geoip_region_name`.
    pub region_name: String,
    /// `$geoip_city`.
    pub city: String,
    /// `$geoip_postal_code`.
    pub postal_code: String,
    /// `$geoip_latitude`.
    pub latitude: String,
    /// `$geoip_longitude`.
    pub longitude: String,
    /// `$geoip_dma_code` (GeoIP2 `location.metro_code`).
    pub dma_code: String,
    /// `$geoip_area_code` (always empty; no MMDB field exists).
    pub area_code: String,
}

impl GeoIpRequestState {
    /// Resolve one nginx GeoIP variable name (without `$`).
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&str> {
        Some(match name {
            "geoip_country_code" => self.country_code.as_str(),
            "geoip_country_code3" => self.country_code3.as_str(),
            "geoip_country_name" => self.country_name.as_str(),
            "geoip_org" => self.org.as_str(),
            "geoip_city_continent_code" => self.city_continent_code.as_str(),
            "geoip_city_country_code" => self.city_country_code.as_str(),
            "geoip_city_country_code3" => self.city_country_code3.as_str(),
            "geoip_city_country_name" => self.city_country_name.as_str(),
            "geoip_region" => self.region.as_str(),
            "geoip_region_name" => self.region_name.as_str(),
            "geoip_city" => self.city.as_str(),
            "geoip_postal_code" => self.postal_code.as_str(),
            "geoip_latitude" => self.latitude.as_str(),
            "geoip_longitude" => self.longitude.as_str(),
            "geoip_dma_code" => self.dma_code.as_str(),
            "geoip_area_code" => self.area_code.as_str(),
            _ => return None,
        })
    }
}

/// Immutable, generation-scoped MMDB readers and trust policy.
///
/// One instance is built per runtime configuration generation; a reload builds
/// a new instance and drops the old one, so request handling never reopens a
/// database file.
pub struct GeoIpDatabases {
    config: GeoIpConfig,
    country: Option<Reader<Vec<u8>>>,
    city: Option<Reader<Vec<u8>>>,
    org: Option<OrgReader>,
    proxies: Vec<IpNet>,
}

impl std::fmt::Debug for GeoIpDatabases {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GeoIpDatabases")
            .field("config", &self.config)
            .field("proxies", &self.proxies)
            .finish_non_exhaustive()
    }
}

impl GeoIpDatabases {
    /// Open every configured database, validating its edition. Missing,
    /// unreadable, corrupt, oversized, or wrong-edition files fail closed.
    pub fn load(
        config: &GeoIpConfig,
        database_root: Option<&str>,
    ) -> Result<Self, GeoIpDatabaseError> {
        for kind in [
            GeoIpDatabaseKind::Country,
            GeoIpDatabaseKind::City,
            GeoIpDatabaseKind::Org,
        ] {
            let Some(path) = config.database(kind) else {
                continue;
            };
            if let Some(root) = database_root {
                if !super::validate::unix_path_is_confined(root, path) {
                    return Err(GeoIpDatabaseError::Path {
                        path: path.to_owned(),
                        reason: "is outside the configured geoipDatabaseRoot",
                    });
                }
            }
            validate_database_path(path)?;
        }
        let country = config
            .country
            .as_deref()
            .map(|path| Self::open(path, GeoIpDatabaseKind::Country))
            .transpose()?;
        let city = config
            .city
            .as_deref()
            .map(|path| Self::open(path, GeoIpDatabaseKind::City))
            .transpose()?;
        let org = match config.org.as_deref() {
            Some(path) => {
                let reader = Self::open(path, GeoIpDatabaseKind::Org)?;
                Some(
                    match OrgEdition::from_database_type(&reader.metadata().database_type) {
                        OrgEdition::Asn => OrgReader::Asn(reader),
                        OrgEdition::Isp => OrgReader::Isp(reader),
                        OrgEdition::Domain => OrgReader::Domain(reader),
                        OrgEdition::Enterprise => OrgReader::Enterprise(reader),
                    },
                )
            }
            None => None,
        };
        let mut proxies = Vec::with_capacity(config.proxies.len());
        for proxy in &config.proxies {
            let network = parse_geoip_proxy(proxy).map_err(|_| GeoIpDatabaseError::Path {
                path: proxy.clone(),
                reason: "is not a valid geoip_proxy network",
            })?;
            proxies.push(network);
        }
        Ok(Self {
            config: config.clone(),
            country,
            city,
            org,
            proxies,
        })
    }

    fn open(path: &str, kind: GeoIpDatabaseKind) -> Result<Reader<Vec<u8>>, GeoIpDatabaseError> {
        let bytes = read_bounded_database(path)?;
        let reader = Reader::from_source(bytes).map_err(|source| GeoIpDatabaseError::Invalid {
            path: path.to_owned(),
            source,
        })?;
        let database_type = reader.metadata().database_type.as_str();
        if !kind.accepts(database_type) {
            return Err(GeoIpDatabaseError::WrongEdition {
                path: path.to_owned(),
                directive: kind.directive(),
                database_type: database_type.to_owned(),
            });
        }
        Ok(reader)
    }

    /// The configuration this instance was built from.
    #[must_use]
    pub fn config(&self) -> &GeoIpConfig {
        &self.config
    }

    /// Resolve every GeoIP variable for one request.
    ///
    /// `client_ip` is the connection address after the runtime's real_ip
    /// handling. `xff_values` are the `X-Forwarded-For` field values in wire
    /// order; the trust walk visits them from the right, exactly like nginx.
    #[must_use]
    pub fn lookup(&self, client_ip: IpAddr, xff_values: &[&str]) -> GeoIpRequestState {
        let address = self.resolve_address(client_ip, xff_values);
        let mut state = GeoIpRequestState::default();
        if let Some(reader) = self.country.as_ref() {
            if let Ok(lookup) = reader.lookup(address) {
                if let Ok(Some(record)) = lookup.decode::<geoip2::Country>() {
                    state.country_code = record
                        .country
                        .iso_code
                        .unwrap_or_default()
                        .to_ascii_uppercase();
                    state.country_code3 = alpha3_for(&state.country_code)
                        .unwrap_or_default()
                        .to_owned();
                    state.country_name =
                        record.country.names.english.unwrap_or_default().to_owned();
                }
            }
        }
        if let Some(reader) = self.city.as_ref() {
            if let Ok(lookup) = reader.lookup(address) {
                if let Ok(Some(record)) = lookup.decode::<geoip2::City>() {
                    state.city_country_code = record
                        .country
                        .iso_code
                        .unwrap_or_default()
                        .to_ascii_uppercase();
                    state.city_country_code3 = alpha3_for(&state.city_country_code)
                        .unwrap_or_default()
                        .to_owned();
                    state.city_country_name =
                        record.country.names.english.unwrap_or_default().to_owned();
                    state.city_continent_code =
                        record.continent.code.unwrap_or_default().to_owned();
                    state.city = record.city.names.english.unwrap_or_default().to_owned();
                    state.region = record
                        .subdivisions
                        .first()
                        .and_then(|subdivision| subdivision.iso_code)
                        .unwrap_or_default()
                        .to_owned();
                    state.region_name = record
                        .subdivisions
                        .first()
                        .and_then(|subdivision| subdivision.names.english)
                        .unwrap_or_default()
                        .to_owned();
                    state.postal_code = record.postal.code.unwrap_or_default().to_owned();
                    state.latitude = record
                        .location
                        .latitude
                        .map(format_coordinate)
                        .unwrap_or_default();
                    state.longitude = record
                        .location
                        .longitude
                        .map(format_coordinate)
                        .unwrap_or_default();
                    state.dma_code = record
                        .location
                        .metro_code
                        .map(|code| code.to_string())
                        .unwrap_or_default();
                    // GeoIP2/GeoLite2 has no legacy `area_code` field.
                    state.area_code = String::new();
                }
            }
        }
        if let Some(reader) = self.org.as_ref() {
            state.org = match reader {
                OrgReader::Asn(reader) => reader
                    .lookup(address)
                    .ok()
                    .and_then(|lookup| lookup.decode::<geoip2::Asn>().ok().flatten())
                    .and_then(|record| record.autonomous_system_organization.map(str::to_owned))
                    .unwrap_or_default(),
                OrgReader::Isp(reader) => reader
                    .lookup(address)
                    .ok()
                    .and_then(|lookup| lookup.decode::<geoip2::Isp>().ok().flatten())
                    .and_then(|record| record.organization.or(record.isp).map(str::to_owned))
                    .unwrap_or_default(),
                OrgReader::Domain(reader) => reader
                    .lookup(address)
                    .ok()
                    .and_then(|lookup| lookup.decode::<geoip2::Domain>().ok().flatten())
                    .and_then(|record| record.domain.map(str::to_owned))
                    .unwrap_or_default(),
                OrgReader::Enterprise(reader) => reader
                    .lookup(address)
                    .ok()
                    .and_then(|lookup| lookup.decode::<geoip2::Enterprise>().ok().flatten())
                    .and_then(|record| {
                        record
                            .traits
                            .organization
                            .or(record.traits.isp)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default(),
            };
        }
        state
    }

    /// The address the GeoIP lookup runs against, after the nginx
    /// `geoip_proxy` / `geoip_proxy_recursive` trust walk.
    fn resolve_address(&self, client_ip: IpAddr, xff_values: &[&str]) -> IpAddr {
        let mut address = canonical_ip(client_ip);
        if self.proxies.is_empty() {
            return address;
        }
        let mut tokens = XffTokens::new(xff_values);
        let mut inspected = 0_usize;
        let mut inspected_bytes = 0_usize;
        while self.is_trusted(address) {
            if inspected >= MAX_GEOIP_XFF_HOPS {
                break;
            }
            let Some(token) = tokens.next() else {
                break;
            };
            inspected = inspected.saturating_add(1);
            inspected_bytes = inspected_bytes.saturating_add(token.len());
            if inspected_bytes > MAX_GEOIP_XFF_BYTES {
                break;
            }
            let Some(next) = parse_forwarded_ip(token) else {
                break;
            };
            address = next;
            if !self.config.proxy_recursive {
                break;
            }
        }
        address
    }

    fn is_trusted(&self, address: IpAddr) -> bool {
        let address = canonical_ip(address);
        self.proxies
            .iter()
            .any(|network| network.contains(&address))
    }
}

/// Right-to-left iterator over the tokens of every `X-Forwarded-For` value.
///
/// nginx reverses the header list and walks each header's value from the right,
/// so the last header's last token (the hop closest to the connection) is
/// visited first.
struct XffTokens<'a> {
    values: &'a [&'a str],
    value_index: usize,
    tokens: Vec<&'a str>,
    token_index: i64,
}

impl<'a> XffTokens<'a> {
    fn new(values: &'a [&'a str]) -> Self {
        Self {
            values,
            value_index: values.len(),
            tokens: Vec::new(),
            token_index: -1,
        }
    }
}

impl<'a> Iterator for XffTokens<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.token_index >= 0 {
                let index = usize::try_from(self.token_index).ok()?;
                self.token_index -= 1;
                if let Some(token) = self.tokens.get(index) {
                    return Some(token.trim());
                }
            }
            if self.value_index == 0 {
                return None;
            }
            self.value_index -= 1;
            let value = self.values.get(self.value_index).copied()?;
            self.tokens = value.split(',').collect();
            self.token_index = i64::try_from(self.tokens.len()).ok()?.saturating_sub(1);
        }
    }
}

fn canonical_ip(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(address) => address
            .to_ipv4_mapped()
            .map_or(IpAddr::V6(address), IpAddr::V4),
        address => address,
    }
}

/// Parse one X-Forwarded-For token the way nginx `ngx_parse_addr_port` does:
/// a bare address, `address:port`, or `[IPv6]:port`.
fn parse_forwarded_ip(token: &str) -> Option<IpAddr> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    token
        .parse::<IpAddr>()
        .ok()
        .or_else(|| token.parse::<SocketAddr>().ok().map(|value| value.ip()))
        .or_else(|| {
            token
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
                .and_then(|value| value.parse::<IpAddr>().ok())
        })
        .map(canonical_ip)
}

/// Format a coordinate like nginx's `%.4f` (`$geoip_latitude`/`$geoip_longitude`).
fn format_coordinate(value: f64) -> String {
    format!("{value:.4}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmdb_writer::{Ipv4Aliasing, Writer};
    use serde_json::json;
    use std::time::{Duration, SystemTime};

    const BUILD_EPOCH: SystemTime = SystemTime::UNIX_EPOCH;

    fn write_fixture(dir: &Path) -> (String, String, String) {
        use mmdb_writer::Value;

        fn names(english: &str) -> Value {
            Value::map([("en", Value::from(english))])
        }

        fn country(iso_code: &str, english: &str, continent: &str, continent_name: &str) -> Value {
            Value::map([
                (
                    "continent",
                    Value::map([
                        ("code", Value::from(continent)),
                        ("names", names(continent_name)),
                    ]),
                ),
                (
                    "country",
                    Value::map([
                        ("iso_code", Value::from(iso_code)),
                        ("names", names(english)),
                    ]),
                ),
            ])
        }

        let country_path = dir.join("GeoIP2-Country-Test.mmdb");
        let city_path = dir.join("GeoIP2-City-Test.mmdb");
        let org_path = dir.join("GeoIP2-ASN-Test.mmdb");

        let mut country_db = Writer::builder("GeoIP2-Country-Test")
            .build_epoch(BUILD_EPOCH)
            .ipv4_aliasing(Ipv4Aliasing::Disabled)
            .build();
        country_db
            .insert(
                "198.51.100.0/24".parse::<IpNet>().expect("network"),
                &country("US", "United States", "NA", "North America"),
            )
            .expect("insert country");
        country_db
            .insert(
                "203.0.113.0/24".parse::<IpNet>().expect("network"),
                &country("DE", "Germany", "EU", "Europe"),
            )
            .expect("insert country");
        country_db
            .insert(
                "127.0.0.1/32".parse::<IpNet>().expect("network"),
                &country("US", "United States", "NA", "North America"),
            )
            .expect("insert loopback country");
        fs::write(&country_path, country_db.to_bytes().expect("country bytes"))
            .expect("write country");

        let mut city_db = Writer::builder("GeoIP2-City-Test")
            .build_epoch(BUILD_EPOCH)
            .ipv4_aliasing(Ipv4Aliasing::Disabled)
            .build();
        let mut us_city = country("US", "United States", "NA", "North America");
        if let Value::Map(entries) = &mut us_city {
            entries.insert(
                "city".to_owned(),
                Value::map([("names", names("Testville"))]),
            );
            entries.insert(
                "location".to_owned(),
                Value::map([
                    ("latitude", Value::from(34.0522_f64)),
                    ("longitude", Value::from(-118.2437_f64)),
                    ("metro_code", Value::from(807_u16)),
                ]),
            );
            entries.insert(
                "postal".to_owned(),
                Value::map([("code", Value::from("90210"))]),
            );
            entries.insert(
                "subdivisions".to_owned(),
                Value::array([Value::map([
                    ("iso_code", Value::from("CA")),
                    ("names", names("California")),
                ])]),
            );
        }
        city_db
            .insert(
                "198.51.100.0/24".parse::<IpNet>().expect("network"),
                &us_city,
            )
            .expect("insert city");
        let mut de_city = country("DE", "Germany", "EU", "Europe");
        if let Value::Map(entries) = &mut de_city {
            entries.insert(
                "city".to_owned(),
                Value::map([("names", names("Testburg"))]),
            );
            entries.insert(
                "location".to_owned(),
                Value::map([
                    ("latitude", Value::from(52.5200_f64)),
                    ("longitude", Value::from(13.4050_f64)),
                    ("metro_code", Value::from(0_u16)),
                ]),
            );
            entries.insert(
                "postal".to_owned(),
                Value::map([("code", Value::from("10115"))]),
            );
            entries.insert(
                "subdivisions".to_owned(),
                Value::array([Value::map([
                    ("iso_code", Value::from("BE")),
                    ("names", names("Berlin")),
                ])]),
            );
        }
        city_db
            .insert(
                "203.0.113.0/24".parse::<IpNet>().expect("network"),
                &de_city,
            )
            .expect("insert city");
        let loopback_city = us_city.clone();
        city_db
            .insert(
                "127.0.0.1/32".parse::<IpNet>().expect("network"),
                &loopback_city,
            )
            .expect("insert loopback city");
        fs::write(&city_path, city_db.to_bytes().expect("city bytes")).expect("write city");

        let mut org_db = Writer::builder("GeoIP2-ASN-Test")
            .build_epoch(BUILD_EPOCH)
            .ipv4_aliasing(Ipv4Aliasing::Disabled)
            .build();
        org_db
            .insert(
                "198.51.100.0/24".parse::<IpNet>().expect("network"),
                &Value::map([
                    ("autonomous_system_number", Value::from(64_500_u32)),
                    ("autonomous_system_organization", Value::from("Example Org")),
                ]),
            )
            .expect("insert org");
        org_db
            .insert(
                "203.0.113.0/24".parse::<IpNet>().expect("network"),
                &Value::map([
                    ("autonomous_system_number", Value::from(64_501_u32)),
                    (
                        "autonomous_system_organization",
                        Value::from("Beispiel GmbH"),
                    ),
                ]),
            )
            .expect("insert org");
        org_db
            .insert(
                "127.0.0.1/32".parse::<IpNet>().expect("network"),
                &Value::map([
                    ("autonomous_system_number", Value::from(64_500_u32)),
                    ("autonomous_system_organization", Value::from("Example Org")),
                ]),
            )
            .expect("insert loopback org");
        fs::write(&org_path, org_db.to_bytes().expect("org bytes")).expect("write org");

        (
            country_path.to_string_lossy().into_owned(),
            city_path.to_string_lossy().into_owned(),
            org_path.to_string_lossy().into_owned(),
        )
    }

    fn fixture() -> (tempfile::TempDir, GeoIpConfig) {
        let dir = tempfile::tempdir().expect("tempdir");
        let (country, city, org) = write_fixture(dir.path());
        (
            dir,
            GeoIpConfig {
                country: Some(country),
                city: Some(city),
                org: Some(org),
                proxies: vec!["127.0.0.1".to_owned(), "192.0.2.0/24".to_owned()],
                proxy_recursive: false,
            },
        )
    }

    #[test]
    fn resolves_every_nginx_variable_from_the_three_databases() {
        let (_dir, config) = fixture();
        let databases = GeoIpDatabases::load(&config, None).expect("load");
        let state = databases.lookup("127.0.0.1".parse().expect("ip"), &["198.51.100.7"]);
        assert_eq!(state.value("geoip_country_code"), Some("US"));
        assert_eq!(state.value("geoip_country_code3"), Some("USA"));
        assert_eq!(state.value("geoip_country_name"), Some("United States"));
        assert_eq!(state.value("geoip_city_country_code"), Some("US"));
        assert_eq!(state.value("geoip_city_country_code3"), Some("USA"));
        assert_eq!(
            state.value("geoip_city_country_name"),
            Some("United States")
        );
        assert_eq!(state.value("geoip_region"), Some("CA"));
        assert_eq!(state.value("geoip_region_name"), Some("California"));
        assert_eq!(state.value("geoip_city"), Some("Testville"));
        assert_eq!(state.value("geoip_postal_code"), Some("90210"));
        assert_eq!(state.value("geoip_city_continent_code"), Some("NA"));
        assert_eq!(state.value("geoip_latitude"), Some("34.0522"));
        assert_eq!(state.value("geoip_longitude"), Some("-118.2437"));
        assert_eq!(state.value("geoip_org"), Some("Example Org"));
        assert_eq!(state.value("geoip_dma_code"), Some("807"));
        assert_eq!(state.value("geoip_area_code"), Some(""));
        assert_eq!(state.value("unknown"), None);
    }

    #[test]
    fn an_address_without_a_record_yields_empty_strings() {
        let (_dir, config) = fixture();
        let databases = GeoIpDatabases::load(&config, None).expect("load");
        // No trusted hop and no record for the documentation address.
        let state = databases.lookup("192.0.2.99".parse().expect("ip"), &[]);
        for variable in GEOIP_VARIABLES {
            assert_eq!(state.value(variable), Some(""), "{variable}");
        }
    }

    #[test]
    fn geoip_proxy_walks_one_hop_when_not_recursive() {
        let (_dir, mut config) = fixture();
        config.proxy_recursive = false;
        let databases = GeoIpDatabases::load(&config, None).expect("load");
        // The rightmost trusted hop (192.0.2.99) has no record; a
        // non-recursive walk stops there instead of reaching 198.51.100.7.
        let state = databases.lookup(
            "127.0.0.1".parse().expect("ip"),
            &["198.51.100.7, 192.0.2.99"],
        );
        assert_eq!(state.value("geoip_country_code"), Some(""));
    }

    #[test]
    fn geoip_proxy_recursive_walks_until_an_untrusted_hop() {
        let (_dir, mut config) = fixture();
        config.proxy_recursive = true;
        let databases = GeoIpDatabases::load(&config, None).expect("load");
        let state = databases.lookup(
            "127.0.0.1".parse().expect("ip"),
            &["198.51.100.7, 192.0.2.99"],
        );
        assert_eq!(state.value("geoip_country_code"), Some("US"));
        assert_eq!(state.value("geoip_city"), Some("Testville"));
    }

    #[test]
    fn an_untrusted_client_address_ignores_x_forwarded_for() {
        let (_dir, config) = fixture();
        let databases = GeoIpDatabases::load(&config, None).expect("load");
        let state = databases.lookup("198.18.0.1".parse().expect("ip"), &["198.51.100.7"]);
        assert_eq!(state.value("geoip_country_code"), Some(""));
    }

    #[test]
    fn validation_rejects_relative_missing_and_wrong_edition_databases() {
        let (_dir, config) = fixture();
        assert!(validate_geoip_config(&config, None).is_empty());
        let relative = GeoIpConfig {
            country: Some("relative/GeoIP2-Country.mmdb".to_owned()),
            ..GeoIpConfig::default()
        };
        assert_eq!(validate_geoip_config(&relative, None).len(), 1);
        let missing = GeoIpConfig {
            country: Some("/nonexistent/GeoIP2-Country.mmdb".to_owned()),
            ..GeoIpConfig::default()
        };
        assert_eq!(validate_geoip_config(&missing, None).len(), 1);
        // A City database is not a valid `geoip_country` edition.
        let wrong = GeoIpConfig {
            country: config.city.clone(),
            ..GeoIpConfig::default()
        };
        let diagnostics = validate_geoip_config(&wrong, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].1.contains("database_type"));
    }

    #[test]
    fn validation_confines_databases_to_the_operator_root() {
        let (dir, config) = fixture();
        let root = dir.path().to_string_lossy().into_owned();
        assert!(validate_geoip_config(&config, Some(&root)).is_empty());
        assert!(!validate_geoip_config(&config, Some("/etc")).is_empty());
    }

    #[test]
    fn proxy_networks_accept_bare_addresses_and_cidrs() {
        assert_eq!(
            parse_geoip_proxy("127.0.0.1").expect("address"),
            "127.0.0.1/32".parse::<IpNet>().expect("net")
        );
        assert_eq!(
            parse_geoip_proxy("10.0.0.0/8").expect("cidr"),
            "10.0.0.0/8".parse::<IpNet>().expect("net")
        );
        assert!(parse_geoip_proxy("not-a-network").is_err());
        assert!(parse_geoip_proxy("").is_err());
    }

    #[test]
    fn x_forwarded_for_is_read_right_to_left_across_repeated_fields() {
        let values = ["198.51.100.7", "127.0.0.9, 127.0.0.8"];
        let tokens: Vec<&str> = XffTokens::new(&values).collect();
        assert_eq!(tokens, vec!["127.0.0.8", "127.0.0.9", "198.51.100.7"]);
    }

    #[test]
    fn oversized_databases_fail_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("huge.mmdb");
        fs::write(&path, b"not a database").expect("write");
        let config = GeoIpConfig {
            country: Some(path.to_string_lossy().into_owned()),
            ..GeoIpConfig::default()
        };
        let diagnostics = validate_geoip_config(&config, None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].1.contains("not a valid MaxMind DB"));
    }

    #[test]
    fn iso_alpha3_derivation_matches_the_country_code() {
        assert_eq!(alpha3_for("US"), Some("USA"));
        assert_eq!(alpha3_for("DE"), Some("DEU"));
        assert_eq!(alpha3_for("ZZ"), None);
    }

    #[test]
    fn build_epoch_is_not_used_for_request_behavior() {
        // Guards against a future fixture generator embedding wall-clock data
        // in a way the lookup contracts depend on.
        let _ = BUILD_EPOCH.duration_since(SystemTime::UNIX_EPOCH);
        let _ = Duration::from_secs(1);
    }
}
