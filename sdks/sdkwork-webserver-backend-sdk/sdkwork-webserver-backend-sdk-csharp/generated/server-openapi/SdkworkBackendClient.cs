using System;
using SDKwork.Common.Core;
using SdkHttpClient = SDKWork.Webserver.BackendSdk.Http.HttpClient;
using SDKWork.Webserver.BackendSdk.Api;

namespace SDKWork.Webserver.BackendSdk
{
    public class SdkworkBackendClient
    {
        private readonly SdkHttpClient _httpClient;

        public ApplicationApi Application { get; }
        public ApplicationDomainApi ApplicationDomain { get; }
        public CertificateApi Certificate { get; }
        public DomainApi Domain { get; }
        public ApplicationSourceVersionApi ApplicationSourceVersion { get; }
        public ApplicationDeploymentApi ApplicationDeployment { get; }
        public CertificateDistributionApi CertificateDistribution { get; }
        public NginxApi Nginx { get; }
        public ServerApi Server { get; }
        public ServerFileApi ServerFile { get; }
        public AgentApi Agent { get; }
        public AuditApi Audit { get; }

        public SdkworkBackendClient(string baseUrl)
        {
            _httpClient = new SdkHttpClient(baseUrl);
            Application = new ApplicationApi(_httpClient);
            ApplicationDomain = new ApplicationDomainApi(_httpClient);
            Certificate = new CertificateApi(_httpClient);
            Domain = new DomainApi(_httpClient);
            ApplicationSourceVersion = new ApplicationSourceVersionApi(_httpClient);
            ApplicationDeployment = new ApplicationDeploymentApi(_httpClient);
            CertificateDistribution = new CertificateDistributionApi(_httpClient);
            Nginx = new NginxApi(_httpClient);
            Server = new ServerApi(_httpClient);
            ServerFile = new ServerFileApi(_httpClient);
            Agent = new AgentApi(_httpClient);
            Audit = new AuditApi(_httpClient);
        }

        public SdkworkBackendClient(SdkConfig config)
        {
            _httpClient = new SdkHttpClient(config);
            Application = new ApplicationApi(_httpClient);
            ApplicationDomain = new ApplicationDomainApi(_httpClient);
            Certificate = new CertificateApi(_httpClient);
            Domain = new DomainApi(_httpClient);
            ApplicationSourceVersion = new ApplicationSourceVersionApi(_httpClient);
            ApplicationDeployment = new ApplicationDeploymentApi(_httpClient);
            CertificateDistribution = new CertificateDistributionApi(_httpClient);
            Nginx = new NginxApi(_httpClient);
            Server = new ServerApi(_httpClient);
            ServerFile = new ServerFileApi(_httpClient);
            Agent = new AgentApi(_httpClient);
            Audit = new AuditApi(_httpClient);
        }
        public SdkworkBackendClient SetAuthToken(string token)
        {
            _httpClient.SetAuthToken(token);
            return this;
        }

        public SdkworkBackendClient SetAccessToken(string token)
        {
            _httpClient.SetAccessToken(token);
            return this;
        }

        public SdkworkBackendClient SetHeader(string key, string value)
        {
            _httpClient.SetHeader(key, value);
            return this;
        }
    }
}
