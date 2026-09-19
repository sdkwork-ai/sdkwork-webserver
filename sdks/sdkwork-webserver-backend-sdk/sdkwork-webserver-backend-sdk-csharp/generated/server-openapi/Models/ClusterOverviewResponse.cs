using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterOverviewResponse
    {
        public string TotalHosts { get; set; }
        public string OnlineHosts { get; set; }
        public string TotalInstances { get; set; }
        public string OnlineInstances { get; set; }
        public string UnhealthyInstances { get; set; }
        public string PendingPeerMessages { get; set; }
        public string GeneratedAt { get; set; }
    }
}
