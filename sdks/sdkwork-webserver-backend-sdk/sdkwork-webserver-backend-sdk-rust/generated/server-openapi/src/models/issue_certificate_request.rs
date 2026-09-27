use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct IssueCertificateRequest {
    /// Ordered exact or wildcard domain identifiers included in the certificate SAN extension.
    #[serde(rename = "domainIds")]
    pub domain_ids: Vec<String>,

    /// 1=Let's Encrypt, 3=self-signed. Custom import is a separate future workflow.
    #[serde(rename = "certType")]
    pub cert_type: i64,

    /// Key algorithm of the issued leaf. Defaults to RSA: a managed certificate is renewed unattended and RSA-2048 is the leaf key every TLS client accepts, so a certificate requested without an opinion on this stays reachable from old stacks as well. Ask for ECDSA explicitly where every client is known to support P-256.
    /// Named `keyAlgorithm` rather than the control plane's `preferredKeyAlgorithm`: the Web Server's field predates it and is served by twelve generated SDKs, so a rename would be a breaking change to a published request shape for no wire-level gain. The two carry the same two values and the consoles label both the localized and the English name of the field.
    #[serde(rename = "keyAlgorithm")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_algorithm: Option<String>,

    #[serde(rename = "autoRenew")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_renew: Option<bool>,

    /// Operator-facing name of the certificate. Optional: an unnamed request keeps the generated name, so an existing caller that does not send one is unaffected. Bounded at 200 because that is the width of `webserver_certificate.cert_name`.
    #[serde(rename = "certName")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert_name: Option<String>,

    /// What the identifier set is allowed to contain. `SINGLE_DOMAIN` admits only exact hostnames and refuses a wildcard; `WILDCARD` requires at least one wildcard and is refused together with `HTTP_01`, because a wildcard can only be authorized over DNS-01.
    /// Unlike the control plane's field of the same name, this does **not** plan the wildcard's apex in: the identifier set here is resolved from the served, verified domain rows the caller names, so adding an apex the caller did not name would attach a SAN to a hostname this edge has no verified row for. Where the apex should be covered, it is named as its own identifier.
    #[serde(rename = "certificateScope")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_scope: Option<String>,

    /// How the ACME challenge is answered. `AUTO` lets the edge choose from what it has configured; `HTTP_01` and `DNS_01` pin the method. `WILDCARD` coverage is refused with `HTTP_01`, because a wildcard can only be authorized over DNS-01 and the refusal is stated rather than silently upgraded.
    #[serde(rename = "validationMethod")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_method: Option<String>,

    /// How many days before expiry an unattended renewal is scheduled. Bounded at 7..90 to match the stored constraint, and only meaningful while `autoRenew` is true.
    #[serde(rename = "renewBeforeDays")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renew_before_days: Option<i64>,

    /// Which Let's Encrypt directory to order from. Only meaningful for `certType` 1; a self-signed request resolves to `SELF_SIGNED` on the server regardless of what is sent.
    #[serde(rename = "caProfile")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_profile: Option<String>,

    /// Which of the edge's configured DNS accounts publishes the DNS-01 challenge records. Optional: omitted means the edge resolves an account from the identifier set, and an id that the edge has no credentials for is refused rather than quietly replaced. The accounts that may be named are listed by `GET /backend/v3/api/dns_accounts`.
    /// The pattern bounds the id rather than shaping it: no surrounding whitespace and no control characters, because the id reaches a log line and a stored document. It deliberately says nothing about the id's structure: a DNS accounts file may spell an id in any alphabet, and a structural pattern here would refuse ids the edge is happily loading.
    #[serde(rename = "providerAccountId")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_account_id: Option<String>,
}
