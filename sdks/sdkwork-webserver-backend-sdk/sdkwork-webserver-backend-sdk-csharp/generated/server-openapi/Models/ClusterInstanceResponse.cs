using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterInstanceResponse
    {
        public string Id { get; set; }
        public string ClusterId { get; set; }
        public string HostId { get; set; }
        public string? HostName { get; set; }
        public string Name { get; set; }
        public string Role { get; set; }
        public string Environment { get; set; }
        public int? ProcessPid { get; set; }
        public string? ProcessStartedAt { get; set; }
        public string? BindHost { get; set; }
        public int? BindPort { get; set; }
        public string? PublicEndpoint { get; set; }
        public string? BuildVersion { get; set; }
        public int Status { get; set; }
        public string HealthState { get; set; }
        public string? LastHeartbeatAt { get; set; }
        public string? LastOnlineAt { get; set; }
        public string UptimeSeconds { get; set; }
        public Dictionary<string, object> Metrics { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
