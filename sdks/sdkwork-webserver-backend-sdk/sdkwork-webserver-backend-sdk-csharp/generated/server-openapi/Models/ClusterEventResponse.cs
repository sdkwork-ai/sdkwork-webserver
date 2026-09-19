using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterEventResponse
    {
        public string Id { get; set; }
        public string ClusterId { get; set; }
        public string? HostId { get; set; }
        public string? InstanceId { get; set; }
        public string EventType { get; set; }
        public string Severity { get; set; }
        public string Message { get; set; }
        public Dictionary<string, object> Detail { get; set; }
        public string OccurredAt { get; set; }
        public string CreatedAt { get; set; }
    }
}
