using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class AgentHeartbeatResponse
    {
        public string ServerId { get; set; }
        public int Status { get; set; }
        public string AcknowledgedAt { get; set; }
    }
}
