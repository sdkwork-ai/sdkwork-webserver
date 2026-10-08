use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaChecksum {
    pub algorithm: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaResource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub kind: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "publicUrl", default, skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(
        rename = "objectBlobId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub object_blob_id: Option<String>,
    #[serde(rename = "fileName", default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(rename = "sizeBytes", default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<MediaChecksum>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(
        rename = "durationSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_seconds: Option<f64>,
    #[serde(rename = "altText", default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationStoreListing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<MediaResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previews: Vec<MediaResource>,
    #[serde(
        rename = "shortDescription",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub short_description: Option<String>,
    #[serde(
        rename = "fullDescription",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub full_description: Option<String>,
    #[serde(
        rename = "releaseNotes",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub release_notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    #[serde(
        rename = "supportUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub support_url: Option<String>,
    #[serde(
        rename = "privacyPolicyUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub privacy_policy_url: Option<String>,
    #[serde(
        rename = "officialWebsiteUrl",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub official_website_url: Option<String>,
}

/// Business application kind: the user-facing app type. The internal site
/// carrier row derives its technical type from this value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppKind {
    #[default]
    StaticWeb,
    SpaWeb,
    ApiService,
    WechatMiniprogram,
    DouyinMiniprogram,
    IosApp,
    AndroidApp,
    HarmonyosApp,
}

impl AppKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AppKind::StaticWeb => "STATIC_WEB",
            AppKind::SpaWeb => "SPA_WEB",
            AppKind::ApiService => "API_SERVICE",
            AppKind::WechatMiniprogram => "WECHAT_MINIPROGRAM",
            AppKind::DouyinMiniprogram => "DOUYIN_MINIPROGRAM",
            AppKind::IosApp => "IOS_APP",
            AppKind::AndroidApp => "ANDROID_APP",
            AppKind::HarmonyosApp => "HARMONYOS_APP",
        }
    }

    pub fn parse(value: &str) -> Option<AppKind> {
        match value {
            "STATIC_WEB" => Some(AppKind::StaticWeb),
            "SPA_WEB" => Some(AppKind::SpaWeb),
            "API_SERVICE" => Some(AppKind::ApiService),
            "WECHAT_MINIPROGRAM" => Some(AppKind::WechatMiniprogram),
            "DOUYIN_MINIPROGRAM" => Some(AppKind::DouyinMiniprogram),
            "IOS_APP" => Some(AppKind::IosApp),
            "ANDROID_APP" => Some(AppKind::AndroidApp),
            "HARMONYOS_APP" => Some(AppKind::HarmonyosApp),
            _ => None,
        }
    }

    /// Derived internal site carrier type (webserver_site.site_type 1..=6).
    pub fn carrier_site_type(self) -> i32 {
        match self {
            AppKind::StaticWeb => 1,
            AppKind::SpaWeb => 2,
            _ => 6,
        }
    }

    /// Derived internal carrier application type (webserver_site.application_type).
    pub fn carrier_application_type(self) -> &'static str {
        match self {
            AppKind::ApiService => "API",
            _ => "WEB",
        }
    }
}

/// Target deployment platform of an application platform target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Platform {
    Web,
    Api,
    Wechat,
    Douyin,
    Ios,
    Android,
    Harmonyos,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Web => "WEB",
            Platform::Api => "API",
            Platform::Wechat => "WECHAT",
            Platform::Douyin => "DOUYIN",
            Platform::Ios => "IOS",
            Platform::Android => "ANDROID",
            Platform::Harmonyos => "HARMONYOS",
        }
    }

    pub fn parse(value: &str) -> Option<Platform> {
        match value {
            "WEB" => Some(Platform::Web),
            "API" => Some(Platform::Api),
            "WECHAT" => Some(Platform::Wechat),
            "DOUYIN" => Some(Platform::Douyin),
            "IOS" => Some(Platform::Ios),
            "ANDROID" => Some(Platform::Android),
            "HARMONYOS" => Some(Platform::Harmonyos),
            _ => None,
        }
    }
}

/// Development technology stack of a platform target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TechStack {
    Flutter,
    Native,
    UniApp,
    Node,
    Rust,
    Go,
    Java,
    Other,
}

