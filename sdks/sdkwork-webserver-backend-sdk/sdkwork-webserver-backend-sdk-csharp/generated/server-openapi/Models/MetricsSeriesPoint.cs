using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsSeriesPoint
    {
        public string Date { get; set; }
        public string Quantity { get; set; }
    }
}
