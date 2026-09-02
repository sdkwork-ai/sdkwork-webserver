using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class NginxReloadResponse
    {
        public bool? Success { get; set; }
        public string? Message { get; set; }
        public string? Timestamp { get; set; }
    }
}
