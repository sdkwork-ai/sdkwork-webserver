using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class RootDomainResponse
    {
        public string Id { get; set; }
        public string Hostname { get; set; }
        public int Status { get; set; }
        public string SubdomainCount { get; set; }
        public string BoundSubdomainCount { get; set; }
        public string VerifiedSubdomainCount { get; set; }
        public string HttpsSubdomainCount { get; set; }
        public string ActiveDeploymentCount { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
