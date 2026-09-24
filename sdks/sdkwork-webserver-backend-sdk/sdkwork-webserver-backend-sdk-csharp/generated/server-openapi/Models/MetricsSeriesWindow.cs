using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsSeriesWindow
    {
        public string DateFrom { get; set; }
        public string DateTo { get; set; }
    }
}
