using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerProjectOperations
    {
        public string NodeId { get; set; }
        public string Path { get; set; }
        public string ProjectType { get; set; }
        public List<ServerProjectOperation> Operations { get; set; }
    }
}
