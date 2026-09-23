using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateClusterRequest
    {
        public string? Name { get; set; }
        public string? Description { get; set; }
        public int? Status { get; set; }
        public int? HeartbeatIntervalSeconds { get; set; }
        public int? OfflineThresholdSeconds { get; set; }
        public string? LbStrategy { get; set; }
        public List<string>? ServedDomains { get; set; }
    }
}
