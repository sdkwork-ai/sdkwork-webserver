using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class ServerProjectOperation
    {
        public string Id { get; set; }
        public string Kind { get; set; }
        public string Label { get; set; }
        public string Permission { get; set; }
        public string? Description { get; set; }
        public bool? Dangerous { get; set; }
    }
}
