using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class SdkWorkAsyncData
    {
        public bool Accepted { get; set; }
        public string OperationId { get; set; }
        public string Status { get; set; }
        public string? PollUrl { get; set; }
    }
}
