using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class AgentHeartbeatRequest
    {
        public string? AgentVersion { get; set; }
        public bool? NginxEnabled { get; set; }
        public string? ActiveConfigs { get; set; }
        public string? LastSyncVersion { get; set; }
        public List<AgentCertificateObservation>? CertificateObservations { get; set; }
    }
}
