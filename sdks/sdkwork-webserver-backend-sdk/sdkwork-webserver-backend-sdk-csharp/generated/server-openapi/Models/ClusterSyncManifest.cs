using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ClusterSyncManifest
    {
        public string ClusterId { get; set; }
        public string Kind { get; set; }
        public string Revision { get; set; }
        public string Sha256 { get; set; }
        public Dictionary<string, object> Payload { get; set; }
        public string CreatedAt { get; set; }
    }
}