impl TechStack {
    pub fn as_str(self) -> &'static str {
        match self {
            TechStack::Flutter => "FLUTTER",
            TechStack::Native => "NATIVE",
            TechStack::UniApp => "UNI_APP",
            TechStack::Node => "NODE",
            TechStack::Rust => "RUST",
            TechStack::Go => "GO",
            TechStack::Java => "JAVA",
            TechStack::Other => "OTHER",
        }
    }

    pub fn parse(value: &str) -> Option<TechStack> {
        match value {
            "FLUTTER" => Some(TechStack::Flutter),
            "NATIVE" => Some(TechStack::Native),
            "UNI_APP" => Some(TechStack::UniApp),
            "NODE" => Some(TechStack::Node),
            "RUST" => Some(TechStack::Rust),
            "GO" => Some(TechStack::Go),
            "JAVA" => Some(TechStack::Java),
            "OTHER" => Some(TechStack::Other),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformTargetResponse {
    pub id: String,
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(rename = "targetKey")]
    pub target_key: String,
    pub platform: String,
    #[serde(rename = "techStack", skip_serializing_if = "Option::is_none")]
    pub tech_stack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Vec<String>>,
    #[serde(rename = "bundleId", skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(rename = "packageName", skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    #[serde(rename = "appIdValue", skip_serializing_if = "Option::is_none")]
    pub app_id_value: Option<String>,
    #[serde(rename = "bundleName", skip_serializing_if = "Option::is_none")]
    pub bundle_name: Option<String>,
    #[serde(rename = "targetStatus", skip_serializing_if = "Option::is_none")]
    pub target_status: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatePlatformTargetRequest {
    #[serde(rename = "targetKey")]
    pub target_key: String,
    pub platform: String,
    #[serde(rename = "techStack", default)]
    pub tech_stack: Option<String>,
    #[serde(default)]
    pub architectures: Option<Vec<String>>,
    #[serde(rename = "bundleId", default)]
    pub bundle_id: Option<String>,
    #[serde(rename = "packageName", default)]
    pub package_name: Option<String>,
    #[serde(rename = "appId", default)]
    pub app_id: Option<String>,
    #[serde(rename = "bundleName", default)]
    pub bundle_name: Option<String>,
    #[serde(rename = "allowedChannels", default)]
    pub allowed_channels: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PlatformTargetPage {
    pub items: Vec<PlatformTargetResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(rename = "siteId", skip_serializing_if = "Option::is_none")]
    pub site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "appKind")]
    pub app_kind: String,
    #[serde(rename = "siteType", skip_serializing_if = "Option::is_none")]
    pub site_type: Option<i32>,
    pub status: i32,
    /// Whether the application already owns a source version.
    ///
    /// Filled by the application list/retrieve projections in the same
    /// statement (`EXISTS` over `webserver_source_version`) so callers never have to
    /// probe `applications/{applicationId}/source_versions` row by row.
    #[serde(rename = "hasSourceVersion")]
    pub has_source_version: bool,
    #[serde(rename = "runtimeConfig", skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", skip_serializing_if = "Option::is_none")]
    pub store_listing: Option<ApplicationStoreListing>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ApplicationPage {
    pub items: Vec<ApplicationResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateApplicationRequest {
    pub name: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "appKind")]
    pub app_kind: String,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", default)]
    pub store_listing: Option<ApplicationStoreListing>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UpdateApplicationRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "runtimeConfig", default)]
    pub runtime_config: Option<Value>,
    #[serde(rename = "storeListing", default)]
    pub store_listing: Option<ApplicationStoreListing>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainDeploymentResponse {
    pub id: String,
    pub status: i32,
    pub environment: String,
    #[serde(rename = "versionTag", skip_serializing_if = "Option::is_none")]
    pub version_tag: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainResponse {
    pub id: String,
    pub hostname: String,
    #[serde(rename = "rootDomainId", skip_serializing_if = "Option::is_none")]
    pub root_domain_id: Option<String>,
    #[serde(rename = "recordName", skip_serializing_if = "Option::is_none")]
    pub record_name: Option<String>,
    #[serde(rename = "applicationId", skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(rename = "applicationName", skip_serializing_if = "Option::is_none")]
    pub application_name: Option<String>,
    #[serde(rename = "certificateCount", with = "sdkwork_utils_rust::serde_int64")]
    pub certificate_count: i64,
    #[serde(rename = "isPrimary")]
    pub is_primary: bool,
    #[serde(rename = "isVerified")]
    pub is_verified: bool,
    #[serde(rename = "sslEnabled")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", skip_serializing_if = "Option::is_none")]
    pub ssl_provider: Option<String>,
    pub status: i32,
    #[serde(rename = "latestDeployment", skip_serializing_if = "Option::is_none")]
    pub latest_deployment: Option<DomainDeploymentResponse>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainPage {
    pub items: Vec<DomainResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RootDomainResponse {
    pub id: String,
    pub hostname: String,
    /// Operator-facing label. The apex hostname is the identity and never
    /// changes, so this is the only free-text name a root domain carries.
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// DNS provider declaration (`alidns`, `dnspod`, …) or the literal `manual`
    /// when records are published by hand. The tenant console's zone form edits
    /// the same three fields on the Deployments plane.
    #[serde(rename = "dnsProvider", skip_serializing_if = "Option::is_none")]
    pub dns_provider: Option<String>,
    /// Provider-side zone identifier, up to 512 characters.
    #[serde(rename = "providerZoneRef", skip_serializing_if = "Option::is_none")]
    pub provider_zone_ref: Option<String>,
    /// The cloud account whose DNS automation this Zone publishes through.
    ///
    /// Absent means the account resolves per operation, which is the state every
    /// root domain reconciled from the edge's own configuration is in. The value
    /// is an account-center id (`iam_provider_account`), held as a reference
    /// rather than a join because that table belongs to sdkwork-iam.
    #[serde(rename = "cloudAccountId", skip_serializing_if = "Option::is_none")]
    pub cloud_account_id: Option<String>,
    pub status: i32,
    #[serde(rename = "subdomainCount", with = "sdkwork_utils_rust::serde_int64")]
    pub subdomain_count: i64,
    #[serde(
        rename = "boundSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub bound_subdomain_count: i64,
    #[serde(
        rename = "verifiedSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub verified_subdomain_count: i64,
    #[serde(
        rename = "httpsSubdomainCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub https_subdomain_count: i64,
    #[serde(
        rename = "activeDeploymentCount",
        with = "sdkwork_utils_rust::serde_int64"
    )]
    pub active_deployment_count: i64,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RootDomainPage {
    pub items: Vec<RootDomainResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRootDomainRequest {
    pub hostname: String,
    /// The cloud account to bind at creation. Omitted leaves the Zone resolving
    /// its account per operation, exactly as a root domain reconciled from the
    /// edge's configuration does.
    #[serde(rename = "cloudAccountId", default)]
    pub cloud_account_id: Option<String>,
}

/// Partial edit of a tenant root-domain Zone.
///
/// Every member is optional because the three callers send disjoint subsets:
/// the edit form sends the descriptive fields, and the pause/resume action sends
/// `status` alone. An absent member means "leave it as it is" — there is no
/// meaning of "clear this" on the wire for the descriptive fields, because the
/// form that would express it is the same form that omits untouched fields.
///
/// The apex `hostname` is deliberately absent: a root domain's apex is its
/// identity, the tenant-level uniqueness index is built on it, and every
/// registered subdomain resolves against it. Renaming is a delete-and-recreate,
/// not an edit.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRootDomainRequest {
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "dnsProvider", default)]
    pub dns_provider: Option<String>,
    #[serde(rename = "providerZoneRef", default)]
    pub provider_zone_ref: Option<String>,
    #[serde(default)]
    pub status: Option<i32>,
    /// The cloud account to bind, or `null` to unbind.
    ///
    /// The **only** member of this request that distinguishes "leave it alone"
    /// from "clear it", because it names an association rather than describing
    /// the row: an absent member keeps the stored account and an explicit `null`
    /// removes it. That is the difference between the outer `Option` (was the
    /// member present at all?) and the inner one (is the association set?), and
    /// it is why this field cannot share the "a blank value is an omission"
    /// reading the descriptive fields use.
    #[serde(
        rename = "cloudAccountId",
        default,
        deserialize_with = "deserialize_explicit_nullable"
    )]
    pub cloud_account_id: Option<Option<String>>,
}

/// Longest cloud-account id a root-domain request may name.
///
/// The account center's ids are unbounded; this bounds only what travels on the
/// wire and what `webserver_root_domain.cloud_account_id` stores (VARCHAR(128)),
/// which is the same ceiling the deployments DNS Zone already applies to the same
/// reference.
pub const CLOUD_ACCOUNT_ID_MAX_CHARS: usize = 128;

/// The shape rule for a cloud-account reference, in one place.
///
/// Shared rather than duplicated because the reference is validated twice: once
/// where the request is received, so a malformed value never reaches the store,
/// and again where it is persisted, so the durability boundary does not have to
/// trust its caller. Two copies of this rule would eventually disagree, and the
/// failure mode of that is a value one layer accepts under one meaning and
/// another stores under a different one.
///
/// Shape only. Whether the account center actually holds this id is a runtime
/// fact owned by sdkwork-iam; answering it here would be a second, weaker copy of
/// another module's rule.
///
/// Returns the reason the value is unacceptable, or `None` when it is fine.
pub fn cloud_account_id_shape_error(account_id: &str) -> Option<String> {
    let valid = !account_id.is_empty()
        && account_id.chars().count() <= CLOUD_ACCOUNT_ID_MAX_CHARS
        && account_id
            .chars()
            .all(|character| !character.is_whitespace() && !character.is_control())
        && account_id
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric());
    if valid {
        return None;
    }
    Some(format!(
        "cloudAccountId must be 1..{CLOUD_ACCOUNT_ID_MAX_CHARS} characters, start with an \
         alphanumeric, and contain no whitespace or control characters"
    ))
}

