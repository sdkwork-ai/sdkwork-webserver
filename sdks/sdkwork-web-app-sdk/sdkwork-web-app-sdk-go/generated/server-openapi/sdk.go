package app

import (
    "github.com/sdkwork/sdkwork-web-app-sdk/api"
    sdkhttp "github.com/sdkwork/sdkwork-web-app-sdk/http"
)

type SdkworkAppClient struct {
    http *sdkhttp.Client
    Application *api.ApplicationApi
    Domain *api.DomainApi
    Certificate *api.CertificateApi
    SourceVersion *api.SourceVersionApi
    Deployment *api.DeploymentApi
    EnvVariable *api.EnvVariableApi
    Monitor *api.MonitorApi
}

func NewSdkworkAppClient(baseURL string) *SdkworkAppClient {
    cfg := sdkhttp.NewDefaultConfig(baseURL)
    return NewSdkworkAppClientWithConfig(cfg)
}

func NewSdkworkAppClientWithConfig(config sdkhttp.Config) *SdkworkAppClient {
    client := sdkhttp.NewClient(config)
    return &SdkworkAppClient{
        http: client,
        Application: api.NewApplicationApi(client),
        Domain: api.NewDomainApi(client),
        Certificate: api.NewCertificateApi(client),
        SourceVersion: api.NewSourceVersionApi(client),
        Deployment: api.NewDeploymentApi(client),
        EnvVariable: api.NewEnvVariableApi(client),
        Monitor: api.NewMonitorApi(client),
    }
}

func (c *SdkworkAppClient) SetAuthToken(token string) *SdkworkAppClient {
    c.http.SetAuthToken(token)
    return c
}

func (c *SdkworkAppClient) SetAccessToken(token string) *SdkworkAppClient {
    c.http.SetAccessToken(token)
    return c
}

func (c *SdkworkAppClient) SetHeader(key string, value string) *SdkworkAppClient {
    c.http.SetHeader(key, value)
    return c
}

func (c *SdkworkAppClient) Http() *sdkhttp.Client {
    return c.http
}
