using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class CreateHealthCheckRequest
    {
        public int CheckType { get; set; }
        public string CheckUrl { get; set; }
        public int? CheckInterval { get; set; }
        public int? TimeoutMs { get; set; }
        public int? RetryCount { get; set; }
    }
}
