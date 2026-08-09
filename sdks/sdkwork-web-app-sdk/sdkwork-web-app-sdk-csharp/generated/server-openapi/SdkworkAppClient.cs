using System;
using SDKwork.Common.Core;
using SdkHttpClient = SDKWork.Web.AppSdk.Http.HttpClient;
using SDKWork.Web.AppSdk.Api;

namespace SDKWork.Web.AppSdk
{
    public class SdkworkAppClient
    {
        private readonly SdkHttpClient _httpClient;

        public ApplicationApi Application { get; }
        public DomainApi Domain { get; }
        public CertificateApi Certificate { get; }
        public SourceVersionApi SourceVersion { get; }
        public DeploymentApi Deployment { get; }
        public EnvVariableApi EnvVariable { get; }
        public MonitorApi Monitor { get; }

        public SdkworkAppClient(string baseUrl)
        {
            _httpClient = new SdkHttpClient(baseUrl);
            Application = new ApplicationApi(_httpClient);
            Domain = new DomainApi(_httpClient);
            Certificate = new CertificateApi(_httpClient);
            SourceVersion = new SourceVersionApi(_httpClient);
            Deployment = new DeploymentApi(_httpClient);
            EnvVariable = new EnvVariableApi(_httpClient);
            Monitor = new MonitorApi(_httpClient);
        }

        public SdkworkAppClient(SdkConfig config)
        {
            _httpClient = new SdkHttpClient(config);
            Application = new ApplicationApi(_httpClient);
            Domain = new DomainApi(_httpClient);
            Certificate = new CertificateApi(_httpClient);
            SourceVersion = new SourceVersionApi(_httpClient);
            Deployment = new DeploymentApi(_httpClient);
            EnvVariable = new EnvVariableApi(_httpClient);
            Monitor = new MonitorApi(_httpClient);
        }
        public SdkworkAppClient SetAuthToken(string token)
        {
            _httpClient.SetAuthToken(token);
            return this;
        }

        public SdkworkAppClient SetAccessToken(string token)
        {
            _httpClient.SetAccessToken(token);
            return this;
        }

        public SdkworkAppClient SetHeader(string key, string value)
        {
            _httpClient.SetHeader(key, value);
            return this;
        }
    }
}
