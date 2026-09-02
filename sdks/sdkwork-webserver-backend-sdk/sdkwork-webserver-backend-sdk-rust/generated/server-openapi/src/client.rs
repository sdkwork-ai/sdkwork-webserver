use std::sync::Arc;

use crate::api::{ApplicationApi, ApplicationDomainApi, CertificateApi, DomainApi, ApplicationSourceVersionApi, ApplicationDeploymentApi, CertificateDistributionApi, NginxApi, ServerApi, ServerFileApi, AgentApi, AuditApi};
use crate::http::{SdkworkConfig, SdkworkError, SdkworkHttpClient};

#[derive(Clone)]
pub struct SdkworkBackendClient {
    http: Arc<SdkworkHttpClient>,
}

impl SdkworkBackendClient {
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

    pub fn set_agent_token(&self, token: impl Into<String>) -> &Self {
        self.http.set_agent_token(token);
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

    pub fn application_domain(&self) -> ApplicationDomainApi {
            ApplicationDomainApi::new(Arc::clone(&self.http))
        }

    pub fn certificate(&self) -> CertificateApi {
            CertificateApi::new(Arc::clone(&self.http))
        }

    pub fn domain(&self) -> DomainApi {
            DomainApi::new(Arc::clone(&self.http))
        }

    pub fn application_source_version(&self) -> ApplicationSourceVersionApi {
            ApplicationSourceVersionApi::new(Arc::clone(&self.http))
        }

    pub fn application_deployment(&self) -> ApplicationDeploymentApi {
            ApplicationDeploymentApi::new(Arc::clone(&self.http))
        }

    pub fn certificate_distribution(&self) -> CertificateDistributionApi {
            CertificateDistributionApi::new(Arc::clone(&self.http))
        }

    pub fn nginx(&self) -> NginxApi {
            NginxApi::new(Arc::clone(&self.http))
        }

    pub fn server(&self) -> ServerApi {
            ServerApi::new(Arc::clone(&self.http))
        }

    pub fn server_file(&self) -> ServerFileApi {
            ServerFileApi::new(Arc::clone(&self.http))
        }

    pub fn agent(&self) -> AgentApi {
            AgentApi::new(Arc::clone(&self.http))
        }

    pub fn audit(&self) -> AuditApi {
            AuditApi::new(Arc::clone(&self.http))
        }
}
