using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class NginxDeployResponse
    {
        public bool? Success { get; set; }
        public string? ConfigId { get; set; }
        public string? DeployedAt { get; set; }
        public Dictionary<string, object>? ReloadResult { get; set; }
    }
}
