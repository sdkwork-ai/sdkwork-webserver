using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateClusterHostRequest
    {
        public string? Name { get; set; }
        public string? ClusterId { get; set; }
    }
}
