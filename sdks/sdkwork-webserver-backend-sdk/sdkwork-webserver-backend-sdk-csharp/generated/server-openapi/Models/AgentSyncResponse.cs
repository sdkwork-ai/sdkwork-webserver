using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class AgentSyncResponse
    {
        public string ServerId { get; set; }
        public string SyncVersion { get; set; }
        public bool Unchanged { get; set; }
        public List<AgentNginxConfigBundle> NginxConfigs { get; set; }
        public List<AgentCertificateBundle> Certificates { get; set; }
    }
}
