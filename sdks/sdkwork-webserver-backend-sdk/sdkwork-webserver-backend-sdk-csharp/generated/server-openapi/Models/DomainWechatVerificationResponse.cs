using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class DomainWechatVerificationResponse
    {
        public string RootDomainId { get; set; }
        public string? FileName { get; set; }
        public string? Content { get; set; }
        public string? UpdatedAt { get; set; }
    }
}
