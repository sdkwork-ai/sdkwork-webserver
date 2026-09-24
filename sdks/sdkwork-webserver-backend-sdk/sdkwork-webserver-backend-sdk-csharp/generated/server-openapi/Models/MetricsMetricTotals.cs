using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsMetricTotals
    {
        public string Metric { get; set; }
        public string Unit { get; set; }
        public List<MetricsWindowValue> Values { get; set; }
    }
}
