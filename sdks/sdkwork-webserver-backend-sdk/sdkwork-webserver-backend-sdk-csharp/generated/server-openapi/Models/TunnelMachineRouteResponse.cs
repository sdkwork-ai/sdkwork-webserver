using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TunnelMachineRouteResponse
    {
        public string RouteId { get; set; }
        public string Name { get; set; }
        public string Protocol { get; set; }
        public string? Domain { get; set; }
        public int? Port { get; set; }
        public string? SecretName { get; set; }
        public List<string>? Locations { get; set; }
        public string? LoadBalancerGroup { get; set; }
        public string? BandwidthLimitBytesPerSecond { get; set; }
        public string? ProxyProtocol { get; set; }
        public bool AllowPublic { get; set; }
        public string Auth { get; set; }
        public List<string>? AllowedIps { get; set; }
        public bool Healthy { get; set; }
    }
}
