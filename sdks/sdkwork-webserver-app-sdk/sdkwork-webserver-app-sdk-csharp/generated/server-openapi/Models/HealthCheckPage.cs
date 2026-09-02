using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class HealthCheckPage
    {
        public List<HealthCheckResponse>? Items { get; set; }
        public string? Total { get; set; }
    }
}
