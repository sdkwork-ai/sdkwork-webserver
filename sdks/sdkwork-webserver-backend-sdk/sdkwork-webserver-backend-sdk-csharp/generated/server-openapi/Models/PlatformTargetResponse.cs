using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.Webserver.BackendSdk.Models
{
    public class PlatformTargetResponse
    {
        public string? Id { get; set; }
        public string? AppId { get; set; }
        public string? TargetKey { get; set; }
        public string? Platform { get; set; }
        public string? TechStack { get; set; }
        public List<string>? Architectures { get; set; }
        public string? BundleId { get; set; }
        public string? PackageName { get; set; }
        public string? AppIdValue { get; set; }
        public string? BundleName { get; set; }
        public string? TargetStatus { get; set; }
        public string? CreatedAt { get; set; }
        public string? UpdatedAt { get; set; }
    }
}
