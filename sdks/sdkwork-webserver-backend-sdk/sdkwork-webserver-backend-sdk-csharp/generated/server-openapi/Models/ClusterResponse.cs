using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterResponse
    {
        public string Id { get; set; }
        public string Name { get; set; }
        public string Code { get; set; }
        public string? Description { get; set; }
        public int Status { get; set; }
        public int HeartbeatIntervalSeconds { get; set; }
        public int OfflineThresholdSeconds { get; set; }
        public string HostCount { get; set; }
        public string InstanceCount { get; set; }
        public string OnlineInstanceCount { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
