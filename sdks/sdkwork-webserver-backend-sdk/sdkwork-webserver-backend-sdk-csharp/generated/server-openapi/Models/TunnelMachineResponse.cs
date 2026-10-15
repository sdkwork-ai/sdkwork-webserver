using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TunnelMachineResponse
    {
        public string DeviceId { get; set; }
        public string DeviceName { get; set; }
        public string Platform { get; set; }
        public bool Online { get; set; }
        public bool Enabled { get; set; }
        public string? SessionId { get; set; }
        public string FirstSeenAt { get; set; }
        public string LastSeenAt { get; set; }
        public string? ConnectedAt { get; set; }
        public string? DisconnectedAt { get; set; }
        public string? RemoteAddr { get; set; }
        public string BytesIn { get; set; }
        public string BytesOut { get; set; }
        public string SessionsTotal { get; set; }
        public string RouteCount { get; set; }
        public string HealthyRouteCount { get; set; }
        public List<string>? UnhealthyRouteIds { get; set; }
        public List<TunnelMachineRouteResponse>? Routes { get; set; }
    }
}
