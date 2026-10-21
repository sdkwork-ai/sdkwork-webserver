using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class NginxConfigRevisionSummary
    {
        public int? VersionNo { get; set; }
        public string? ConfigName { get; set; }
        public string? ConfigHash { get; set; }
        public string? CreatedBy { get; set; }
        public string? CreatedAt { get; set; }
    }
}
