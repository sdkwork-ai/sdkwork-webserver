namespace SDKWork.Webserver.AppSdk.Api
{
    /// <summary>
    /// API modules for sdkwork-webserver-app-sdk
    /// </summary>
    public static class Api
    {
        public static ApplicationApi? Application { get; set; }
        public static DomainApi? Domain { get; set; }
        public static CertificateApi? Certificate { get; set; }
        public static SourceVersionApi? SourceVersion { get; set; }
        public static DeploymentApi? Deployment { get; set; }
        public static EnvVariableApi? EnvVariable { get; set; }
        public static MonitorApi? Monitor { get; set; }
    }
}