/// Reads a member that may be absent, `null`, or a string.
///
/// `serde`'s own `Option<Option<T>>` collapses `null` and an absent member into
/// the same `None`, which is exactly the distinction the root-domain edit needs:
/// absent is "leave the stored account alone" and `null` is "unbind it". Wrapping
/// the *inner* value instead of the outer one keeps the two apart — an absent
/// member never reaches this function (the `default` supplies its `None`), a
/// present `null` deserializes to `Some(None)`, and a string to `Some(Some(_))`.
/// That is what lets the repository express the second as a real
/// `SET cloud_account_id = NULL` rather than a no-op the operator cannot see the
/// difference of.
fn deserialize_explicit_nullable<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRootDomainHostnameRequest {
    #[serde(rename = "recordName")]
    pub record_name: String,
    #[serde(rename = "applicationId", default)]
    pub application_id: Option<String>,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDomainRequest {
    pub hostname: String,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateManagedDomainRequest {
    pub hostname: String,
    #[serde(rename = "applicationId", default)]
    pub application_id: Option<String>,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
    #[serde(rename = "sslEnabled", default = "default_true")]
    pub ssl_enabled: bool,
    #[serde(rename = "sslProvider", default)]
    pub ssl_provider: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateDomainApplicationBindingRequest {
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "isPrimary", default)]
    pub is_primary: bool,
}

fn default_true() -> bool {
    true
}

pub(crate) fn default_page() -> i32 {
    1
}

pub(crate) fn default_page_size() -> i32 {
    20
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainVerifyResponse {
    pub verified: bool,
    pub status: String,
    pub method: String,
    #[serde(rename = "recordName")]
    pub record_name: String,
    #[serde(rename = "recordValue")]
    pub record_value: String,
    #[serde(rename = "attemptCount")]
    pub attempt_count: i32,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
    #[serde(rename = "nextAttemptAt", skip_serializing_if = "Option::is_none")]
    pub next_attempt_at: Option<String>,
    #[serde(rename = "checkedAt", skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(rename = "failureCode", skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

/// One synced DNS resolution record of a root-domain Zone.
///
/// A row of the snapshot the last cloud-account sync read from the provider —
/// the "how does this subdomain resolve" answer the subdomain page renders:
/// the record type (解析类型) and the record content (解析 IP for `A`/`AAAA`,
/// the target for `CNAME`/`MX`). The row is a snapshot, not live DNS: it is
/// what the provider answered at `syncedAt`, and a later sync replaces it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainDnsRecordResponse {
    pub id: String,
    /// Absolute record owner inside the zone, e.g. `www.example.com`.
    #[serde(rename = "recordName")]
    pub record_name: String,
    /// Record type as the provider spells it (`A`, `AAAA`, `CNAME`, `TXT`, `MX`).
    #[serde(rename = "recordType")]
    pub record_type: String,
    /// Record content: the resolution address for `A`/`AAAA`, the target for
    /// `CNAME`/`MX`/`NS`, the text for `TXT`.
    #[serde(rename = "recordValue")]
    pub record_value: String,
    #[serde(rename = "ttlSeconds", skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i32>,
    /// Priority for the record types that carry one (`MX`, `SRV`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Provider resolution line, when the provider splits one owner into
    /// per-line records.
    #[serde(rename = "recordLine", skip_serializing_if = "Option::is_none")]
    pub record_line: Option<String>,
    /// The registered subdomain this record resolves, when its owner matched
    /// one at sync time; absent when the owner matches none.
    #[serde(rename = "domainId", skip_serializing_if = "Option::is_none")]
    pub domain_id: Option<String>,
    /// Provider family the snapshot was read from (`ALIYUN_DNS`, `DNSPOD`,
    /// `CLOUDFLARE`).
    #[serde(rename = "dnsProvider")]
    pub dns_provider: String,
    /// The cloud account the snapshot was read through.
    #[serde(rename = "cloudAccountId")]
    pub cloud_account_id: String,
    /// Provider-assigned record identity, when the provider returned one.
    #[serde(rename = "providerRecordRef", skip_serializing_if = "Option::is_none")]
    pub provider_record_ref: Option<String>,
    /// When this row was read from the provider.
    #[serde(rename = "syncedAt")]
    pub synced_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DomainDnsRecordPage {
    pub items: Vec<DomainDnsRecordResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

/// What one cloud-account sync run established.
///
/// Returned by the sync endpoint so the caller can show the run's outcome
/// without re-reading: how many records the provider answered, when, through
/// which account and provider. `recordCount` counts the whole Zone snapshot,
/// not one subdomain's rows — a sync reads the Zone's inventory in one pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DomainDnsSyncResponse {
    #[serde(rename = "recordCount", with = "sdkwork_utils_rust::serde_int64")]
    pub record_count: i64,
    #[serde(rename = "syncedAt")]
    pub synced_at: String,
    /// The zone apex the provider inventory was read for.
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,
    /// Provider family the inventory was read from.
    #[serde(rename = "dnsProvider")]
    pub dns_provider: String,
    /// The cloud account whose credential answered the inventory read.
    #[serde(rename = "cloudAccountId")]
    pub cloud_account_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVersionConfigSnapshot {
    #[serde(rename = "appConfigPath", default = "default_app_config_path")]
    pub app_config_path: String,
    #[serde(
        rename = "deploymentConfigPath",
        default = "default_deployment_config_path"
    )]
    pub deployment_config_path: String,
    #[serde(rename = "appConfigDetected", default)]
    pub app_config_detected: bool,
    #[serde(rename = "deploymentConfigDetected", default)]
    pub deployment_config_detected: bool,
}

fn default_app_config_path() -> String {
    "sdkwork.app.config.json".to_string()
}

fn default_deployment_config_path() -> String {
    "etc/sdkwork.deployment.config.json".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceVersionResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceRef", skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(rename = "commitHash", skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(rename = "artifactDriveUri")]
    pub artifact_drive_uri: String,
    #[serde(rename = "artifactSize", with = "sdkwork_utils_rust::serde_int64")]
    pub artifact_size: i64,
    #[serde(rename = "artifactHash")]
    pub artifact_hash: String,
    #[serde(rename = "configSnapshot")]
    pub config_snapshot: SourceVersionConfigSnapshot,
    pub status: i32,
    pub retained: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceVersionPage {
    pub items: Vec<SourceVersionResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSourceVersionRequest {
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "sourceRef", default)]
    pub source_ref: Option<String>,
    #[serde(rename = "commitHash", default)]
    pub commit_hash: Option<String>,
    #[serde(rename = "artifactDriveUri")]
    pub artifact_drive_uri: String,
    #[serde(rename = "artifactSize", with = "sdkwork_utils_rust::serde_int64")]
    pub artifact_size: i64,
    #[serde(rename = "artifactHash")]
    pub artifact_hash: String,
    #[serde(rename = "configSnapshot", default)]
    pub config_snapshot: SourceVersionConfigSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportGitSourceVersionRequest {
    #[serde(rename = "versionTag")]
    pub version_tag: String,
    #[serde(rename = "repositoryUrl")]
    pub repository_url: String,
    #[serde(rename = "gitRef", default)]
    pub git_ref: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    pub status: i32,
    #[serde(rename = "deployType")]
    pub deploy_type: i32,
    #[serde(rename = "sourceVersionId", skip_serializing_if = "Option::is_none")]
    pub source_version_id: Option<String>,
    pub environment: String,
    #[serde(rename = "versionTag", skip_serializing_if = "Option::is_none")]
    pub version_tag: Option<String>,
    #[serde(rename = "commitHash", skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(rename = "sourceRef", skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(
        rename = "rollbackFromDeploymentId",
        skip_serializing_if = "Option::is_none"
    )]
    pub rollback_from_deployment_id: Option<String>,
    #[serde(rename = "artifactDriveUri", skip_serializing_if = "Option::is_none")]
    pub artifact_drive_uri: Option<String>,
    #[serde(
        rename = "artifactSize",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub artifact_size: Option<i64>,
    #[serde(rename = "artifactHash", skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(
        rename = "durationMs",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_ms: Option<i64>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeploymentPage {
    pub items: Vec<DeploymentResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateDeploymentRequest {
    #[serde(rename = "deployType")]
    pub deploy_type: i32,
    #[serde(rename = "sourceVersionId", default)]
    pub source_version_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(rename = "versionTag", default)]
    pub version_tag: Option<String>,
    #[serde(rename = "commitHash", default)]
    pub commit_hash: Option<String>,
    #[serde(rename = "sourceRef", default)]
    pub source_ref: Option<String>,
    #[serde(rename = "artifactDriveUri", default)]
    pub artifact_drive_uri: Option<String>,
    #[serde(
        rename = "artifactSize",
        with = "sdkwork_utils_rust::serde_int64::option",
        default
    )]
    pub artifact_size: Option<i64>,
    #[serde(rename = "artifactHash", default)]
    pub artifact_hash: Option<String>,
    /// Framework-scoped idempotency identity used by the repository for durable deployment deduplication.
    /// This value is injected from the validated Header context and is never accepted from JSON input.
    #[serde(skip)]
    pub idempotency_key: Option<String>,
}

