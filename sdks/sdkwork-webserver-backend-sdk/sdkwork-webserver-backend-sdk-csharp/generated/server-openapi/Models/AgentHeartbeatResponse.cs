using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class AgentHeartbeatResponse
    {
        public string ServerId { get; set; }
        public int Status { get; set; }
        public string AcknowledgedAt { get; set; }
    }
}
