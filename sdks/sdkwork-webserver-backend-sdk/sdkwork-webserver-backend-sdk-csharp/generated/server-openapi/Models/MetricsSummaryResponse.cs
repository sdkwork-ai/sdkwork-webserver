using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsSummaryResponse
    {
        public string AsOf { get; set; }
        public bool PlatformScope { get; set; }
        public string? TrafficSince { get; set; }
        public List<MetricsWindowBounds> Windows { get; set; }
        public List<MetricsMetricTotals> Entities { get; set; }
        public List<MetricsMetricTotals> Traffic { get; set; }
        public List<MetricsMetricTotals> Storage { get; set; }
        public List<MetricsSeries> Series { get; set; }
        public MetricsSeriesWindow SeriesWindow { get; set; }
        public List<string> UnassembledMetrics { get; set; }
    }
}
