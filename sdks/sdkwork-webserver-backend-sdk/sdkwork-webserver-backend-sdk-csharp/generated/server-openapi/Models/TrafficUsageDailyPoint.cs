using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class TrafficUsageDailyPoint
    {
        public string UsageDate { get; set; }
        public string Dimension { get; set; }
        public string Quantity { get; set; }
    }
}
