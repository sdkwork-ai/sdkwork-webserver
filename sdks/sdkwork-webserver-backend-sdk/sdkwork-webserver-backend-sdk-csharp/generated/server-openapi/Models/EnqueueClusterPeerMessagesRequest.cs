using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class EnqueueClusterPeerMessagesRequest
    {
        public string ClusterId { get; set; }
        public string? ToInstanceId { get; set; }
        public string? FromInstanceId { get; set; }
        public string MessageType { get; set; }
        public Dictionary<string, object>? Payload { get; set; }
        public int? ExpiresInSeconds { get; set; }
    }
}
