using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateClusterRequest
    {
        public string Name { get; set; }
        public string Code { get; set; }
        public string? Description { get; set; }
        public int? HeartbeatIntervalSeconds { get; set; }
        public int? OfflineThresholdSeconds { get; set; }
    }
}
