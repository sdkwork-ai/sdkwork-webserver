using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class MetricsWindowBounds
    {
        public string Window { get; set; }
        public string? DateFrom { get; set; }
        public string DateTo { get; set; }
    }
}
