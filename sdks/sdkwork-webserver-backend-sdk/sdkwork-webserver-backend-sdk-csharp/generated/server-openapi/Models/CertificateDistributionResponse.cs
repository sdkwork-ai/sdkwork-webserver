using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CertificateDistributionResponse
    {
        public string ServerId { get; set; }
        public string ServerName { get; set; }
        public string Host { get; set; }
        public string DesiredSyncVersion { get; set; }
        public string? AppliedSyncVersion { get; set; }
        public string Status { get; set; }
        public string? LastHeartbeatAt { get; set; }
    }
}
