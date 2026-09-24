using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsSeries
    {
        public string Metric { get; set; }
        public string Unit { get; set; }
        public List<MetricsSeriesPoint> Points { get; set; }
    }
}
