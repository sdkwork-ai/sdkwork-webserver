using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TrafficUsageStatisticsResponse
    {
        public string DateFrom { get; set; }
        public string DateTo { get; set; }
        public bool PlatformScope { get; set; }
        public List<TrafficUsageTotal> Totals { get; set; }
        public List<TrafficUsageDailyPoint> Daily { get; set; }
        public List<TrafficUsageAppTotal> Apps { get; set; }
        public List<TrafficUsageTenantTotal> Tenants { get; set; }
    }
}
