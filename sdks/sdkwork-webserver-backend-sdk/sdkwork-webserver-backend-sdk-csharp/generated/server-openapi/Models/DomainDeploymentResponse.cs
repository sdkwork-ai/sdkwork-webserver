using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class DomainDeploymentResponse
    {
        public string Id { get; set; }
        public int Status { get; set; }
        public string Environment { get; set; }
        public string? VersionTag { get; set; }
        public string? CompletedAt { get; set; }
        public string CreatedAt { get; set; }
    }
}
