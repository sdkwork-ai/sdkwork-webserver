using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class PublishClusterSyncRequest
    {
        public string Kind { get; set; }
        public Dictionary<string, object> Payload { get; set; }
    }
}
