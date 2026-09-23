using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TrafficUsageAppTotal
    {
        public string? AppUuid { get; set; }
        public string? AppSlug { get; set; }
        public string Dimension { get; set; }
        public string Quantity { get; set; }
        public string Unit { get; set; }
    }
}
