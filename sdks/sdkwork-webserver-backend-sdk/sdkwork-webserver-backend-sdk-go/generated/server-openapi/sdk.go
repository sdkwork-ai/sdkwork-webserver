package backend

import (
    "github.com/sdkwork/sdkwork-webserver-backend-sdk/api"
    sdkhttp "github.com/sdkwork/sdkwork-webserver-backend-sdk/http"
)

type SdkworkBackendClient struct {
    http *sdkhttp.Client
    Application *api.ApplicationApi
    ApplicationDomain *api.ApplicationDomainApi
    Certificate *api.CertificateApi
    Domain *api.DomainApi
    ApplicationSourceVersion *api.ApplicationSourceVersionApi
    ApplicationDeployment *api.ApplicationDeploymentApi
    CertificateDistribution *api.CertificateDistributionApi
    Nginx *api.NginxApi
    Server *api.ServerApi
    ServerFile *api.ServerFileApi
    Agent *api.AgentApi
    Audit *api.AuditApi
}

func NewSdkworkBackendClient(baseURL string) *SdkworkBackendClient {
    cfg := sdkhttp.NewDefaultConfig(baseURL)
    return NewSdkworkBackendClientWithConfig(cfg)
}

func NewSdkworkBackendClientWithConfig(config sdkhttp.Config) *SdkworkBackendClient {
    client := sdkhttp.NewClient(config)
    return &SdkworkBackendClient{
        http: client,
        Application: api.NewApplicationApi(client),
        ApplicationDomain: api.NewApplicationDomainApi(client),
        Certificate: api.NewCertificateApi(client),
        Domain: api.NewDomainApi(client),
        ApplicationSourceVersion: api.NewApplicationSourceVersionApi(client),
        ApplicationDeployment: api.NewApplicationDeploymentApi(client),
        CertificateDistribution: api.NewCertificateDistributionApi(client),
        Nginx: api.NewNginxApi(client),
        Server: api.NewServerApi(client),
        ServerFile: api.NewServerFileApi(client),
        Agent: api.NewAgentApi(client),
        Audit: api.NewAuditApi(client),
    }
}

func (c *SdkworkBackendClient) SetAuthToken(token string) *SdkworkBackendClient {
    c.http.SetAuthToken(token)
    return c
}

func (c *SdkworkBackendClient) SetAccessToken(token string) *SdkworkBackendClient {
    c.http.SetAccessToken(token)
    return c
}

func (c *SdkworkBackendClient) SetHeader(key string, value string) *SdkworkBackendClient {
    c.http.SetHeader(key, value)
    return c
}

func (c *SdkworkBackendClient) Http() *sdkhttp.Client {
    return c.http
}
