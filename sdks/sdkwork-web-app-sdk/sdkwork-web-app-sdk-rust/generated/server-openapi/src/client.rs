use std::sync::Arc;

use crate::api::{ApplicationApi, DomainApi, CertificateApi, SourceVersionApi, DeploymentApi, EnvVariableApi, MonitorApi};
use crate::http::{SdkworkConfig, SdkworkError, SdkworkHttpClient};

#[derive(Clone)]
pub struct SdkworkAppClient {
    http: Arc<SdkworkHttpClient>,
}

impl SdkworkAppClient {
    pub fn new(config: SdkworkConfig) -> Result<Self, SdkworkError> {
        Ok(Self {
            http: Arc::new(SdkworkHttpClient::new(config)?),
        })
    }

    pub fn new_with_base_url(base_url: impl Into<String>) -> Result<Self, SdkworkError> {
        Self::new(SdkworkConfig::new(base_url))
    }
    pub fn set_auth_token(&self, token: impl Into<String>) -> &Self {
        self.http.set_auth_token(token);
        self
    }

    pub fn set_access_token(&self, token: impl Into<String>) -> &Self {
        self.http.set_access_token(token);
        self
    }


    pub fn set_header(&self, key: impl Into<String>, value: impl Into<String>) -> &Self {
        self.http.set_header(key, value);
        self
    }

    pub fn http_client(&self) -> Arc<SdkworkHttpClient> {
        Arc::clone(&self.http)
    }

    pub fn application(&self) -> ApplicationApi {
            ApplicationApi::new(Arc::clone(&self.http))
        }

    pub fn domain(&self) -> DomainApi {
            DomainApi::new(Arc::clone(&self.http))
        }

    pub fn certificate(&self) -> CertificateApi {
            CertificateApi::new(Arc::clone(&self.http))
        }

    pub fn source_version(&self) -> SourceVersionApi {
            SourceVersionApi::new(Arc::clone(&self.http))
        }

    pub fn deployment(&self) -> DeploymentApi {
            DeploymentApi::new(Arc::clone(&self.http))
        }

    pub fn env_variable(&self) -> EnvVariableApi {
            EnvVariableApi::new(Arc::clone(&self.http))
        }

    pub fn monitor(&self) -> MonitorApi {
            MonitorApi::new(Arc::clone(&self.http))
        }
}
