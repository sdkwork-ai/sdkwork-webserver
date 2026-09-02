using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class NginxStatusResponse
    {
        public bool? Running { get; set; }
        public string? Version { get; set; }
        public int? Pid { get; set; }
        public int? ActiveConnections { get; set; }
        public string? ConfigPath { get; set; }
        public string? Uptime { get; set; }
    }
}
