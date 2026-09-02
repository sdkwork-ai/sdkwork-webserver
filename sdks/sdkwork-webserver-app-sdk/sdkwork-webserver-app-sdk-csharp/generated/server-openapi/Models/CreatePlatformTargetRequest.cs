using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverAppSdk.Models
{
    public class CreatePlatformTargetRequest
    {
        public string TargetKey { get; set; }
        public string Platform { get; set; }
        public string? TechStack { get; set; }
        public List<string>? Architectures { get; set; }
        public string? BundleId { get; set; }
        public string? PackageName { get; set; }
        public string? AppId { get; set; }
        public string? BundleName { get; set; }
        public List<string>? AllowedChannels { get; set; }
    }
}
