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
        public bool? RoutingEnabled { get; set; }
        public bool? Draining { get; set; }
        public string? ProbeUrl { get; set; }
        public Dictionary<string, string>? Labels { get; set; }
        public int? RoutingWeight { get; set; }
        public string? MaintenanceNote { get; set; }
    }
}
