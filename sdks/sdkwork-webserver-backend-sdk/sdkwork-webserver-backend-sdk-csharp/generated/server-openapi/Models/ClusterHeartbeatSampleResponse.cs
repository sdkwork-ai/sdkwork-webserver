using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterHeartbeatSampleResponse
    {
        public string Id { get; set; }
        public int Status { get; set; }
        public int? LatencyMs { get; set; }
        public Dictionary<string, object> Metrics { get; set; }
        public string ReportedAt { get; set; }
    }
}
