using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace SDKWork.WebserverBackendSdk.Models
{
    public class PluginResponse
    {
        public string Id { get; set; }
        public string PluginKey { get; set; }
        public string DisplayName { get; set; }
        public string Summary { get; set; }
        public string Version { get; set; }
        public string CategoryId { get; set; }
        public List<string> SupportedHostTools { get; set; }
        public List<string> ContributedCapabilities { get; set; }
        public string SourceKind { get; set; }
        public string? GitRepository { get; set; }
        public string? GitRef { get; set; }
        public string? ArtifactRef { get; set; }
        public string? ChecksumSha256 { get; set; }
        public string? ArchiveFileName { get; set; }
        public string Status { get; set; }
        public string CreatedAt { get; set; }
        public string UpdatedAt { get; set; }
    }
}
