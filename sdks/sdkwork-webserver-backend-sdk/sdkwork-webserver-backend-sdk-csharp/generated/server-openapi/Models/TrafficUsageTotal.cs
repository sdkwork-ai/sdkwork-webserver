using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TrafficUsageTotal
    {
        public string Dimension { get; set; }
        public string Quantity { get; set; }
        public string Unit { get; set; }
    }
}
