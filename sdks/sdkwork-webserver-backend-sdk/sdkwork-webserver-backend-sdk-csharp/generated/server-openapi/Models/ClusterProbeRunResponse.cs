using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterProbeRunResponse
    {
        public bool Healthy { get; set; }
        public int LatencyMs { get; set; }
        public int Failures { get; set; }
        public bool Ejected { get; set; }
        public bool Recovered { get; set; }
    }
}