fn default_deploy_type() -> i32 {
    1
}

impl Default for CreateDeploymentRequest {
    fn default() -> Self {
        Self {
            deploy_type: default_deploy_type(),
            source_version_id: None,
            environment: None,
            version_tag: None,
            commit_hash: None,
            source_ref: None,
            artifact_drive_uri: None,
            artifact_size: None,
            artifact_hash: None,
            idempotency_key: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariableResponse {
    pub id: String,
    pub key: String,
    pub value: String,
    pub environment: String,
    #[serde(rename = "isSecret")]
    pub is_secret: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EnvVariablePage {
    pub items: Vec<EnvVariableResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEnvVariableRequest {
    pub key: String,
    pub value: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

/// Environment variable rotation: replaces the stored value (encrypted when
/// secret) without changing the key or environment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateEnvVariableRequest {
    pub value: String,
    #[serde(rename = "isSecret", default)]
    pub is_secret: bool,
}

fn default_environment() -> String {
    "production".to_string()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateIdentifierResponse {
    #[serde(rename = "domainId")]
    pub domain_id: String,
    pub hostname: String,
    #[serde(rename = "identifierType")]
    pub identifier_type: String,
    pub position: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateResponse {
    pub id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub identifiers: Vec<CertificateIdentifierResponse>,
    #[serde(rename = "certType", skip_serializing_if = "Option::is_none")]
    pub cert_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    #[serde(rename = "notBefore", skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    #[serde(rename = "autoRenew", skip_serializing_if = "Option::is_none")]
    pub auto_renew: Option<bool>,
    #[serde(rename = "renewalStatus", skip_serializing_if = "Option::is_none")]
    pub renewal_status: Option<String>,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateOperationAcceptedResponse {
    pub accepted: bool,
    pub operation_id: String,
    pub status: String,
}

/// Decrypted node-scoped TLS assignment material projected from the control
/// plane for the self-hosted TLS runtime snapshot. Private key material is
/// transported only inside the process boundary and never leaves the node.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsCertificateAssignmentMaterial {
    pub certificate_id: String,
    pub version_uuid: String,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub fingerprint_sha256: String,
    pub not_before: String,
    pub not_after: String,
    pub fullchain_pem: String,
    pub private_key_pem: String,
}

impl std::fmt::Debug for TlsCertificateAssignmentMaterial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // PEM bodies (certificate chain, private key) are redacted so any
        // future `?value` log site can never leak key material.
        f.debug_struct("TlsCertificateAssignmentMaterial")
            .field("certificate_id", &self.certificate_id)
            .field("version_uuid", &self.version_uuid)
            .field("cert_name", &self.cert_name)
            .field("hostnames", &self.hostnames)
            .field("fingerprint_sha256", &self.fingerprint_sha256)
            .field("not_before", &self.not_before)
            .field("not_after", &self.not_after)
            .field(
                "fullchain_pem",
                &format!("<pem {} bytes>", self.fullchain_pem.len()),
            )
            .field(
                "private_key_pem",
                &format!("<private key {} bytes>", self.private_key_pem.len()),
            )
            .finish()
    }
}

/// Certificate revocation request. `reason` selects one of the RFC 5280
/// §5.3.1 revocation reasons accepted by the control plane.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeCertificateRequest {
    pub reason: String,
}

/// Ceiling for `CertificateOperationResponse::failure_detail`, in characters.
///
/// One number, three places: this constant, the `failure_detail` column width in
/// the baseline DDL, and the `maxLength` the OpenAPI contract declares. They are
/// character counts rather than byte counts so a Chinese provider's diagnostic is
/// not truncated three times shorter than an English one.
pub const CERTIFICATE_FAILURE_DETAIL_MAX_CHARS: usize = 512;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateOperationResponse {
    pub id: String,
    pub certificate_id: String,
    pub operation_type: String,
    pub status: String,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub next_attempt_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    /// What the failure actually said, as the provider worded it.
    ///
    /// The code classifies the failure for retry, cooldown and localized console
    /// copy; this is the part an operator acts on. Kept separate from the code
    /// rather than concatenated into it because the console translates the code
    /// and the scheduler branches on it, so widening the code into prose would
    /// break both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_detail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificatePage {
    pub items: Vec<CertificateResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCertificateRequest {
    #[serde(rename = "domainIds")]
    pub domain_ids: Vec<String>,
    #[serde(rename = "certType")]
    pub cert_type: i32,
    #[serde(rename = "keyAlgorithm", default = "default_certificate_key_algorithm")]
    pub key_algorithm: String,
    #[serde(rename = "autoRenew", default = "default_true")]
    pub auto_renew: bool,
    /// Operator-facing name. `None` keeps the generated identity, which is what
    /// a caller written before this field existed still expects.
    #[serde(rename = "certName", skip_serializing_if = "Option::is_none")]
    pub cert_name: Option<String>,
    #[serde(rename = "certificateScope", default = "default_certificate_scope")]
    pub certificate_scope: String,
    #[serde(
        rename = "validationMethod",
        default = "default_certificate_validation_method"
    )]
    pub validation_method: String,
    #[serde(
        rename = "renewBeforeDays",
        default = "default_certificate_renew_before_days"
    )]
    pub renew_before_days: i32,
    /// Which CA directory to order from. `None` leaves the choice to the server,
    /// which is what a self-signed request wants anyway.
    #[serde(rename = "caProfile", skip_serializing_if = "Option::is_none")]
    pub ca_profile: Option<String>,
    /// Which configured DNS account publishes the DNS-01 challenge records.
    /// `None` is "resolve one from the identifier set", which is deliberately
    /// not the same as "no account": pinning here would freeze a choice the
    /// operator never made.
    #[serde(rename = "providerAccountId", skip_serializing_if = "Option::is_none")]
    pub provider_account_id: Option<String>,
}

/// The request a caller who supplied only the required fields gets.
///
/// This is deliberately **not** Rust's zero value: every field here resolves
/// through the same `default_*` function the matching `#[serde(default = ...)]`
/// names, because otherwise a Rust caller and a JSON caller that both omitted a
/// field would order two different certificates from one request shape.
///
/// `cert_type` is the exception and is left at `0`, which the shape validator
/// rejects — there is no defensible default between "Let's Encrypt" and
/// "self-signed", so a caller has to say which. Fixtures build on this
/// (`..Default::default()`) so that widening the request does not require
/// editing every one of them; see the two `E0063` sweeps that motivated it.
impl Default for IssueCertificateRequest {
    fn default() -> Self {
        Self {
            domain_ids: Vec::new(),
            cert_type: 0,
            key_algorithm: default_certificate_key_algorithm(),
            auto_renew: true,
            cert_name: None,
            certificate_scope: default_certificate_scope(),
            validation_method: default_certificate_validation_method(),
            renew_before_days: default_certificate_renew_before_days(),
            ca_profile: None,
            provider_account_id: None,
        }
    }
}

fn default_certificate_key_algorithm() -> String {
    // Not a literal: which algorithm an operator with no opinion gets is one
    // decision, and it has to be the same one the ACME engine maps and the
    // deployment control plane writes, so it is read from the shared vocabulary.
    sdkwork_deploy_core::CERTIFICATE_DEFAULT_KEY_ALGORITHM.to_owned()
}

/// Identifier scope vocabulary, mirrored by the `enum` on the authored
/// `IssueCertificateRequest` schema. Defined once because the default, the
/// validator and the authored contract all have to spell `SINGLE_DOMAIN` the
/// same way for a request to be accepted and stored under one meaning.
pub const CERTIFICATE_SCOPE_SINGLE_DOMAIN: &str = "SINGLE_DOMAIN";
pub const CERTIFICATE_SCOPE_WILDCARD: &str = "WILDCARD";

/// Challenge-method vocabulary. The spellings are exactly the ones
/// `sdkwork_webserver_acme_service::DeclaredChallengeMethod` parses, so a value
/// accepted into a request is a value the issuance engine can act on rather
/// than one it would reject later, on the worker, as a configuration error.
pub const CERTIFICATE_VALIDATION_AUTO: &str = "AUTO";
pub const CERTIFICATE_VALIDATION_HTTP_01: &str = "HTTP_01";
pub const CERTIFICATE_VALIDATION_DNS_01: &str = "DNS_01";

/// CA-profile vocabulary, mirrored by the `enum` on the authored schema.
///
/// `SELF_SIGNED` is reachable only by derivation from `certType` 3; a caller
/// cannot ask for it by name, because "issue me something self-signed while I
/// said Let's Encrypt" is a contradiction rather than a preference.
pub const CERTIFICATE_CA_PROFILE_LETS_ENCRYPT_PRODUCTION: &str = "LETS_ENCRYPT_PRODUCTION";
pub const CERTIFICATE_CA_PROFILE_LETS_ENCRYPT_STAGING: &str = "LETS_ENCRYPT_STAGING";
pub const CERTIFICATE_CA_PROFILE_SELF_SIGNED: &str = "SELF_SIGNED";

/// Accepted bounds for `renewBeforeDays`, re-exported from the shared
/// certificate vocabulary so the authored schema's `minimum`/`maximum`, the
/// request validators and the renewal scheduler all read one pair of numbers
/// rather than each carrying its own copy of 7 and 90.
pub use sdkwork_deploy_core::{
    CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS, CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS,
    CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS,
};

fn default_certificate_scope() -> String {
    // One exact hostname, because that is the narrowest reading of "the domain
    // ids I listed" and widening a request is a decision only the operator can
    // make. Mirrors the `default:` on the authored schema.
    CERTIFICATE_SCOPE_SINGLE_DOMAIN.to_owned()
}

fn default_certificate_validation_method() -> String {
    // `AUTO` rather than a pinned method: the edge knows which accounts it has
    // configured, and pinning one here would refuse requests the edge could
    // have answered.
    CERTIFICATE_VALIDATION_AUTO.to_owned()
}

fn default_certificate_renew_before_days() -> i32 {
    // Same shared vocabulary the console's field is seeded from and the bounds
    // are enforced at, so all three agree on what "no opinion" means.
    sdkwork_deploy_core::CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS
}

/// One configured cloud DNS account, as an operator may name it.
///
/// Deliberately carries no verification state. The registry's `verify_accounts`
/// runs once at startup and only logs, so there is no cached verdict to serve;
/// answering this request by calling the provider would turn a list into a
/// vendor round trip on an authenticated read path. What an operator needs to
/// choose an account is which zone it can publish into, and that is what this
/// reports.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DnsAccountResponse {
    #[serde(rename = "accountId")]
    pub account_id: String,
    pub provider: String,
    #[serde(rename = "zoneApex")]
    pub zone_apex: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DnsAccountPage {
    pub items: Vec<DnsAccountResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

/// `webserver_certificate.cert_name` is VARCHAR(200); a name that would be
/// truncated on insert is a name the ledger reports differently from the one
/// that was asked for.
pub const CERTIFICATE_NAME_MAX_CHARS: usize = 200;

/// Longest account id a request may name. The configured ids are unbounded;
/// this bounds only what travels on the wire.
pub const CERTIFICATE_ACCOUNT_ID_MAX_CHARS: usize = 128;

/// The shape rules for [`IssueCertificateRequest`], in one place.
///
/// Shared rather than duplicated because the request is validated twice: once
/// where it is received, so a malformed request never reaches the store, and
/// again where it is persisted, so the durability boundary does not have to
/// trust its caller. Two copies of these rules would eventually disagree, and
/// the failure mode of that is a request one layer accepts under one meaning
/// and another stores under a different one.
///
/// Returns the reason the request is unacceptable, or `None` when it is fine.
/// The identifier-set checks are deliberately **not** here: they need each
/// row's `EXACT`/`WILDCARD` type, which only exists after the domains are read.
pub fn certificate_issue_shape_error(request: &IssueCertificateRequest) -> Option<String> {
    if !matches!(request.cert_type, 1 | 3) {
        return Some("certType must be 1 (Let's Encrypt) or 3 (self-signed)".to_owned());
    }
    if request.cert_type == 3 && request.auto_renew {
        return Some("automatic renewal is unavailable for self-signed certificates".to_owned());
    }
    if !matches!(request.key_algorithm.as_str(), "ECDSA" | "RSA") {
        return Some("keyAlgorithm must be ECDSA or RSA".to_owned());
    }
    if !matches!(
        request.certificate_scope.as_str(),
        CERTIFICATE_SCOPE_SINGLE_DOMAIN | CERTIFICATE_SCOPE_WILDCARD
    ) {
        return Some("certificateScope must be SINGLE_DOMAIN or WILDCARD".to_owned());
    }
    if !matches!(
        request.validation_method.as_str(),
        CERTIFICATE_VALIDATION_AUTO
            | CERTIFICATE_VALIDATION_HTTP_01
            | CERTIFICATE_VALIDATION_DNS_01
    ) {
        return Some("validationMethod must be AUTO, HTTP_01 or DNS_01".to_owned());
    }
    // Refused here rather than left to the worker: a wildcard can only be
    // authorized over DNS-01, so accepting the pair would queue an operation
    // whose only possible outcome is a failure the caller already had the
    // information to avoid.
    if request.certificate_scope == CERTIFICATE_SCOPE_WILDCARD
        && request.validation_method == CERTIFICATE_VALIDATION_HTTP_01
    {
        return Some(
            "certificateScope WILDCARD cannot be validated with HTTP_01; \
             a wildcard name can only be authorized over DNS-01"
                .to_owned(),
        );
    }
    if let Some(name) = request.cert_name.as_deref() {
        if name.is_empty()
            || name.len() > CERTIFICATE_NAME_MAX_CHARS
            || name.chars().any(char::is_control)
        {
            return Some(format!(
                "certName must be 1..{CERTIFICATE_NAME_MAX_CHARS} characters without control characters"
            ));
        }
    }
    if let Some(profile) = request.ca_profile.as_deref() {
        if !matches!(
            profile,
            CERTIFICATE_CA_PROFILE_LETS_ENCRYPT_PRODUCTION
                | CERTIFICATE_CA_PROFILE_LETS_ENCRYPT_STAGING
        ) {
            return Some(
                "caProfile must be LETS_ENCRYPT_PRODUCTION or LETS_ENCRYPT_STAGING".to_owned(),
            );
        }
        // A self-signed request resolves to `SELF_SIGNED` regardless of what is
        // sent. Refusing is the honest answer: silently recording a profile the
        // request did not mean would misreport where the certificate came from.
        if request.cert_type != 1 {
            return Some(
                "caProfile names a CA directory and is meaningful only for certType 1 \
                 (Let's Encrypt); a self-signed request resolves to SELF_SIGNED"
                    .to_owned(),
            );
        }
    }
    if let Some(account_id) = request.provider_account_id.as_deref() {
        // Shape only. Whether this edge holds credentials for the id is a
        // runtime fact, checked where the registry is reachable rather than
        // restated here as a second, weaker answer.
        if account_id.is_empty()
            || account_id.len() > CERTIFICATE_ACCOUNT_ID_MAX_CHARS
            || account_id
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Some(format!(
                "providerAccountId must be 1..{CERTIFICATE_ACCOUNT_ID_MAX_CHARS} characters \
                 without whitespace or control characters"
            ));
        }
    }
    if !(CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS..=CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS)
        .contains(&request.renew_before_days)
    {
        return Some(format!(
            "renewBeforeDays must be between {CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS} and {CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS}"
        ));
    }
    None
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateCertificateRequest {
    #[serde(rename = "autoRenew")]
    pub auto_renew: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateListenerCertificateBindingRequest {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(
        rename = "certificateVersionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub certificate_version_id: Option<String>,
    #[serde(default = "default_certificate_binding_priority")]
    pub priority: i32,
    #[serde(rename = "isDefault", default)]
    pub is_default: bool,
}

fn default_certificate_binding_priority() -> i32 {
    100
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateBindingResponse {
    pub id: String,
    #[serde(rename = "applicationId")]
    pub application_id: String,
    #[serde(rename = "domainId")]
    pub domain_id: String,
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "desiredCertificateVersionId")]
    pub desired_certificate_version_id: String,
    #[serde(
        rename = "currentCertificateVersionId",
        skip_serializing_if = "Option::is_none"
    )]
    pub current_certificate_version_id: Option<String>,
    #[serde(rename = "desiredCertificate")]
    pub desired_certificate: ListenerCertificateSummaryResponse,
    #[serde(rename = "currentCertificate", skip_serializing_if = "Option::is_none")]
    pub current_certificate: Option<ListenerCertificateSummaryResponse>,
    #[serde(rename = "keyAlgorithm")]
    pub key_algorithm: String,
    pub priority: i32,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    pub status: String,
    #[serde(rename = "activatedAt", skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateSummaryResponse {
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub identifiers: Vec<CertificateIdentifierResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(rename = "notAfter", skip_serializing_if = "Option::is_none")]
    pub not_after: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListenerCertificateBindingPage {
    pub items: Vec<ListenerCertificateBindingResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateDistributionResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "serverName")]
    pub server_name: String,
    pub host: String,
    #[serde(rename = "desiredSyncVersion")]
    pub desired_sync_version: String,
    #[serde(rename = "appliedSyncVersion", skip_serializing_if = "Option::is_none")]
    pub applied_sync_version: Option<String>,
    pub status: String,
    #[serde(rename = "lastHeartbeatAt", skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateDistributionPage {
    pub items: Vec<CertificateDistributionResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone)]
