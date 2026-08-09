from .http_client import HttpClient, SdkConfig
from .api.application import ApplicationApi
from .api.domain import DomainApi
from .api.certificate import CertificateApi
from .api.source_version import SourceVersionApi
from .api.deployment import DeploymentApi
from .api.env_variable import EnvVariableApi
from .api.monitor import MonitorApi


class SdkworkAppClient:
    """sdkwork-web-app-sdk SDK Client."""

    def __init__(self, config: SdkConfig):
        self._client = HttpClient(config)
        self.application: ApplicationApi
        self.domain: DomainApi
        self.certificate: CertificateApi
        self.source_version: SourceVersionApi
        self.deployment: DeploymentApi
        self.env_variable: EnvVariableApi
        self.monitor: MonitorApi

        # Initialize API modules
        self.application = ApplicationApi(self._client)
        self.domain = DomainApi(self._client)
        self.certificate = CertificateApi(self._client)
        self.source_version = SourceVersionApi(self._client)
        self.deployment = DeploymentApi(self._client)
        self.env_variable = EnvVariableApi(self._client)
        self.monitor = MonitorApi(self._client)
    def set_auth_token(self, token: str) -> 'SdkworkAppClient':
        """Set auth token for authentication."""
        self._client.set_auth_token(token)
        return self

    def set_access_token(self, token: str) -> 'SdkworkAppClient':
        """Set access token for authentication."""
        self._client.set_access_token(token)
        return self

    def set_header(self, key: str, value: str) -> 'SdkworkAppClient':
        """Set custom header."""
        self._client.set_header(key, value)
        return self

    @property
    def http(self) -> HttpClient:
        """Get the underlying HTTP client."""
        return self._client


def create_client(config: SdkConfig) -> SdkworkAppClient:
    """Create a new SDK client instance."""
    return SdkworkAppClient(config)
