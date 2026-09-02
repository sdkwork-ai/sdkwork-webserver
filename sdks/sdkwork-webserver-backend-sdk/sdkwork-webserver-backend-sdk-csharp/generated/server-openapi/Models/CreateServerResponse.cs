using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class CreateServerResponse
    {
        public string Id { get; set; }
        public string Name { get; set; }
        public string Host { get; set; }
        public string TenantScopeHash { get; set; }
        public int SshPort { get; set; }
        public int Status { get; set; }
        public string? LastHeartbeatAt { get; set; }
        public string CreatedAt { get; set; }
        public string AgentToken { get; set; }
    }
}
