using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class CreateTunnelMachineRouteRequest
    {
        public string Name { get; set; }
        public string Protocol { get; set; }
        public string? Domain { get; set; }
        public int? Port { get; set; }
        public bool? AllowPublic { get; set; }
        public List<string>? AllowedIps { get; set; }
        public string? Auth { get; set; }
        public List<string>? VisitorTokens { get; set; }
        public List<string>? Locations { get; set; }
        public string? BandwidthLimitBytesPerSecond { get; set; }
        public string? ProxyProtocol { get; set; }
        public string? LoadBalancerGroup { get; set; }
    }
}
