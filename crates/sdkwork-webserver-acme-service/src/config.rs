use std::path::Path;

use url::Url;

use crate::{AcmeServiceError, AcmeServiceResult};

const MAX_DIRECTORY_URL_BYTES: usize = 2_048;
const MAX_CONTACT_EMAIL_BYTES: usize = 254;
const MAX_WEBROOT_BYTES: usize = 4_096;

pub const DEFAULT_ACME_OPERATION_TIMEOUT_MS: u64 = 180_000;
pub const MIN_ACME_OPERATION_TIMEOUT_MS: u64 = 10_000;
pub const MAX_ACME_OPERATION_TIMEOUT_MS: u64 = 600_000;

/// Validated runtime ACME configuration.
#[derive(Clone, Debug)]
pub struct AcmeConfig {
    pub directory_url: String,
    pub contact_email: String,
    pub renew_before_days: u32,
    pub webroot: Option<String>,
    pub use_production: bool,
}

impl AcmeConfig {
    pub fn new(
        directory_url: String,
        contact_email: String,
        renew_before_days: u32,
        webroot: Option<String>,
        use_production: bool,
    ) -> AcmeServiceResult<Self> {
        validate_directory_url(&directory_url)?;
        validate_contact_email(&contact_email)?;
        if !(1..=90).contains(&renew_before_days) {
            return Err(AcmeServiceError::config(
                "certificate renewal window must be between 1 and 90 days",
            ));
        }
        if let Some(path) = webroot.as_deref() {
            validate_webroot(path)?;
        }
        Ok(Self {
            directory_url,
            contact_email,
            renew_before_days,
            webroot,
            use_production,
        })
    }

    pub fn validate(&self) -> AcmeServiceResult<()> {
        validate_directory_url(&self.directory_url)?;
        validate_contact_email(&self.contact_email)?;
        if !(1..=90).contains(&self.renew_before_days) {
            return Err(AcmeServiceError::config(
                "certificate renewal window must be between 1 and 90 days",
            ));
        }
        if let Some(path) = self.webroot.as_deref() {
            validate_webroot(path)?;
        }
        Ok(())
    }
}

fn validate_directory_url(value: &str) -> AcmeServiceResult<()> {
    if value.is_empty() || value.len() > MAX_DIRECTORY_URL_BYTES {
        return Err(AcmeServiceError::config(
            "ACME directory URL must contain 1..2048 bytes",
        ));
    }
    let url = Url::parse(value).map_err(|error| {
        AcmeServiceError::config(format!("invalid ACME directory URL: {error}"))
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AcmeServiceError::config(
            "ACME directory URL must be an HTTPS URL without userinfo",
        ));
    }
    Ok(())
}

fn validate_contact_email(value: &str) -> AcmeServiceResult<()> {
    if value.is_empty()
        || value.len() > MAX_CONTACT_EMAIL_BYTES
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(AcmeServiceError::config(
            "ACME contact email must contain 1..254 safe ASCII bytes",
        ));
    }
    let Some((local, domain)) = value.rsplit_once('@') else {
        return Err(AcmeServiceError::config("ACME contact email is invalid"));
    };
    if local.is_empty() || domain.is_empty() || domain.starts_with('.') || domain.ends_with('.') {
        return Err(AcmeServiceError::config("ACME contact email is invalid"));
    }
    Ok(())
}

fn validate_webroot(value: &str) -> AcmeServiceResult<()> {
    if value.is_empty()
        || value.len() > MAX_WEBROOT_BYTES
        || value
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
        || Path::new(value).as_os_str().is_empty()
    {
        return Err(AcmeServiceError::config(
            "ACME webroot must contain 1..4096 safe path bytes",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> AcmeServiceResult<AcmeConfig> {
        AcmeConfig::new(
            "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            Some("/var/www/acme".to_string()),
            false,
        )
    }

    #[test]
    fn typed_config_is_validated() {
        let config = config().expect("config");
        config.validate().expect("validate");
    }

    #[test]
    fn rejects_unbounded_or_unsafe_values() {
        assert!(AcmeConfig::new(
            "http://acme.invalid/directory".to_string(),
            "admin@example.com".to_string(),
            30,
            None,
            false,
        )
        .is_err());
        assert!(AcmeConfig::new(
            "https://acme.example/directory".to_string(),
            "invalid email".to_string(),
            0,
            None,
            false,
        )
        .is_err());
    }
}
