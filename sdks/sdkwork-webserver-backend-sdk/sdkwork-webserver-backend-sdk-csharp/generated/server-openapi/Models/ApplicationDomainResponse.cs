using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ApplicationDomainResponse
    {
        public string Id { get; set; }
        public string Hostname { get; set; }
        public string? RootDomainId { get; set; }
        public string? RecordName { get; set; }
        public string? ApplicationId { get; set; }
        public string? ApplicationName { get; set; }
        public string CertificateCount { get; set; }
        public bool IsPrimary { get; set; }
        public bool IsVerified { get; set; }
        public bool SslEnabled { get; set; }
        public string? SslProvider { get; set; }
        public int Status { get; set; }
        public DomainDeploymentResponse? LatestDeployment { get; set; }
        public string CreatedAt { get; set; }
        public string? UpdatedAt { get; set; }
    }
}
