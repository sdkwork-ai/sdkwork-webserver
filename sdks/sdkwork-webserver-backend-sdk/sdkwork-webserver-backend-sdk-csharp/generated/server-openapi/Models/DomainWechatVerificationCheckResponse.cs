using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DomainWechatVerificationCheckResponse
    {
        public bool Reachable { get; set; }
        public bool Matched { get; set; }
        public int? StatusCode { get; set; }
        public string? Scheme { get; set; }
        public string? Detail { get; set; }
        public string CheckedAt { get; set; }
    }
}
