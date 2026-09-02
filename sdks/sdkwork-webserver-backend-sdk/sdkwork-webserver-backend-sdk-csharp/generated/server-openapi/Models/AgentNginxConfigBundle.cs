using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class AgentNginxConfigBundle
    {
        public string ConfigId { get; set; }
        public string Domain { get; set; }
        public string ConfigContent { get; set; }
        public string Fingerprint { get; set; }
        public string Version { get; set; }
    }
}
