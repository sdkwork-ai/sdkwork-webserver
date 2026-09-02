using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class HealthCheckResponse
    {
        public string Id { get; set; }
        public int CheckType { get; set; }
        public string CheckUrl { get; set; }
        public int CheckInterval { get; set; }
        public int TimeoutMs { get; set; }
        public int RetryCount { get; set; }
        public int Status { get; set; }
        public string CreatedAt { get; set; }
    }
}
