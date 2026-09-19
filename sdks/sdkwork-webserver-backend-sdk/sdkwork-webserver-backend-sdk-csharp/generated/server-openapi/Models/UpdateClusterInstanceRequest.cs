using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdateClusterInstanceRequest
    {
        public string? Name { get; set; }
        public int? Status { get; set; }
        public string? PublicEndpoint { get; set; }
    }
}
