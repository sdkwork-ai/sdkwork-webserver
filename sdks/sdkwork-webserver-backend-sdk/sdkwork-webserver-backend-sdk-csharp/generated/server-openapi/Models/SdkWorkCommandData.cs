using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class SdkWorkCommandData
    {
        public bool Accepted { get; set; }
        public string? ResourceId { get; set; }
        public string? Status { get; set; }
    }
}
