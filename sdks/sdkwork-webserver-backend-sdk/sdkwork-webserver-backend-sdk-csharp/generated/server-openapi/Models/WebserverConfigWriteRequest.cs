using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class WebserverConfigWriteRequest
    {
        public string Content { get; set; }
        public string? ExpectedSha256 { get; set; }
    }
}
