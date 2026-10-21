using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class UpdatePluginRequest
    {
        public string? DisplayName { get; set; }
        public string? Summary { get; set; }
        public string? Version { get; set; }
        public string? CategoryId { get; set; }
        public List<string>? SupportedHostTools { get; set; }
        public List<string>? ContributedCapabilities { get; set; }
        public string? GitRepository { get; set; }
        public string? GitRef { get; set; }
        public string? ArtifactRef { get; set; }
        public string? ChecksumSha256 { get; set; }
        public string? ArchiveFileName { get; set; }
        public string? Status { get; set; }
    }
}
