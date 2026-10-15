using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TunnelMachineRouteDeclarationResponse
    {
        public string RouteId { get; set; }
        public bool Active { get; set; }
        public string? PublicUrl { get; set; }
        public string? Detail { get; set; }
    }
}