pub struct CertificateIssueUpdate {
    pub cert_name: String,
    pub cert_type: i32,
    pub issuer: String,
    pub subject: String,
    pub serial_sha256: String,
    pub fingerprint_sha256: String,
    pub spki_sha256: String,
    pub chain_sha256: String,
    pub key_algorithm: String,
    pub fullchain_pem: String,
    pub private_key_pem: String,
    pub not_before: String,
    pub not_after: String,
    pub auto_renew: bool,
}

impl std::fmt::Debug for CertificateIssueUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Same redaction contract as TlsCertificateAssignmentMaterial.
        f.debug_struct("CertificateIssueUpdate")
            .field("cert_name", &self.cert_name)
            .field("cert_type", &self.cert_type)
            .field("issuer", &self.issuer)
            .field("subject", &self.subject)
            .field("serial_sha256", &self.serial_sha256)
            .field("fingerprint_sha256", &self.fingerprint_sha256)
            .field("spki_sha256", &self.spki_sha256)
            .field("chain_sha256", &self.chain_sha256)
            .field("key_algorithm", &self.key_algorithm)
            .field(
                "fullchain_pem",
                &format!("<pem {} bytes>", self.fullchain_pem.len()),
            )
            .field(
                "private_key_pem",
                &format!("<private key {} bytes>", self.private_key_pem.len()),
            )
            .field("not_before", &self.not_before)
            .field("not_after", &self.not_after)
            .field("auto_renew", &self.auto_renew)
            .finish()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub id: String,
    #[serde(rename = "checkType")]
    pub check_type: i32,
    #[serde(rename = "checkUrl")]
    pub check_url: String,
    #[serde(rename = "checkInterval")]
    pub check_interval: i32,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i32,
    #[serde(rename = "retryCount")]
    pub retry_count: i32,
    pub status: i32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HealthCheckPage {
    pub items: Vec<HealthCheckResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHealthCheckRequest {
    #[serde(rename = "checkType")]
    pub check_type: i32,
    #[serde(rename = "checkUrl")]
    pub check_url: String,
    #[serde(rename = "checkInterval", default = "default_health_check_interval")]
    pub check_interval: i32,
    #[serde(rename = "timeoutMs", default = "default_health_check_timeout_ms")]
    pub timeout_ms: i32,
    #[serde(rename = "retryCount", default = "default_health_check_retry_count")]
    pub retry_count: i32,
}

fn default_health_check_interval() -> i32 {
    60
}

fn default_health_check_timeout_ms() -> i32 {
    5_000
}

fn default_health_check_retry_count() -> i32 {
    3
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigResponse {
    pub id: String,
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    pub status: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NginxConfigPage {
    pub items: Vec<NginxConfigResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ListNginxConfigsQuery {
    #[serde(default = "crate::dto::default_page")]
    pub page: i32,
    #[serde(default = "crate::dto::default_page_size")]
    pub page_size: i32,
    #[serde(rename = "site_id", default)]
    pub site_id: Option<String>,
    #[serde(rename = "config_type", default)]
    pub config_type: Option<i32>,
    #[serde(rename = "is_active", default)]
    pub is_active: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateNginxConfigRequest {
    #[serde(rename = "siteId")]
    pub site_id: String,
    #[serde(rename = "configName")]
    pub config_name: String,
    #[serde(rename = "configType")]
    pub config_type: i32,
    #[serde(rename = "configContent")]
    pub config_content: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateNginxConfigRequest {
    #[serde(rename = "configName", default)]
    pub config_name: Option<String>,
    #[serde(rename = "configContent", default)]
    pub config_content: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxReloadResponse {
    pub reloaded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NginxStatusResponse {
    pub running: bool,
    #[serde(rename = "activeConfigs", with = "sdkwork_utils_rust::serde_int64")]
    pub active_configs: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerResponse {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,
    #[serde(rename = "sshPort")]
    pub ssh_port: i32,
    pub status: i32,
    #[serde(rename = "lastHeartbeatAt", skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateServerResponse {
    #[serde(flatten)]
    pub server: ServerResponse,
    #[serde(rename = "agentToken")]
    pub agent_token: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServerPage {
    pub items: Vec<ServerResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateServerRequest {
    pub name: String,
    pub host: String,
    #[serde(rename = "tenantScopeHash")]
    pub tenant_scope_hash: String,
    #[serde(rename = "sshPort", default = "default_ssh_port")]
    pub ssh_port: i32,
}

fn default_ssh_port() -> i32 {
    22
}

#[derive(Clone, Debug)]
pub struct CertificateRenewalCandidate {
    pub tenant_id: i64,
    pub certificate_id: String,
    pub cert_type: i32,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub key_algorithm: String,
    pub auto_renew: bool,
    pub not_after: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CertificateOperationCycleReport {
    pub scheduled: usize,
    pub claimed: usize,
    pub succeeded: usize,
    pub retried: usize,
    pub failed: usize,
}

#[derive(Clone, Debug)]
pub struct CertificateOperationLease {
    pub tenant_id: i64,
    pub operation_id: String,
    pub certificate_id: String,
    pub operation_type: String,
    pub cert_type: i32,
    pub cert_name: String,
    pub hostnames: Vec<String>,
    pub key_algorithm: String,
    pub auto_renew: bool,
    /// The certificate's declared identifier scope (`SINGLE_DOMAIN` or
    /// `WILDCARD`), or `None` for a certificate accepted before the scope was
    /// recorded.
    ///
    /// Re-checked against the claimed identifiers, not merely carried: a
    /// certificate accepted as single-domain must not renew as a wildcard
    /// because a domain row's type changed after the fact.
    pub certificate_scope: Option<String>,
    /// The certificate's declared challenge method (`AUTO`, `HTTP_01` or
    /// `DNS_01`). Carried per operation rather than read from the deployment's
    /// setting so a renewal presents the way its original request said.
    pub validation_method: Option<String>,
    /// The cloud account that must present this certificate's DNS-01
    /// challenges, when the request pinned one.
    pub provider_account_id: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub lease_owner: String,
    pub fencing_token: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentHeartbeatRequest {
    #[serde(rename = "agentVersion", skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,
    #[serde(rename = "nginxEnabled", skip_serializing_if = "Option::is_none")]
    pub nginx_enabled: Option<bool>,
    #[serde(
        rename = "activeConfigs",
        with = "sdkwork_utils_rust::serde_int64::option",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub active_configs: Option<i64>,
    #[serde(rename = "lastSyncVersion", skip_serializing_if = "Option::is_none")]
    pub last_sync_version: Option<String>,
    #[serde(rename = "certificateObservations", default)]
    pub certificate_observations: Vec<AgentCertificateObservation>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCertificateObservation {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    pub fingerprint: String,
    #[serde(rename = "syncVersion")]
    pub sync_version: String,
    pub state: String,
    #[serde(rename = "observedAt")]
    pub observed_at: String,
    #[serde(rename = "failureCode", skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentHeartbeatResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub status: i32,
    #[serde(rename = "acknowledgedAt")]
    pub acknowledged_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentSyncResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    #[serde(rename = "syncVersion")]
    pub sync_version: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unchanged: bool,
    #[serde(rename = "nginxConfigs")]
    pub nginx_configs: Vec<AgentNginxConfigBundle>,
    pub certificates: Vec<AgentCertificateBundle>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentNginxConfigBundle {
    #[serde(rename = "configId")]
    pub config_id: String,
    pub domain: String,
    #[serde(rename = "configContent")]
    pub config_content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub version: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentCertificateBundle {
    #[serde(rename = "certificateId")]
    pub certificate_id: String,
    #[serde(rename = "certName")]
    pub cert_name: String,
    pub fingerprint: String,
    pub hostnames: Vec<String>,
    #[serde(rename = "fullchainPem")]
    pub fullchain_pem: String,
    #[serde(rename = "privkeyPem")]
    pub privkey_pem: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub id: String,
    pub action: String,
    pub resource: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AuditLogPage {
    pub items: Vec<AuditLogResponse>,
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    pub total: i64,
    pub page: i32,
    pub page_size: i32,
    /// Opaque keyset continuation for cursor mode; `None` in offset mode or on
    /// the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Exact page continuation flag for cursor mode; `None` in offset mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_totals_serialize_as_decimal_strings() {
        let page = ApplicationPage {
            items: Vec::new(),
            total: 1_234_567_890_123,
            page: 1,
            page_size: 20,
        };
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["total"], serde_json::json!("1234567890123"));
        assert_eq!(json["page"], serde_json::json!(1));
    }

    #[test]
    fn agent_nginx_config_bundle_version_round_trips_as_string() {
        let bundle = AgentNginxConfigBundle {
            config_id: "cfg-1".into(),
            domain: "example.com".into(),
            config_content: "server {}".into(),
            fingerprint: "abc".into(),
            version: 9_876_543_210_987,
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(json.contains(r#""version":"9876543210987""#));
        let parsed: AgentNginxConfigBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, bundle.version);
    }

    #[test]
    fn agent_heartbeat_request_optional_int64_round_trips() {
        let request = AgentHeartbeatRequest {
            agent_version: Some("0.1".into()),
            nginx_enabled: Some(true),
            active_configs: Some(42),
            last_sync_version: Some("v1".into()),
            certificate_observations: Vec::new(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""activeConfigs":"42""#));
        let parsed: AgentHeartbeatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.active_configs, Some(42));
    }

    #[test]
    fn rejects_non_numeric_int64_string_input() {
        let json = r#"{"items":[],"total":"not-a-number","page":1,"pageSize":20}"#;
        let result: Result<ApplicationPage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    /// A request that only names the required fields has to be acceptable: the
    /// defaults are the platform's answer to "the caller had no opinion", and a
    /// validator that refused them would make the documented defaults unusable.
    #[test]
    fn the_default_issue_request_is_acceptable() {
        let request = IssueCertificateRequest {
            domain_ids: vec!["domain-1".to_owned()],
            cert_type: 1,
            ..IssueCertificateRequest::default()
        };
        assert_eq!(certificate_issue_shape_error(&request), None);
        assert_eq!(
            request.certificate_scope, CERTIFICATE_SCOPE_SINGLE_DOMAIN,
            "the default scope is the narrow one; widening it is the operator's call"
        );
        assert_eq!(request.validation_method, CERTIFICATE_VALIDATION_AUTO);
        assert_eq!(
            request.renew_before_days,
            CERTIFICATE_DEFAULT_RENEW_BEFORE_DAYS
        );
        // The two fields whose absence is meaningful: absent means "no choice",
        // and the server resolves where the validator does not.
        assert_eq!(request.provider_account_id, None);
        assert_eq!(request.ca_profile, None);
    }

    /// `certType` has no defensible default, so the `Default` impl leaves it at
    /// a value the validator refuses rather than picking a CA for the caller.
    #[test]
    fn the_zero_cert_type_is_refused_not_defaulted() {
        let request = IssueCertificateRequest {
            domain_ids: vec!["domain-1".to_owned()],
            ..IssueCertificateRequest::default()
        };
        assert!(certificate_issue_shape_error(&request).is_some());
    }

    /// Every field this validator owns, one unacceptable value each.
    ///
    /// Written as a table because the failure mode being guarded against is a
    /// clause being dropped while the function still compiles: a per-field case
    /// that stops being checked has to turn exactly one row red.
    #[test]
    fn every_shape_rule_refuses_its_own_violation() {
        let acceptable = || IssueCertificateRequest {
            domain_ids: vec!["domain-1".to_owned()],
            cert_type: 1,
            ..IssueCertificateRequest::default()
        };
        let cases: Vec<(&str, IssueCertificateRequest)> = vec![
            (
                "certType outside 1|3",
                IssueCertificateRequest {
                    cert_type: 2,
                    ..acceptable()
                },
            ),
            (
                "self-signed with automatic renewal",
                IssueCertificateRequest {
                    cert_type: 3,
                    auto_renew: true,
                    ..acceptable()
                },
            ),
            (
                "unknown key algorithm",
                IssueCertificateRequest {
                    key_algorithm: "ED25519".to_owned(),
                    ..acceptable()
                },
            ),
            (
                "unknown scope",
                IssueCertificateRequest {
                    certificate_scope: "PER_HOST".to_owned(),
                    ..acceptable()
                },
            ),
            (
                "unknown validation method",
                IssueCertificateRequest {
                    validation_method: "TLS_ALPN_01".to_owned(),
                    ..acceptable()
                },
            ),
            (
                "a wildcard that would be authorized over HTTP-01",
                IssueCertificateRequest {
                    certificate_scope: CERTIFICATE_SCOPE_WILDCARD.to_owned(),
                    validation_method: CERTIFICATE_VALIDATION_HTTP_01.to_owned(),
                    ..acceptable()
                },
            ),
            (
                "empty certificate name",
                IssueCertificateRequest {
                    cert_name: Some(String::new()),
                    ..acceptable()
                },
            ),
            (
                "a certificate name past the column's width",
                IssueCertificateRequest {
                    cert_name: Some("n".repeat(CERTIFICATE_NAME_MAX_CHARS + 1)),
                    ..acceptable()
                },
            ),
            (
                "a certificate name carrying a newline",
                IssueCertificateRequest {
                    cert_name: Some("edge\nfront-door".to_owned()),
                    ..acceptable()
                },
            ),
            (
                "an unknown CA profile",
                IssueCertificateRequest {
                    ca_profile: Some("BUYPASS".to_owned()),
                    ..acceptable()
                },
            ),
            (
                "a CA profile on a self-signed request",
                IssueCertificateRequest {
                    cert_type: 3,
                    auto_renew: false,
                    ca_profile: Some(CERTIFICATE_CA_PROFILE_LETS_ENCRYPT_STAGING.to_owned()),
                    ..acceptable()
                },
            ),
            (
                "an account id carrying whitespace",
                IssueCertificateRequest {
                    provider_account_id: Some("aliyun prod".to_owned()),
                    ..acceptable()
                },
            ),
            (
                "an account id past the wire bound",
                IssueCertificateRequest {
                    provider_account_id: Some("a".repeat(CERTIFICATE_ACCOUNT_ID_MAX_CHARS + 1)),
                    ..acceptable()
                },
            ),
            (
                "a renewal window under the floor",
                IssueCertificateRequest {
                    renew_before_days: CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS - 1,
                    ..acceptable()
                },
            ),
            (
                "a renewal window over the ceiling",
                IssueCertificateRequest {
                    renew_before_days: CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS + 1,
                    ..acceptable()
                },
            ),
        ];
        for (reason, request) in cases {
            assert!(
                certificate_issue_shape_error(&request).is_some(),
                "expected a refusal for {reason}"
            );
        }
    }

    /// The two bounds are inclusive, and the boundary is where an off-by-one
    /// hides: `renewBeforeDays` is accepted at 7 and 90 and refused at 6 and 91.
    #[test]
    fn the_renewal_window_bounds_are_inclusive() {
        for days in [
            CERTIFICATE_MINIMUM_RENEW_BEFORE_DAYS,
            CERTIFICATE_MAXIMUM_RENEW_BEFORE_DAYS,
        ] {
            let request = IssueCertificateRequest {
                domain_ids: vec!["domain-1".to_owned()],
                cert_type: 1,
                renew_before_days: days,
                ..IssueCertificateRequest::default()
            };
            assert_eq!(certificate_issue_shape_error(&request), None, "{days} days");
        }
    }

    /// The identifier set is deliberately outside this function: it needs each
    /// row's stored `EXACT`/`WILDCARD` type, which only exists after the domains
    /// are read. A wildcard scope with no wildcard identifier is therefore *not*
    /// refused here — the check that does refuse it runs against the rows.
    #[test]
    fn the_identifier_set_is_not_this_function_s_job() {
        let request = IssueCertificateRequest {
            domain_ids: vec!["domain-1".to_owned()],
            cert_type: 1,
            certificate_scope: CERTIFICATE_SCOPE_WILDCARD.to_owned(),
            ..IssueCertificateRequest::default()
        };
        assert_eq!(certificate_issue_shape_error(&request), None);
    }
}
