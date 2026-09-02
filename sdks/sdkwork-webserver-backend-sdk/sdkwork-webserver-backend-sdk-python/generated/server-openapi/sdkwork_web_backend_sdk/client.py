from .http_client import HttpClient, SdkConfig
from .api.application import ApplicationApi
from .api.application_domain import ApplicationDomainApi
from .api.certificate import CertificateApi
from .api.domain import DomainApi
from .api.application_source_version import ApplicationSourceVersionApi
from .api.application_deployment import ApplicationDeploymentApi
from .api.certificate_distribution import CertificateDistributionApi
from .api.nginx import NginxApi
from .api.server import ServerApi
from .api.server_file import ServerFileApi
from .api.agent import AgentApi
from .api.audit import AuditApi


class SdkworkBackendClient:
    """sdkwork-webserver-backend-sdk SDK Client."""

    def __init__(self, config: SdkConfig):
        self._client = HttpClient(config)
        self.application: ApplicationApi
        self.application_domain: ApplicationDomainApi
        self.certificate: CertificateApi
        self.domain: DomainApi
        self.application_source_version: ApplicationSourceVersionApi
        self.application_deployment: ApplicationDeploymentApi
        self.certificate_distribution: CertificateDistributionApi
        self.nginx: NginxApi
        self.server: ServerApi
        self.server_file: ServerFileApi
        self.agent: AgentApi
        self.audit: AuditApi

        # Initialize API modules
        self.application = ApplicationApi(self._client)
        self.application_domain = ApplicationDomainApi(self._client)
        self.certificate = CertificateApi(self._client)
        self.domain = DomainApi(self._client)
        self.application_source_version = ApplicationSourceVersionApi(self._client)
        self.application_deployment = ApplicationDeploymentApi(self._client)
        self.certificate_distribution = CertificateDistributionApi(self._client)
        self.nginx = NginxApi(self._client)
        self.server = ServerApi(self._client)
        self.server_file = ServerFileApi(self._client)
        self.agent = AgentApi(self._client)
        self.audit = AuditApi(self._client)
    def set_auth_token(self, token: str) -> 'SdkworkBackendClient':
        """Set auth token for authentication."""
        self._client.set_auth_token(token)
        return self

    def set_access_token(self, token: str) -> 'SdkworkBackendClient':
        """Set access token for authentication."""
        self._client.set_access_token(token)
        return self

    def set_header(self, key: str, value: str) -> 'SdkworkBackendClient':
        """Set custom header."""
        self._client.set_header(key, value)
        return self

    @property
    def http(self) -> HttpClient:
        """Get the underlying HTTP client."""
        return self._client


def create_client(config: SdkConfig) -> SdkworkBackendClient:
    """Create a new SDK client instance."""
    return SdkworkBackendClient(config)
