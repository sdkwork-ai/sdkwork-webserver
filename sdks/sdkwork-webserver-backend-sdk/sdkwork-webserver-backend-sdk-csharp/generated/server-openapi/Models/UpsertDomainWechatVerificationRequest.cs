using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpsertDomainWechatVerificationRequest
    {
        public string FileName { get; set; }
        public string Content { get; set; }
    }
}
